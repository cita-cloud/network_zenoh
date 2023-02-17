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

use md5::{compute, Digest};
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    io::{Error, Write},
    path::{self, Path},
};

use tentacle_multiaddr::{MultiAddr, Protocol};

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

pub fn build_multiaddr(host: &str, port: u16, domain: &str) -> String {
    // TODO: default to return Dns4 for host, consider if it' appropriate
    vec![
        Protocol::Dns4(host.into()),
        Protocol::Tcp(port),
        Protocol::Tls(domain.into()),
    ]
    .into_iter()
    .collect::<MultiAddr>()
    .to_string()
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

pub fn to_u64(tmp: &str) -> u64 {
    let mut decoded = [0; 8];
    hex::decode_to_slice(tmp, &mut decoded).unwrap();
    u64::from_be_bytes(decoded)
}

pub fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

pub fn clap_about() -> String {
    let name = env!("CARGO_PKG_NAME").to_string();
    let version = env!("CARGO_PKG_VERSION");
    let authors = env!("CARGO_PKG_AUTHORS");
    name + " " + version + "\n" + authors
}
