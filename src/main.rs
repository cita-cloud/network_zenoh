// Copyright Rivtower Technologies LLC.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod codec;
mod config;
mod dispatcher;
mod grpc_server;
mod health_check;
mod panic_hook;
mod peer;
mod util;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use bytes::BytesMut;
use cita_cloud_proto::{
    health_check::health_server::HealthServer,
    network::{network_service_server::NetworkServiceServer, NetworkMsg},
};
use clap::Parser;
use flume::bounded;
use log::{debug, error, info};
use panic_hook::set_panic_handler;
use parking_lot::RwLock;
use prost::Message;
use util::write_to_file;
use zenoh::{
    config::{EndPoint, QoSConf},
    prelude::*,
};

use crate::{
    config::NetworkConfig, dispatcher::NetworkMsgDispatcher,
    grpc_server::CitaCloudNetworkServiceServer, health_check::HealthCheckServer, peer::PeersManger,
};

fn main() {
    set_panic_handler();
    let opts: Opts = Opts::parse();
    // You can handle information about subcommands by requesting their matches by name
    // (as below), requesting just the name used, or both at the same time
    match opts.subcmd {
        SubCommand::Run(opts) => {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(run(opts));
        }
    }
}

/// This doc string acts as a help message when the user runs '--help'
/// as do all doc strings on fields
#[derive(Parser)]
#[clap(version, author)]
struct Opts {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    /// run this service
    #[clap(name = "run")]
    Run(RunOpts),
}

/// A subcommand for run
#[derive(Parser)]
struct RunOpts {
    /// Chain config path
    #[clap(short = 'c', long = "config", default_value = "config.toml")]
    config_path: String,
    /// log config path
    #[clap(short = 'l', long = "log", default_value = "network-log4rs.yaml")]
    log_file: String,
}

async fn run(opts: RunOpts) {
    ::std::env::set_var("RUST_BACKTRACE", "full");

    // read config.toml
    let config = NetworkConfig::new(&opts.config_path);

    // init log4rs
    log4rs::init_file(&opts.log_file, Default::default())
        .map_err(|e| println!("log init err: {}", e))
        .unwrap();
    info!("start network zenoh");
    let grpc_port = config.grpc_port.to_string();
    info!("grpc port of this service: {}", &grpc_port);

    // inbound_msg
    let (inbound_msg_tx, inbound_msg_rx) = bounded(1024);

    // outbound_msg
    let (outbound_msg_tx, outbound_msg_rx) = bounded(1024);

    // dispatcher run
    let dispatch_table = Arc::new(RwLock::new(HashMap::new()));
    let dispatcher = NetworkMsgDispatcher {
        dispatch_table: dispatch_table.clone(),
        inbound_msg_rx,
    };
    tokio::spawn(async move {
        dispatcher.run().await;
    });

    // knownpeers
    let mut peers_map = HashMap::new();
    for peer in &config.peers {
        peers_map.insert(peer.domain.to_string(), peer.clone());
    }

    let peers = Arc::new(RwLock::new(PeersManger::new(peers_map)));

    // grpc server
    let network_svc = CitaCloudNetworkServiceServer {
        dispatch_table,
        peers: peers.clone(),
        inbound_msg_tx: inbound_msg_tx.clone(),
        outbound_msg_tx,
        chain_origin: config.get_chain_origin(),
    };
    let grpc_addr = format!("0.0.0.0:{}", grpc_port).parse().unwrap();
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(NetworkServiceServer::new(network_svc))
            .add_service(HealthServer::new(HealthCheckServer {}))
            .serve(grpc_addr)
            .await
            .unwrap();
    });

    // run zenoh peer
    zenoh_serve(peers, config, inbound_msg_tx, outbound_msg_rx).await
}

