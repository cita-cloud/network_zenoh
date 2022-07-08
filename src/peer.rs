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

use std::collections::{HashMap, HashSet};

use log::debug;

use crate::config::PeerConfig;

#[derive(Debug)]
pub struct PeersManger {
    known_peers: HashMap<String, PeerConfig>,
    connected_peers: HashSet<String>,
}

impl PeersManger {
    pub fn new(known_peers: HashMap<String, PeerConfig>) -> Self {
        Self {
            known_peers,
            connected_peers: HashSet::new(),
        }
    }

    pub fn get_known_peers(&self) -> &HashMap<String, PeerConfig> {
        &self.known_peers
    }

    pub fn add_known_peers(&mut self, domain: String, peer: PeerConfig) -> Option<PeerConfig> {
        debug!("add_from_config_peers: {}", domain);
        self.known_peers.insert(domain, peer)
    }

    pub fn get_connected_peers(&self) -> &HashSet<String> {
        &self.connected_peers
    }

    pub fn set_connected_peers(&mut self, peers: HashSet<String>) {
        self.connected_peers = peers;
    }
}
