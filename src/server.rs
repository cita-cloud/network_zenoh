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

use std::{collections::HashSet, sync::Arc};

use bytes::BytesMut;
use cita_cloud_proto::network::NetworkMsg;
use log::{debug, error, info, warn};
use parking_lot::RwLock;
use prost::Message;
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
) -> ! {
    // read config.toml
    let config = NetworkConfig::new(config_path);
    // Peer instance mode
    let mut zenoh_config = zenoh::prelude::config::peer();

    // Set the chain_id to the zenoh instance id
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
    let session = zenoh::open(zenoh_config).await.unwrap();

    let mut node_subscriber = session
        .subscribe(config.get_node_origin().to_string())
        .await
        .unwrap();
    let mut chain_subscriber = session
        .subscribe(config.get_chain_origin().to_string())
        .await
        .unwrap();

    let mut hot_update_interval =
        tokio::time::interval(tokio::time::Duration::from_secs(config.hot_update_interval));

    let mut config_md5 = calculate_md5(&config_path).unwrap();
    debug!("config file initial md5: {:x}", config_md5);

    // When the controller (node) address is the same as the consensus address, simply subscribe to the controller (node) address
    if config.get_node_origin() != config.get_validator_origin() {
        let mut validator_subscriber = session
            .subscribe(config.get_validator_origin().to_string())
            .await
            .unwrap();
        loop {
            tokio::select! {
                // node subscriber
                sample = node_subscriber.receiver().recv_async() => if let Ok(sample) = sample {
                    debug!("inbound msg: {:?}", &sample);
                    let msg = NetworkMsg::decode(&*sample.value.payload.contiguous()).map_err(|e| error!("{e}")).unwrap();
                    network_svc.inbound_msg_tx.send(msg).map_err(|e| error!("{e}")).unwrap();
                },
                // validator subscriber
                sample = validator_subscriber.receiver().recv_async() => if let Ok(sample) = sample {
                    debug!("inbound msg: {:?}", &sample);
                    let msg = NetworkMsg::decode(&*sample.value.payload.contiguous()).map_err(|e| error!("{e}")).unwrap();
                    network_svc.inbound_msg_tx.send(msg).map_err(|e| error!("{e}")).unwrap();
                },
                // chain subscriber
                sample = chain_subscriber.receiver().recv_async() => if let Ok(sample) = sample {
                    debug!("inbound msg: {:?}", &sample);
                    let msg = NetworkMsg::decode(&*sample.value.payload.contiguous()).map_err(|e| error!("{e}")).unwrap();
                    network_svc.inbound_msg_tx.send(msg).map_err(|e| error!("{e}")).unwrap();
                },
                // outbound msg
                outbound_msg = outbound_msg_rx.recv_async() => if let Ok(mut msg) = outbound_msg {
                    debug!("outbound msg: {:?}", &msg);
                    let expr_id = session.declare_expr(&msg.origin.to_string()).await.unwrap();
                    session.declare_publication(expr_id).await.unwrap();
                    msg.origin = config.get_node_origin();
                    let mut dst = BytesMut::new();
                    msg.encode(&mut dst).unwrap();
                    session.put(expr_id, &*dst).await.unwrap();
                },
                // hot update
                _ = hot_update_interval.tick() => {
                    // update connected peers
                    let endpoints: Vec<EndPoint> = session
                        .config()
                        .get("connect/endpoints")
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

                    // hot update
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
            }
        }
    } else {
        loop {
            tokio::select! {
                // node subscriber
                sample = node_subscriber.receiver().recv_async() => if let Ok(sample) = sample {
                    debug!("inbound msg: {:?}", &sample);
                    let msg = NetworkMsg::decode(&*sample.value.payload.contiguous()).map_err(|e| error!("{e}")).unwrap();
                    network_svc.inbound_msg_tx.send(msg).map_err(|e| error!("{e}")).unwrap();
                },
                // chain subscriber
                sample = chain_subscriber.receiver().recv_async() => if let Ok(sample) = sample {
                    debug!("inbound msg: {:?}", &sample);
                    let msg = NetworkMsg::decode(&*sample.value.payload.contiguous()).map_err(|e| error!("{e}")).unwrap();
                    network_svc.inbound_msg_tx.send(msg).map_err(|e| error!("{e}")).unwrap();
                },
                // outbound msg
                outbound_msg = outbound_msg_rx.recv_async() => if let Ok(mut msg) = outbound_msg {
                    debug!("outbound msg: {:?}", &msg);
                    let expr_id = session.declare_expr(&msg.origin.to_string()).await.unwrap();
                    session.declare_publication(expr_id).await.unwrap();
                    msg.origin = config.get_node_origin();
                    let mut dst = BytesMut::new();
                    msg.encode(&mut dst).unwrap();
                    session.put(expr_id, &*dst).await.unwrap();
                },
                // hot update
                _ = hot_update_interval.tick() => {
                    // update connected peers
                    let endpoints: Vec<EndPoint> = session
                        .config()
                        .get("connect/endpoints")
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

                    // hot update
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
            }
        }
    }
}
