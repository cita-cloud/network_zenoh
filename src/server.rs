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

use std::{collections::HashSet, str::FromStr, sync::Arc};

use bytes::BytesMut;
use cita_cloud_proto::network::NetworkMsg;
use cloud_util::unix_now;
use log::{debug, error, info, warn};
use parking_lot::RwLock;
use prost::Message;
use r#async::AsyncResolve;
use util::write_to_file;
use zenoh::{
    config::{EndPoint, QoSConf},
    prelude::*,
};

use crate::{
    config::NetworkConfig,
    grpc_server::CitaCloudNetworkServiceServer,
    hot_update::try_hot_update,
    peer::PeersManger,
    util::{self, calculate_md5},
};

pub async fn zenoh_serve(
    peers: Arc<RwLock<PeersManger>>,
    config_path: &str,
    network_svc: CitaCloudNetworkServiceServer,
    outbound_msg_rx: flume::Receiver<NetworkMsg>,
) {
    // read config.toml
    let config = NetworkConfig::new(config_path);
    // Peer instance mode
    let mut zenoh_config = zenoh::prelude::config::peer();

    // Set the chain_id to the zenoh instance id
    zenoh_config
        .set_id(ZenohId::from_str(&config.node_address[0..16]).unwrap())
        .unwrap();

    zenoh_config
        .scouting
        .gossip
        .set_enabled(Some(config.scouting))
        .unwrap();

    zenoh_config
        .scouting
        .multicast
        .set_enabled(Some(config.scouting))
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

    // Receiving buffer size in bytes for each link
    zenoh_config
        .transport
        .link
        .rx
        .set_buffer_size(Some(config.rx_buffer_size))
        .unwrap();

    // the shared-memory transport will be disabled.
    zenoh_config
        .transport
        .shared_memory
        .set_enabled(false)
        .unwrap();

    // Set listen endpoints
    zenoh_config
        .listen
        .endpoints
        .push(config.get_address().parse().unwrap());

    // Which zenoh nodes to connect to
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

    // open zenoh session
    let session = zenoh::open(zenoh_config).res().await.unwrap();

    let self_node_origin = config.get_node_origin();

    // node subscriber
    let inbound_msg_tx = network_svc.inbound_msg_tx.clone();
    let _node_subscriber = session
        .declare_subscriber(self_node_origin.to_string())
        .callback(move |sample| {
            let msg = NetworkMsg::decode(&*sample.value.payload.contiguous())
                .map_err(|e| error!("{e}"))
                .unwrap();
            debug!("inbound msg node: {:?}", &msg);
            inbound_msg_tx.send(msg).map_err(|e| error!("{e}")).unwrap();
        })
        .best_effort()
        .res()
        .await
        .unwrap();

    // chain subscriber
    let inbound_msg_tx = network_svc.inbound_msg_tx.clone();
    let _chain_subscriber = session
        .declare_subscriber(config.get_chain_origin().to_string())
        .callback(move |sample| {
            let msg = NetworkMsg::decode(&*sample.value.payload.contiguous())
                .map_err(|e| error!("{e}"))
                .unwrap();
            if msg.origin != self_node_origin {
                debug!("inbound msg chain: {:?}", &msg);
                inbound_msg_tx.send(msg).map_err(|e| error!("{e}")).unwrap();
            }
        })
        .best_effort()
        .res()
        .await
        .unwrap();

    // When the controller (node) address is the same as the consensus address, simply subscribe to the controller (node) address
    let _validator_subscriber;
    if self_node_origin != config.get_validator_origin() {
        debug!("------ (node_origin != validator_origin)");
        let inbound_msg_tx = network_svc.inbound_msg_tx.clone();
        _validator_subscriber = session
            .declare_subscriber(config.get_validator_origin().to_string())
            .callback(move |sample| {
                let msg = NetworkMsg::decode(&*sample.value.payload.contiguous())
                    .map_err(|e| error!("{e}"))
                    .unwrap();
                if msg.origin != self_node_origin {
                    debug!("inbound msg validator: {:?}", &msg);
                    inbound_msg_tx.send(msg).map_err(|e| error!("{e}")).unwrap();
                }
            })
            .best_effort()
            .res()
            .await
            .unwrap();
    }

    // send_msg_check subscriber
    let outbound_msg_tx = network_svc.outbound_msg_tx.clone();
    let _check_forward_subscriber = session
        .declare_subscriber(format!("{}-check", config.get_chain_origin()))
        .callback(move |sample| {
            let mut msg = NetworkMsg::decode(&*sample.value.payload.contiguous())
                .map_err(|e| error!("{e}"))
                .unwrap();
            if msg.origin != self_node_origin {
                info!(
                    "HEALTH_CHECK msg from: {}, sent at: {}",
                    msg.origin,
                    u64::from_be_bytes(msg.msg[8..16].try_into().unwrap())
                );
                msg.origin = u64::from_be_bytes(msg.msg[..8].try_into().unwrap());
                let _ = outbound_msg_tx.send(msg);
            }
        })
        .best_effort()
        .res()
        .await
        .unwrap();

    let mut config_md5 = calculate_md5(&config_path).unwrap();
    debug!("config file initial md5: {:x}", config_md5);

    let mut hot_update_interval =
        tokio::time::interval(tokio::time::Duration::from_secs(config.hot_update_interval));

    let mut health_check_interval = tokio::time::interval(tokio::time::Duration::from_secs(
        config.health_check_interval,
    ));

    loop {
        tokio::select! {
            // outbound msg
            outbound_msg = outbound_msg_rx.recv_async() => match outbound_msg {
                Ok(mut msg) => {
                    debug!("outbound msg: {:?}", &msg);
                    let priority = match msg.r#type.as_str() {
                        "send_tx" => Priority::Data,
                        "send_txs" => Priority::Data,
                        "sync_block" => Priority::Data,
                        "chain_status_respond" => Priority::InteractiveLow,
                        "sync_block_respond" => Priority::InteractiveLow,
                        "sync_tx_respond" => Priority::InteractiveLow,
                        _ => Priority::DataHigh,
                    };
                    let publisher = session
                        .declare_publisher(msg.origin.to_string())
                        .congestion_control(zenoh::publication::CongestionControl::Block)
                        .priority(priority)
                        .res()
                        .await
                        .unwrap();
                    msg.origin = self_node_origin;
                    let mut dst = BytesMut::new();
                    msg.encode(&mut dst).unwrap();
                    publisher
                        .put(&*dst)
                        .res()
                        .await
                        .unwrap();
                }
                Err(e) => debug!("outbound_msg_rx: {e}"),
            },
            // hot update
            _ = hot_update_interval.tick() => {
                if let Ok(new_md5) = calculate_md5(&config_path) {
                    if new_md5 != config_md5 {
                        info!("config file new md5: {:x}", new_md5);
                        config_md5 = new_md5;
                        try_hot_update(config_path, network_svc.peers.clone(), session.config()).await;
                    }
                } else {
                    warn!("calculate config file md5 failed, make sure it's not removed");
                };
            },
            // health check
            _ = health_check_interval.tick() => {
                // update connected peers
                let endpoints: Vec<EndPoint> = session
                    .config()
                    .get("connect/endpoints")
                    .map_err(|e| error!("{e}"))
                    .unwrap()
                    .downcast_ref::<Vec<EndPoint>>()
                    .unwrap()
                    .to_vec();
                debug!("{:?}", endpoints);
                let mut connected_peers = HashSet::new();
                for endpoint in endpoints {
                    connected_peers.insert(endpoint.locator.address().to_string());
                }
                {
                    let mut peers = peers.write();
                    peers.set_connected_peers(connected_peers);
                }

                // send msg check
                let now = unix_now();
                let mut msg = self_node_origin.to_be_bytes().to_vec();
                msg.append(&mut now.to_be_bytes().to_vec());
                let msg = NetworkMsg {
                    module: "HEALTH_CHECK".to_string(),
                    r#type: "HEALTH_CHECK".to_string(),
                    origin: self_node_origin,
                    msg
                };
                let publisher = session
                    .declare_publisher(format!("{}-check", config.get_chain_origin()))
                    .congestion_control(zenoh::publication::CongestionControl::Block)
                    .priority(Priority::DataHigh)
                    .res()
                    .await
                    .unwrap();
                let mut dst = BytesMut::new();
                msg.encode(&mut dst).unwrap();
                publisher
                    .put(&*dst)
                    .res()
                    .await
                    .unwrap();
            },
            else => {
                debug!("network stopped!");
                break;
            }
        }
    }
}
