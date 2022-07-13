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

use cita_cloud_proto::network::NetworkMsg;
use md5::{compute, Digest};
use std::{
    fs,
    io::{Error, Write},
    path::{self, Path},
};

use tentacle_multiaddr::{MultiAddr, Protocol};

use crate::config::NetworkConfig;

pub fn write_to_file(content: &[u8], path: impl AsRef<path::Path>) {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path.as_ref())
        .unwrap();
    file.write_all(content).unwrap();
    file.flush().unwrap();
}

pub fn parse_multiaddr(s: &str) -> Option<(String, u16, String)> {
    let multiaddr = s.parse::<MultiAddr>().ok()?;

    let mut iter = multiaddr.iter().peekable();

    while iter.peek().is_some() {
        match iter.peek() {
            Some(Protocol::Ip4(_))
            | Some(Protocol::Ip6(_) | Protocol::Dns4(_) | Protocol::Dns6(_)) => (),
            _ => {
                // ignore is true
                let _ignore = iter.next();
                continue;
            }
        }

        let proto1 = iter.next()?;
        let proto2 = iter.next()?;
        let proto3 = iter.next()?;

        match (proto1, proto2, proto3) {
            (Protocol::Ip4(ip), Protocol::Tcp(port), Protocol::Tls(d)) => {
                return Some((ip.to_string(), port, d.to_string()));
            }
            (Protocol::Ip6(ip), Protocol::Tcp(port), Protocol::Tls(d)) => {
                return Some((ip.to_string(), port, d.to_string()));
            }
            (Protocol::Dns4(ip), Protocol::Tcp(port), Protocol::Tls(d)) => {
                return Some((ip.to_string(), port, d.to_string()));
            }
            (Protocol::Dns6(ip), Protocol::Tcp(port), Protocol::Tls(d)) => {
                return Some((ip.to_string(), port, d.to_string()));
            }
            _ => (),
        }
    }
    None
}

pub fn calculate_md5(path: impl AsRef<Path>) -> Result<Digest, Error> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(compute(s)),
        Err(e) => Err(e),
    }
}

pub fn set_sent_msg_origin(msg: &mut NetworkMsg, network_config: &NetworkConfig) {
    match msg.module.as_str() {
        "consensus" => {
            msg.origin = network_config.get_validator_origin();
        }
        "controller" => {
            msg.origin = network_config.get_node_origin();
        }
        "" => {
            msg.origin = network_config.get_chain_origin();
        }
        &_ => {}
    }
}
