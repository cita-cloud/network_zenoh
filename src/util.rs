use std::{fs, io::Write, path};

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
