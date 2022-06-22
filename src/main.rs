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

use std::{collections::HashMap, sync::Arc};

use bytes::BytesMut;
use cita_cloud_proto::{
    health_check::health_server::HealthServer,
    network::{network_service_server::NetworkServiceServer, NetworkMsg},
};
use clap::Parser;
use flume::bounded;
use log::{error, info};
use panic_hook::set_panic_handler;
use parking_lot::RwLock;
use prost::Message;
use zenoh::prelude::*;

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
        peers_map.insert(peer.domain.clone(), peer.clone());
    }

    // grpc server
    let network_svc = CitaCloudNetworkServiceServer {
        dispatch_table,
        peers: Arc::new(RwLock::new(PeersManger::new(peers_map))),
        inbound_msg_tx: inbound_msg_tx.clone(),
        outbound_msg_tx,
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
    zenoh_serve(config, inbound_msg_tx, outbound_msg_rx).await
}

async fn zenoh_serve(
    config: NetworkConfig,
    inbound_msg_tx: flume::Sender<NetworkMsg>,
    outbound_msg_rx: flume::Receiver<NetworkMsg>,
) -> ! {
    let mut zenoh_config = zenoh::prelude::config::peer();

    zenoh_config.set_id(Some(config.domain.clone())).unwrap();
    // Whether local writes/queries should reach local subscribers/queryables.
    zenoh_config.set_local_routing(Some(false)).unwrap();
    // If set to false, peers will never automatically establish sessions between each-other.
    zenoh_config
        .scouting
        .set_peers_autoconnect(Some(false))
        .unwrap();

    // listen
    zenoh_config.listen.endpoints.push(
        format!("{}/{}:{}", config.protocol, config.domain, config.port)
            .parse()
            .unwrap(),
    );
    // connect to peers
    for peer in config.peers {
        zenoh_config.connect.endpoints.push(
            format!("{}/{}:{}", peer.protocol, peer.domain, peer.port)
                .parse()
                .unwrap(),
        );
    }
    // cert
    zenoh_config
        .transport
        .link
        .tls
        .set_root_ca_certificate(Some(config.ca_cert.to_string()))
        .unwrap();
    zenoh_config
        .transport
        .link
        .tls
        .set_server_certificate(Some(config.cert.to_string()))
        .unwrap();
    zenoh_config
        .transport
        .link
        .tls
        .set_server_private_key(Some(config.priv_key.to_string()))
        .unwrap();
    let session = zenoh::open(zenoh_config).await.unwrap();
    let mut subscriber = session.subscribe(config.domain).await.unwrap();
    loop {
        tokio::select! {
            sample = subscriber.receiver().recv_async() => if let Ok(sample) = sample {
                info!("inbound msg: {:?}", &sample);
                let msg = NetworkMsg::decode(&*sample.value.payload.contiguous()).map_err(|e| error!("{e}")).unwrap();
                inbound_msg_tx.send(msg).map_err(|e| error!("{e}")).unwrap();
            },
            outbound_msg = outbound_msg_rx.recv_async() => if let Ok(msg) = outbound_msg {
                info!("outbound msg: {:?}", &msg);
                let expr_id = session.declare_expr(&msg.domain).await.unwrap();
                session.declare_publication(expr_id).await.unwrap();
                let mut dst = BytesMut::new();
                msg.encode(&mut dst).unwrap();
                session.put(expr_id, &*dst).await.unwrap();
            }
        }
    }
}
