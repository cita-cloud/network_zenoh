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

use parking_lot::RwLock;
use prost::Message;
use std::{str::FromStr, sync::Arc, time::Duration};
use tokio::time::interval;
use zenoh::{
    bytes::ZBytes,
    config::ZenohId,
    qos::{CongestionControl, Priority},
    sample::SampleKind,
};
use zenoh_ext::SubscriberBuilderExt;

use cita_cloud_proto::network::NetworkMsg;
use util::write_to_file;

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
    rx: flume::Receiver<()>,
) {
    // read config.toml
    let config = NetworkConfig::new(config_path);
    // Peer instance mode
    let mut zenoh_config = zenoh::config::Config::default();

    // Use rand ZenohId
    let zid = ZenohId::default();
    info!("ZenohId: {zid}");
    zenoh_config.set_id(zid).unwrap();

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

    // Link lease duration in milliseconds (default: 10000)
    zenoh_config
        .transport
        .link
        .tx
        .set_lease(config.lease)
        .unwrap();

    // Number fo keep-alive messages in a link lease duration (default: 4)
    zenoh_config
        .transport
        .link
        .tx
        .set_keep_alive(config.keep_alive)
        .unwrap();

    // Receiving buffer size in bytes for each link
    zenoh_config
        .transport
        .link
        .rx
        .set_buffer_size(config.rx_buffer_size)
        .unwrap();

    // Maximum number of unicast incoming links per transport session
    zenoh_config.transport.unicast.set_max_links(4).unwrap();

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
        .set(vec![format!("{}/[::]:{}", config.protocol, config.port)
            .parse()
            .unwrap()])
        .unwrap();

    // Which zenoh nodes to connect to
    let mut connect_peers = Vec::new();
    for peer in &config.peers {
        connect_peers.push(peer.get_address().parse().unwrap());
    }
    zenoh_config.connect.endpoints.set(connect_peers).unwrap();

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
        .set_listen_certificate(Some(format!("{}_cert.pem", &config.domain)))
        .unwrap();
    zenoh_config
        .transport
        .link
        .tls
        .set_listen_private_key(Some(format!("{}_key.pem", &config.domain)))
        .unwrap();

    // open zenoh session
    let session = zenoh::open(zenoh_config).await.unwrap();

    let self_node_origin = config.get_node_origin();
    let self_validator_origin = config.get_validator_origin();

    // node subscriber
    let inbound_msg_tx = network_svc.inbound_msg_tx.clone();
    let _node_subscriber = session
        .declare_subscriber(self_node_origin.to_string())
        .callback(move |sample| {
            let msg = NetworkMsg::decode(&*sample.payload().to_bytes())
                .map_err(|e| error!("{e}"))
                .unwrap();
            debug!("inbound msg node: {:?}", &msg);
            inbound_msg_tx.send(msg).map_err(|e| error!("{e}")).unwrap();
        })
        .await
        .unwrap();

    // chain subscriber
    let inbound_msg_tx = network_svc.inbound_msg_tx.clone();
    let _chain_subscriber = session
        .declare_subscriber(config.get_chain_origin().to_string())
        .callback(move |sample| {
            let msg = NetworkMsg::decode(&*sample.payload().to_bytes())
                .map_err(|e| error!("{e}"))
                .unwrap();
            if msg.origin != self_node_origin {
                debug!("inbound msg chain: {:?}", &msg);
                inbound_msg_tx.send(msg).map_err(|e| error!("{e}")).unwrap();
            }
        })
        .await
        .unwrap();

    // When the controller (node) address is the same as the consensus address, simply subscribe to the controller (node) address
    let _validator_subscriber;
    if self_node_origin != self_validator_origin {
        debug!("------ (node_origin != validator_origin)");
        let inbound_msg_tx = network_svc.inbound_msg_tx.clone();
        _validator_subscriber = session
            .declare_subscriber(self_validator_origin.to_string())
            .callback(move |sample| {
                let msg = NetworkMsg::decode(&*sample.payload().to_bytes())
                    .map_err(|e| error!("{e}"))
                    .unwrap();
                if msg.origin != self_node_origin {
                    debug!("inbound msg validator: {:?}", &msg);
                    inbound_msg_tx.send(msg).map_err(|e| error!("{e}")).unwrap();
                }
            })
            .await
            .unwrap();
    }

    // liveliness declaring
    let liveliness_prefix = format!("chain-id:{}/", config.get_chain_origin());
    let _liveliness_token = session
        .liveliness()
        .declare_token(&format!(
            "{}{}@{}",
            liveliness_prefix, config.domain, self_node_origin
        ))
        .await
        .unwrap();

    // liveliness subscriber
    let peers_to_update = peers.clone();
    let _liveliness_subscriber = session
        .liveliness()
        .declare_subscriber(format!("{}**", liveliness_prefix))
        .querying()
        .callback(move |sample| {
            if let Some(key_expr) = sample.key_expr().as_str().strip_prefix(&liveliness_prefix) {
                if let Some((domain, origin)) = key_expr.split_once('@') {
                    if let Ok(origin) = u64::from_str(origin) {
                        match sample.kind() {
                            SampleKind::Put => {
                                info!("Peer connected, new alive token ({} - {})", domain, origin);
                                peers_to_update.write().add_connected_peer(domain, origin);
                            }
                            SampleKind::Delete => {
                                info!("Peer offline, dropped token ({} - {})", domain, origin);
                                peers_to_update.write().delete_connected_peer(domain);
                            }
                        }
                    }
                }
            }
        })
        .await
        .unwrap();

    let mut config_md5 = calculate_md5(config_path).unwrap();
    debug!("config file initial md5: {:x}", config_md5);

    let mut hot_update_interval = interval(Duration::from_secs(config.hot_update_interval));

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
                        .congestion_control(CongestionControl::Block)
                        .priority(priority)
                        .await
                        .unwrap();
                    msg.origin = if "consensus".eq(&msg.module) {
                        self_validator_origin
                    } else {
                        self_node_origin
                    };
                    let payload = ZBytes::from(msg.encode_to_vec());
                    publisher
                        .put(payload)
                        .await
                        .unwrap();
                }
                Err(e) => debug!("outbound_msg_rx: {e}"),
            },
            // hot update
            _ = hot_update_interval.tick() => {
                if let Ok(new_md5) = calculate_md5(config_path) {
                    if new_md5 != config_md5 {
                        info!("config file new md5: {:x}", new_md5);
                        config_md5 = new_md5;
                        try_hot_update(config_path, network_svc.peers.clone(), session.config()).await;
                    }
                } else {
                    warn!("calculate config file md5 failed, make sure it's not removed");
                };
            },
            _ = rx.recv_async() => {
                info!("zenoh session exit!");
                break;
            },
            else => {
                debug!("zenoh session exit!");
                break;
            }
        }
    }
}
