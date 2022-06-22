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

use std::{collections::HashMap, sync::Arc};

use cita_cloud_proto::network::{
    network_msg_handler_service_client::NetworkMsgHandlerServiceClient, NetworkMsg,
};
use flume::Receiver;
use log::warn;
use parking_lot::RwLock;
use tonic::transport::Channel;

pub struct NetworkMsgDispatcher {
    pub inbound_msg_rx: Receiver<NetworkMsg>,
    pub dispatch_table: Arc<RwLock<HashMap<String, NetworkMsgHandlerServiceClient<Channel>>>>,
}

impl NetworkMsgDispatcher {
    pub async fn run(self) {
        while let Ok(msg) = self.inbound_msg_rx.recv_async().await {
            let client = {
                let guard = self.dispatch_table.read();
                guard.get(&msg.module).cloned()
            };

            if let Some(mut client) = client {
                let msg_module = msg.module.clone();
                let msg_domain = msg.domain.clone();
                tokio::spawn(async move {
                    if let Err(e) = client.process_network_msg(msg).await {
                        warn!(
                            "registered client processes network msg failed: msg.module {} msg.origin {}, error: {}", &msg_module, &msg_domain, e
                        );
                    }
                });
            } else {
                warn!(
                    "unregistered module, will drop msg: msg.module {} msg.origin {}",
                    &msg.module, &msg.domain
                );
            }
        }
    }
}
