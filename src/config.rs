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

use cloud_util::common::read_toml;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PeerConfig {
    pub protocol: String,
    pub port: u16,
    pub domain: String,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    // server grpc port, as network_port
    pub grpc_port: u16,
    // zenoh protocol
    pub protocol: String,
    // zenoh port
    pub port: u16,
    // domain
    pub domain: String,
    // CA certification, raw string
    pub ca_cert: String,
    // Server certification, raw string
    pub cert: String,
    // Server certification private key
    pub priv_key: String,
    // peers net config info
    pub peers: Vec<PeerConfig>,
}

impl NetworkConfig {
    pub fn new(config_str: &str) -> Self {
        read_toml(config_str, "network_zenoh")
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            grpc_port: 50000,
            protocol: "tls".to_string(),
            domain: "".to_string(),
            port: 40000,
            peers: vec![],
            ca_cert: "".to_string(),
            cert: "".to_string(),
            priv_key: "".to_string(),
        }
    }
}
