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

mod config;
mod dispatcher;
mod grpc_server;
mod health_check;
mod hot_update;
mod panic_hook;
mod peer;
mod server;
mod util;

use std::{collections::HashMap, sync::Arc};

use cita_cloud_proto::{
    client::ClientOptions, health_check::health_server::HealthServer,
    network::network_service_server::NetworkServiceServer,
};
use clap::Parser;
use flume::unbounded;
use log::info;
use panic_hook::set_panic_handler;
use parking_lot::RwLock;

use crate::{
    config::NetworkConfig, dispatcher::NetworkMsgDispatcher,
    grpc_server::CitaCloudNetworkServiceServer, health_check::HealthCheckServer, peer::PeersManger,
    server::zenoh_serve,
};

const CLIENT_NAME: &str = "network";

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

    tokio::spawn(cloud_util::signal::handle_signals());

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
    let (inbound_msg_tx, inbound_msg_rx) = unbounded();

    // outbound_msg
    let (outbound_msg_tx, outbound_msg_rx) = unbounded();

    // dispatcher run
    let mut dispatch_table = HashMap::new();

    for module in &config.modules {
        let client = {
            let client_options = ClientOptions::new(
                CLIENT_NAME.to_string(),
                format!("http://{}:{}", module.hostname, module.port),
            );
            client_options.connect_network_msg_handler().unwrap()
        };
        dispatch_table.insert(module.module_name.clone(), client);
    }

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
    let network_svc_hot_update = network_svc.clone();
    let grpc_addr = format!("0.0.0.0:{}", grpc_port).parse().unwrap();
    let peers_for_health_check = peers.clone();
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(NetworkServiceServer::new(network_svc))
            .add_service(HealthServer::new(HealthCheckServer::new(
                peers_for_health_check,
                config.health_check_timeout,
            )))
            .serve(grpc_addr)
            .await
            .unwrap();
    });

    // run zenoh instance
    zenoh_serve(
        peers,
        &opts.config_path,
        network_svc_hot_update,
        outbound_msg_rx,
    )
    .await
}
