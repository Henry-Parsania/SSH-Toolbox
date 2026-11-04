//! What to scan: addresses, CIDR networks (`192.168.1.0/24`), ranges (`192.168.1.10-50` or
//! `10.0.0.1-10.0.0.40`) and host names, separated by commas, spaces or new lines.

use std::net::{IpAddr, Ipv4Addr};

use crate::error::{AppError, ErrorCode};

/// Most hosts one scan may cover.
pub const MAX_HOSTS: usize = 4096;
/// The smallest prefix (largest network) accepted: /20 is 4096 addresses.
pub const MIN_PREFIX: u32 = 20;

fn invalid(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::InvalidInput, message)
}

/// One item of the target text, before host names are looked up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Ip(IpAddr),
    Name(String),
}

fn usable(ip: Ipv4Addr) -> bool {
    // 0.x.x.x is "this network"; 224 and up are multicast and reserved.
    ip.octets()[0] != 0 && ip.octets()[0] < 224
}

fn range(from: u32, to: u32) -> Result<Vec<Item>, AppError> {
    if to < from {
        return Err(invalid("The end of the range is before its start."));
    }
    if (to - from) as usize >= MAX_HOSTS {
        return Err(invalid(format!(
            "That is more than {MAX_HOSTS} addresses. Scan a smaller part."
        )));
    }
    Ok((from..=to)
        .map(Ipv4Addr::from)
        .filter(|ip| usable(*ip))
        .map(|ip| Item::Ip(IpAddr::V4(ip)))
        .collect())
}

fn parse_part(part: &str) -> Result<Vec<Item>, AppError> {
    // 192.168.1.0/24
    if let Some((addr, prefix)) = part.split_once('/') {
        let ip: Ipv4Addr = addr
            .parse()
            .map_err(|_| invalid(format!("“{part}” isn't a network.")))?;
        let prefix: u32 = prefix
            .parse()
            .map_err(|_| invalid(format!("“{part}” isn't a network.")))?;
        if !(MIN_PREFIX..=32).contains(&prefix) {
            return Err(invalid(format!(
                "Networks bigger than /{MIN_PREFIX} (4096 addresses) can't be scanned at once."
            )));
        }
        let mask = if prefix == 0 {
            0
        } else {
            u32::MAX << (32 - prefix)
        };
        let network = u32::from(ip) & mask;
        let broadcast = network | !mask;
        // A normal network has an address for the network and one for broadcast that hold no host.
        return if prefix >= 31 {
            range(network, broadcast)
        } else {
            range(network + 1, broadcast - 1)
        };
    }
    // 192.168.1.10-50, or 10.0.0.1-10.0.0.40
    if let Some((from, to)) = part.split_once('-')
        && let Ok(start) = from.parse::<Ipv4Addr>()
    {
        let end = match to.parse::<Ipv4Addr>() {
            Ok(end) => u32::from(end),
            Err(_) => {
                let last: u8 = to
                    .parse()
                    .map_err(|_| invalid(format!("“{part}” isn't a range.")))?;
                (u32::from(start) & !0xff) | u32::from(last)
            }
        };
        return range(u32::from(start), end);
    }
    if let Ok(ip) = part.parse::<IpAddr>() {
        return Ok(vec![Item::Ip(ip)]);
    }
    let name_ok = !part.is_empty()
        && part.len() <= 253
        && part
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        && part
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric());
    if name_ok {
        Ok(vec![Item::Name(part.to_string())])
    } else {
        Err(invalid(format!(
            "“{part}” isn't an address, a network or a host name."
        )))
    }
}

/// Splits and expands the text. Host names are left for the caller to look up.
pub fn expand(text: &str) -> Result<Vec<Item>, AppError> {
    let mut out: Vec<Item> = Vec::new();
    for part in text
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|p| !p.is_empty())
    {
        for item in parse_part(part)? {
            if !out.contains(&item) {
                out.push(item);
            }
        }
        if out.len() > MAX_HOSTS {
            return Err(invalid(format!(
                "That is more than {MAX_HOSTS} addresses. Scan a smaller part."
            )));
        }
    }
    if out.is_empty() {
        return Err(invalid(
            "Enter an address, a network such as 192.168.1.0/24, or a range.",
        ));
    }
    Ok(out)
}

