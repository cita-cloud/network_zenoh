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

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use cita_cloud_proto::health_check::{
    health_check_response::ServingStatus, health_server::Health, HealthCheckRequest,
    HealthCheckResponse,
};
use cloud_util::unix_now;
use parking_lot::RwLock;
use tonic::{Request, Response, Status};

use crate::peer::PeersManger;

// grpc server of Health Check
pub struct HealthCheckServer {
    peers: Arc<RwLock<PeersManger>>,
    timestamp: AtomicU64,
    timeout: u64,
}

impl HealthCheckServer {
    pub fn new(peers: Arc<RwLock<PeersManger>>, timeout: u64) -> Self {
        HealthCheckServer {
            peers,
            timestamp: AtomicU64::new(unix_now()),
            timeout,
        }
    }
}

#[tonic::async_trait]
impl Health for HealthCheckServer {
    async fn check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        info!("health check entry!");

        let timestamp = unix_now();
        let (peer_count, known_peer_count) = {
            let peers = self.peers.read();
            (
                peers.get_connected_peers().len() as u64,
                peers.get_known_peers().len() as u64,
            )
        };

        if known_peer_count == 0 || peer_count > 0 {
            self.timestamp.store(timestamp, Ordering::Relaxed);
        }
        let old_timestamp = self.timestamp.load(Ordering::Relaxed);
        let timeout = self.timeout * 1000;
        let status = if timestamp - old_timestamp > timeout {
            // peer is offline for a long time
            ServingStatus::NotServing.into()
        } else {
            ServingStatus::Serving.into()
        };

        let reply = Response::new(HealthCheckResponse { status });
        Ok(reply)
    }
}
