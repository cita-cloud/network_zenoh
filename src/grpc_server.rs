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

use cita_cloud_proto::common::{Empty, NodeNetInfo, StatusCode, TotalNodeNetInfo};
use cita_cloud_proto::network::{
    network_msg_handler_service_client::NetworkMsgHandlerServiceClient,
    network_service_server::NetworkService, NetworkMsg, NetworkStatusResponse, RegisterInfo,
};
use flume::Sender;
use log::{debug, info, warn};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tonic::transport::{Channel, Endpoint};
use tonic::{Request, Response, Status};

use crate::config::PeerConfig;
use crate::peer::PeersManger;

#[derive(Clone)]
pub struct CitaCloudNetworkServiceServer {
    pub dispatch_table: Arc<RwLock<HashMap<String, NetworkMsgHandlerServiceClient<Channel>>>>,
    pub peers: Arc<RwLock<PeersManger>>,
    pub inbound_msg_tx: Sender<NetworkMsg>,
    pub outbound_msg_tx: Sender<NetworkMsg>,
}

#[tonic::async_trait]
impl NetworkService for CitaCloudNetworkServiceServer {
    async fn send_msg(
        &self,
        request: Request<NetworkMsg>,
    ) -> Result<Response<StatusCode>, tonic::Status> {
        let msg = request.into_inner();
        debug!("send_msg: {:?}", &msg);
        let peers = self.peers.read();
        if peers
            .get_connected_peers()
            .iter()
            .any(|peer| peer == &msg.domain)
        {
            let _ = self.outbound_msg_tx.send(msg);
        } else {
            // TODO: check if it's necessary
            // fallback to broadcast
            for domain in peers.get_known_peers().keys() {
                let mut msg = msg.clone();
                msg.domain = domain.to_owned();
                let _ = self.outbound_msg_tx.send(msg);
            }
        }
        Ok(Response::new(status_code::StatusCode::Success.into()))
    }

    async fn broadcast(
        &self,
        request: Request<NetworkMsg>,
    ) -> Result<Response<StatusCode>, tonic::Status> {
        let msg = request.into_inner();
        debug!("broadcast: {:?}", &msg);
        let peers = self.peers.read();

        for domain in peers.get_known_peers().keys() {
            let mut msg = msg.clone();
            msg.domain = domain.to_owned();
            let _ = self.outbound_msg_tx.send(msg);
        }

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
        request: Request<RegisterInfo>,
    ) -> Result<Response<StatusCode>, tonic::Status> {
        let info = request.into_inner();
        let module_name = info.module_name;
        let hostname = info.hostname;
        let port = info.port;

        let client = {
            let uri = format!("http://{}:{}", hostname, port);
            let channel = Endpoint::from_shared(uri)
                .map_err(|e| {
                    tonic::Status::invalid_argument(format!("invalid host and port: {}", e))
                })?
                .connect_lazy()
                .unwrap();
            NetworkMsgHandlerServiceClient::new(channel)
        };

        let mut dispatch_table = self.dispatch_table.write();
        dispatch_table.insert(module_name, client);

        Ok(Response::new(status_code::StatusCode::Success.into()))
    }

    async fn add_node(
        &self,
        request: Request<NodeNetInfo>,
    ) -> Result<Response<StatusCode>, tonic::Status> {
        let address = request.into_inner().multi_address;
        let endpoint: zenoh::prelude::config::EndPoint = address.parse().map_err(|_| {
            warn!("parse_addr: not a valid address: {}", address);
            Status::invalid_argument(status_code::StatusCode::MultiAddrParseError.to_string())
        })?;
        let address = endpoint.locator.address();
        info!("attempt to add new peer: {}", &address);

        let mut peers = self.peers.write();
        let (domain, port) = address.split_once(':').unwrap();
        if peers.get_connected_peers().contains(domain) {
            //add a connected peer
            return Ok(Response::new(
                status_code::StatusCode::AddExistedPeer.into(),
            ));
        }
        if peers.get_known_peers().contains_key(domain) {
            //add a known peer which is already trying to connect, return success
            return Ok(Response::new(status_code::StatusCode::Success.into()));
        }

        let peer = PeerConfig {
            protocol: endpoint.locator.protocol().to_string(),
            domain: domain.to_string(),
            port: port.parse().unwrap(),
        };

        peers.add_known_peers(domain.to_string(), peer);

        info!("peer added: {}", &address);

        Ok(Response::new(status_code::StatusCode::Success.into()))
    }

    async fn get_peers_net_info(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<TotalNodeNetInfo>, tonic::Status> {
        let mut node_infos: Vec<NodeNetInfo> = vec![];
        let guard = self.peers.read();
        for domain in guard.get_connected_peers().iter() {
            node_infos.push(NodeNetInfo {
                multi_address: domain.to_string(),
                domain: domain.to_string(),
            });
        }

        Ok(Response::new(TotalNodeNetInfo { nodes: node_infos }))
    }
}