/// Expands the text and looks up host names, giving the addresses to scan.
pub async fn resolve(text: &str) -> Result<Vec<IpAddr>, AppError> {
    let mut ips: Vec<IpAddr> = Vec::new();
    for item in expand(text)? {
        match item {
            Item::Ip(ip) => {
                if !ips.contains(&ip) {
                    ips.push(ip);
                }
            }
            Item::Name(name) => {
                let found = tokio::net::lookup_host((name.as_str(), 0))
                    .await
                    .map_err(|_| invalid(format!("Couldn't find the address of “{name}”.")))?;
                // IPv4 first: that is what a home or office network mostly uses.
                let mut found: Vec<IpAddr> = found.map(|a| a.ip()).collect();
                found.sort_by_key(|ip| ip.is_ipv6());
                for ip in found.into_iter().take(2) {
                    if !ips.contains(&ip) {
                        ips.push(ip);
                    }
                }
            }
        }
        if ips.len() > MAX_HOSTS {
            return Err(invalid(format!(
                "That is more than {MAX_HOSTS} addresses. Scan a smaller part."
            )));
        }
    }
    Ok(ips)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ips(text: &str) -> Vec<String> {
        expand(text)
            .unwrap()
            .into_iter()
            .map(|i| match i {
                Item::Ip(ip) => ip.to_string(),
                Item::Name(n) => n,
            })
            .collect()
    }

    #[test]
    fn a_network_skips_its_own_and_broadcast_addresses() {
        let all = ips("192.168.1.0/24");
        assert_eq!(
            (all.len(), all[0].as_str(), all[253].as_str()),
            (254, "192.168.1.1", "192.168.1.254")
        );
        assert_eq!(ips("10.0.0.0/30"), ["10.0.0.1", "10.0.0.2"]);
        assert_eq!(ips("10.0.0.5/32"), ["10.0.0.5"]);
        assert_eq!(ips("10.0.0.4/31"), ["10.0.0.4", "10.0.0.5"]);
        // Any address inside the network names it.
        assert_eq!(ips("192.168.1.77/29")[0], "192.168.1.73");
    }

    #[test]
    fn ranges_work_in_both_spellings() {
        assert_eq!(
            ips("192.168.1.10-12"),
            ["192.168.1.10", "192.168.1.11", "192.168.1.12"]
        );
        assert_eq!(
            ips("10.0.0.254-10.0.1.1"),
            ["10.0.0.254", "10.0.0.255", "10.0.1.0", "10.0.1.1"]
        );
    }

    #[test]
    fn several_items_are_joined_without_repeats_and_names_are_kept_for_lookup() {
        assert_eq!(
            ips("10.0.0.1, 10.0.0.1 10.0.0.2\nserver-1.local"),
            ["10.0.0.1", "10.0.0.2", "server-1.local"]
        );
        assert_eq!(ips("::1"), ["::1"]);
    }

    #[test]
    fn nonsense_and_too_much_are_refused() {
        for bad in [
            "",
            "   ",
            "10.0.0.0/8",
            "10.0.0.0/19",
            "10.0.0.0/x",
            "10.0.0.9-3",
            "10.0.0.1-300",
            "a b!",
            "http://x",
            "-bad",
            "10.0.0.0-10.0.100.0",
            "1.2.3.4/33",
        ] {
            assert!(expand(bad).is_err(), "{bad:?}");
        }
        // Addresses that can never hold a host are dropped; nothing left is an error.
        assert!(expand("224.0.0.1-224.0.0.5").is_err());
        assert!(
            ips("0.0.0.1").len() == 1,
            "a single address is taken as typed"
        );
    }

    #[tokio::test]
    async fn names_are_looked_up() {
        let found = resolve("localhost, 127.0.0.1").await.unwrap();
        assert!(found.iter().any(|ip| ip.is_loopback()));
        assert_eq!(
            found
                .iter()
                .filter(|ip| ip.to_string() == "127.0.0.1")
                .count(),
            1
        );
        assert!(resolve("no-such-host.invalid").await.is_err());
    }
}
