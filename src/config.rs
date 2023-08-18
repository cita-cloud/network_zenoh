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

use cloud_util::{common::read_toml, tracer::LogConfig};
use serde::{Deserialize, Serialize};
use std::fs;

use crate::util::to_u64;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PeerConfig {
    pub protocol: String,
    pub port: u16,
    pub domain: String,
}

impl PeerConfig {
    pub fn get_address(&self) -> String {
        format!("{}/{}:{}", self.protocol, self.domain, self.port)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModuleConfig {
    pub module_name: String,
    pub hostname: String,
    pub port: u16,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    /// server grpc port, as network_port
    pub grpc_port: u16,
    /// zenoh protocol
    pub protocol: String,
    /// zenoh port
    pub port: u16,
    /// domain
    pub domain: String,
    /// CA certification, raw string
    pub ca_cert: String,
    /// Server certification, raw string
    pub cert: String,
    /// Server certification private key
    pub priv_key: String,
    /// peers net config info
    pub peers: Vec<PeerConfig>,
    /// node address file path
    pub node_address: String,
    /// validator address file path
    pub validator_address: String,
    /// chain id
    pub chain_id: String,
    /// QoS
    pub qos: bool,
    /// local_routing
    pub local_routing: bool,
    /// scouting
    pub scouting: bool,
    /// Link lease duration in milliseconds (default: 10000)
    pub lease: u64,
    /// Number fo keep-alive messages in a link lease duration (default: 4)
    pub keep_alive: usize,
    /// config hot update interval, in seconds
    pub hot_update_interval: u64,
    /// modules config info
    pub modules: Vec<ModuleConfig>,
    /// health check interval, in seconds
    pub health_check_interval: u64,
    /// health check timeout
    pub health_check_timeout: u64,
    /// enable metrics or not
    pub enable_metrics: bool,
    /// metrics exporter port
    pub metrics_port: u16,
    /// metrics histogram buckets
    pub metrics_buckets: Vec<f64>,
    /// Receiving buffer size in bytes for each link
    /// The default the rx_buffer_size value is the same as the default batch size: 65335.
    /// For very high throughput scenarios, the rx_buffer_size can be increased to accomodate
    /// more in-flight data. This is particularly relevant when dealing with large messages.
    /// E.g. for 16MiB rx_buffer_size set the value to: 16777216.
    pub rx_buffer_size: usize,
    /// log config
    pub log_config: LogConfig,
}

impl NetworkConfig {
    pub fn new(config_str: &str) -> Self {
        let mut config: NetworkConfig = read_toml(config_str, "network_zenoh");
        let node_address_path = config.node_address.clone();
        let validator_address_path = config.validator_address.clone();
        config.node_address = fs::read_to_string(node_address_path).unwrap();
        config.validator_address = fs::read_to_string(validator_address_path).unwrap();
        config
    }
    pub fn get_node_origin(&self) -> u64 {
        to_u64(&self.node_address[0..16])
    }
    pub fn get_validator_origin(&self) -> u64 {
        to_u64(&self.validator_address[0..16])
    }
    pub fn get_chain_origin(&self) -> u64 {
        to_u64(&self.chain_id[0..16])
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
            node_address: "".to_string(),
            validator_address: "".to_string(),
            chain_id: "".to_string(),
            qos: true,
            local_routing: false,
            scouting: false,
            lease: 10000,
            keep_alive: 4,
            hot_update_interval: 60,
            modules: vec![],
            health_check_interval: 120,
            health_check_timeout: 300,
            enable_metrics: true,
            metrics_port: 60000,
            metrics_buckets: vec![
                0.25, 0.5, 0.75, 1.0, 2.5, 5.0, 7.5, 10.0, 25.0, 50.0, 75.0, 100.0, 250.0, 500.0,
            ],
            rx_buffer_size: 16777216,
            log_config: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NetworkConfig;

    #[test]
    fn basic_test() {
        let config = NetworkConfig::new("example/config.toml");

        assert_eq!(
            config.chain_id,
            "63586a3c0255f337c77a777ff54f0040b8c388da04f23ecee6bfd4953a6512b4"
        );
        assert_eq!(config.domain, "test-chain-0");
        assert_eq!(config.grpc_port, 50000);
        assert_eq!(config.metrics_port, 60000);
        assert_eq!(config.port, 40000);
        assert_eq!(
            config.node_address,
            "c356876e7f4831476f99ea0593b0cd7a6053e4d3".to_string()
        );
        assert_eq!(
            config.validator_address,
            "c356876e7f4831476f99ea0593b0cd7a6053e4d3".to_string()
        );
    }
}