async fn zenoh_serve(
    peers: Arc<RwLock<PeersManger>>,
    config: NetworkConfig,
    inbound_msg_tx: flume::Sender<NetworkMsg>,
    outbound_msg_rx: flume::Receiver<NetworkMsg>,
) -> ! {
    let mut zenoh_config = zenoh::prelude::config::peer();

    zenoh_config
        .set_id(Some(config.node_address[0..16].to_string()))
        .unwrap();
    // Whether local writes/queries should reach local subscribers/queryables.
    zenoh_config
        .set_local_routing(Some(config.local_routing))
        .unwrap();
    // If set to false, peers will never automatically establish sessions between each-other.
    zenoh_config
        .scouting
        .set_peers_autoconnect(Some(config.peers_autoconnect))
        .unwrap();
    // QoS
    zenoh_config
        .transport
        .set_qos(QoSConf::new(config.qos).unwrap())
        .unwrap();
    // Link lease duration in milliseconds (default: 10000)
    zenoh_config
        .transport
        .link
        .tx
        .set_lease(Some(config.lease))
        .unwrap();
    // Number fo keep-alive messages in a link lease duration (default: 4)
    zenoh_config
        .transport
        .link
        .tx
        .set_keep_alive(Some(config.keep_alive))
        .unwrap();

    // listen
    zenoh_config
        .listen
        .endpoints
        .push(config.get_address().parse().unwrap());
    // connect to peers
    for peer in &config.peers {
        zenoh_config
            .connect
            .endpoints
            .push(peer.get_address().parse().unwrap());
    }
    // cert
    write_to_file(config.ca_cert.as_bytes(), "ca_cert.pem");
    write_to_file(
        config.cert.as_bytes(),
        format!("{}_cert.pem", &config.domain),
    );
    write_to_file(
        config.priv_key.as_bytes(),
        format!("{}_key.pem", &config.domain),
    );
    zenoh_config
        .transport
        .link
        .tls
        .set_root_ca_certificate(Some("ca_cert.pem".to_string()))
        .unwrap();
    zenoh_config
        .transport
        .link
        .tls
        .set_server_certificate(Some(format!("{}_cert.pem", &config.domain)))
        .unwrap();
    zenoh_config
        .transport
        .link
        .tls
        .set_server_private_key(Some(format!("{}_key.pem", &config.domain)))
        .unwrap();

    let session = zenoh::open(zenoh_config).await.unwrap();

    let mut node_subscriber = session
        .subscribe(config.get_node_origin().to_string())
        .await
        .unwrap();
    let mut chain_subscriber = session
        .subscribe(config.get_chain_origin().to_string())
        .await
        .unwrap();

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3));

    if config.get_node_origin() != config.get_validator_origin() {
        let mut validator_subscriber = session
            .subscribe(config.get_validator_origin().to_string())
            .await
            .unwrap();
        loop {
            tokio::select! {
                sample = node_subscriber.receiver().recv_async() => if let Ok(sample) = sample {
                    debug!("inbound msg: {:?}", &sample);
                    let msg = NetworkMsg::decode(&*sample.value.payload.contiguous()).map_err(|e| error!("{e}")).unwrap();
                    inbound_msg_tx.send(msg).map_err(|e| error!("{e}")).unwrap();
                },
                sample = validator_subscriber.receiver().recv_async() => if let Ok(sample) = sample {
                    debug!("inbound msg: {:?}", &sample);
                    let msg = NetworkMsg::decode(&*sample.value.payload.contiguous()).map_err(|e| error!("{e}")).unwrap();
                    inbound_msg_tx.send(msg).map_err(|e| error!("{e}")).unwrap();
                },
                sample = chain_subscriber.receiver().recv_async() => if let Ok(sample) = sample {
                    debug!("inbound msg: {:?}", &sample);
                    let msg = NetworkMsg::decode(&*sample.value.payload.contiguous()).map_err(|e| error!("{e}")).unwrap();
                    inbound_msg_tx.send(msg).map_err(|e| error!("{e}")).unwrap();
                },
                outbound_msg = outbound_msg_rx.recv_async() => if let Ok(mut msg) = outbound_msg {
                    debug!("outbound msg: {:?}", &msg);
                    let expr_id = session.declare_expr(&msg.origin.to_string()).await.unwrap();
                    session.declare_publication(expr_id).await.unwrap();
                    set_sent_msg_origin(&mut msg, &config);
                    let mut dst = BytesMut::new();
                    msg.encode(&mut dst).unwrap();
                    session.put(expr_id, &*dst).await.unwrap();
                },
                _ = interval.tick() => {
                    let endpoints: Vec<EndPoint> = session
                        .config()
                        .get("connect/endpoints")
                        .unwrap()
                        .downcast_ref::<Vec<EndPoint>>()
                        .unwrap()
                        .to_vec();
                    debug!("{:?}", endpoints);
                    let mut peer_set = HashSet::new();
                    for endpoint in endpoints {
                        peer_set.insert(endpoint.locator.address().to_string());
                    }
                    {
                        let mut peers = peers.write();
                        peers.set_connected_peers(peer_set);
                    }
                },
            }
        }
    } else {
        loop {
            tokio::select! {
                sample = node_subscriber.receiver().recv_async() => if let Ok(sample) = sample {
                    debug!("inbound msg: {:?}", &sample);
                    let msg = NetworkMsg::decode(&*sample.value.payload.contiguous()).map_err(|e| error!("{e}")).unwrap();
                    inbound_msg_tx.send(msg).map_err(|e| error!("{e}")).unwrap();
                },
                sample = chain_subscriber.receiver().recv_async() => if let Ok(sample) = sample {
                    debug!("inbound msg: {:?}", &sample);
                    let msg = NetworkMsg::decode(&*sample.value.payload.contiguous()).map_err(|e| error!("{e}")).unwrap();
                    inbound_msg_tx.send(msg).map_err(|e| error!("{e}")).unwrap();
                },
                outbound_msg = outbound_msg_rx.recv_async() => if let Ok(mut msg) = outbound_msg {
                    debug!("outbound msg: {:?}", &msg);
                    let expr_id = session.declare_expr(&msg.origin.to_string()).await.unwrap();
                    session.declare_publication(expr_id).await.unwrap();
                    set_sent_msg_origin(&mut msg, &config);
                    let mut dst = BytesMut::new();
                    msg.encode(&mut dst).unwrap();
                    session.put(expr_id, &*dst).await.unwrap();
                },
                _ = interval.tick() => {
                    let endpoints: Vec<EndPoint> = session
                        .config()
                        .get("connect/endpoints")
                        .unwrap()
                        .downcast_ref::<Vec<EndPoint>>()
                        .unwrap()
                        .to_vec();
                    debug!("{:?}", endpoints);
                    let mut peer_set = HashSet::new();
                    for endpoint in endpoints {
                        peer_set.insert(endpoint.locator.address().to_string());
                    }
                    {
                        let mut peers = peers.write();
                        peers.set_connected_peers(peer_set);
                    }
                },
            }
        }
    }
}

pub fn set_sent_msg_origin(msg: &mut NetworkMsg, network_config: &NetworkConfig) {
    match msg.module.as_str() {
        "consensus" => {
            msg.origin = network_config.get_validator_origin();
        }
        "controller" => {
            msg.origin = network_config.get_node_origin();
        }
        "" => {
            msg.origin = network_config.get_chain_origin();
        }
        &_ => {}
    }
}
