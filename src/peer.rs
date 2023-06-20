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

use crate::config::PeerConfig;

#[derive(Debug)]
pub struct PeersManger {
    known_peers: HashMap<String, (u64, PeerConfig)>,
    connected_peers: HashSet<String>,
}

impl PeersManger {
    pub fn new(known_peers: HashMap<String, (u64, PeerConfig)>) -> Self {
        Self {
            known_peers,
            connected_peers: HashSet::new(),
        }
    }

    pub fn get_known_peers(&self) -> &HashMap<String, (u64, PeerConfig)> {
        &self.known_peers
    }

    pub fn add_known_peers(
        &mut self,
        domain: String,
        peer: (u64, PeerConfig),
    ) -> Option<(u64, PeerConfig)> {
        debug!("add_from_config_peers: {}", domain);
        self.known_peers.insert(domain, peer)
    }

    pub fn get_connected_peers(&self) -> &HashSet<String> {
        &self.connected_peers
    }

    pub fn add_connected_peer(&mut self, domain: &str, origin: u64) {
        if let Some((addr, _)) = self.known_peers.get_mut(domain) {
            *addr = origin;
            self.connected_peers.insert(domain.to_owned());
        }
    }

    pub fn delete_connected_peer(&mut self, domain: &str) {
        if self.connected_peers.get(domain).is_some() {
            debug!("delete_connected_peers: {}", domain);
            self.connected_peers.remove(domain);
        }
    }

    pub fn delete_peer(&mut self, domain: &str) {
        if self.known_peers.contains_key(domain) {
            debug!("delete_peer: {}", domain);
            self.known_peers.remove(domain);
            self.delete_connected_peer(domain);
        }
    }
}
