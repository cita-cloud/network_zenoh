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

use cita_cloud_proto::client::InterceptedSvc;
use cita_cloud_proto::common::{Empty, NodeNetInfo, StatusCode, TotalNodeNetInfo};
use cita_cloud_proto::network::{
    network_msg_handler_service_client::NetworkMsgHandlerServiceClient,
    network_service_server::NetworkService, NetworkMsg, NetworkStatusResponse, RegisterInfo,
};
use cita_cloud_proto::retry::RetryClient;
use flume::Sender;
use log::{debug, error, info, warn};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::config::PeerConfig;
use crate::peer::PeersManger;
use crate::util::parse_multiaddr;

#[derive(Clone)]
pub struct CitaCloudNetworkServiceServer {
    pub dispatch_table:
        HashMap<String, RetryClient<NetworkMsgHandlerServiceClient<InterceptedSvc>>>,
    pub peers: Arc<RwLock<PeersManger>>,
    pub inbound_msg_tx: Sender<NetworkMsg>,
    pub outbound_msg_tx: Sender<NetworkMsg>,
    pub chain_origin: u64,
}

#[tonic::async_trait]
impl NetworkService for CitaCloudNetworkServiceServer {
    async fn send_msg(
        &self,
        request: Request<NetworkMsg>,
    ) -> Result<Response<StatusCode>, tonic::Status> {
        let msg = request.into_inner();
        debug!("send_msg: {:?}", &msg);
        let _ = self
            .outbound_msg_tx
            .send_async(msg)
            .await
            .map_err(|e| error!("{e}"));
        Ok(Response::new(status_code::StatusCode::Success.into()))
    }

    async fn broadcast(
        &self,
        request: Request<NetworkMsg>,
    ) -> Result<Response<StatusCode>, tonic::Status> {
        let mut msg = request.into_inner();
        debug!("broadcast: {:?}", &msg);
        msg.origin = self.chain_origin;
        let _ = self
            .outbound_msg_tx
            .send_async(msg)
            .await
            .map_err(|e| error!("{e}"));

        Ok(Response::new(status_code::StatusCode::Success.into()))
    }

    async fn get_network_status(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<NetworkStatusResponse>, tonic::Status> {
        let reply = NetworkStatusResponse {
            peer_count: self.peers.read().get_connected_peers().len() as u64,
        };

        Ok(Response::new(reply))
    }

    async fn register_network_msg_handler(
        &self,
        _request: Request<RegisterInfo>,
    ) -> Result<Response<StatusCode>, tonic::Status> {
        Ok(Response::new(status_code::StatusCode::Success.into()))
    }

    async fn add_node(
        &self,
        request: Request<NodeNetInfo>,
    ) -> Result<Response<StatusCode>, tonic::Status> {
        let node_net_info = request.into_inner();
        let multiaddr = node_net_info.multi_address;

        let (_, port, domain) = parse_multiaddr(&multiaddr).ok_or_else(|| {
            warn!(
                "parse_multiaddr: not a valid tls multi-address: {}",
                &multiaddr
            );
            Status::invalid_argument(status_code::StatusCode::MultiAddrParseError.to_string())
        })?;

        let address = format!("quic/{}:{}", domain, port);

        let endpoint: zenoh::prelude::config::EndPoint = address.parse().map_err(|_| {
            warn!("parse_addr: not a valid address: {}", address);
            Status::invalid_argument(status_code::StatusCode::MultiAddrParseError.to_string())
        })?;

        let address = endpoint.locator.address();
        info!("attempt to add new peer: {}", &address);

        {
            let mut peers = self.peers.write();
            if peers.get_connected_peers().contains(&domain) {
                //add a connected peer
                return Ok(Response::new(
                    status_code::StatusCode::AddExistedPeer.into(),
                ));
            }
            if peers.get_known_peers().contains_key(&domain) {
                //add a known peer which is already trying to connect, return success
                return Ok(Response::new(status_code::StatusCode::Success.into()));
            }

            let peer = PeerConfig {
                protocol: endpoint.locator.protocol().to_string(),
                domain: domain.to_string(),
                port,
            };

            peers.add_known_peers(domain, peer);
        }
        info!("peer added: {}", &address);

        Ok(Response::new(status_code::StatusCode::Success.into()))
    }

    async fn get_peers_net_info(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<TotalNodeNetInfo>, tonic::Status> {
        let mut node_infos: Vec<NodeNetInfo> = vec![];
        let peers;
        {
            peers = self.peers.read().get_connected_peers().clone();
        }
        for addr in peers.iter() {
            node_infos.push(NodeNetInfo {
                multi_address: addr.to_string(),
                origin: 0,
            });
        }

        Ok(Response::new(TotalNodeNetInfo { nodes: node_infos }))
    }
}
