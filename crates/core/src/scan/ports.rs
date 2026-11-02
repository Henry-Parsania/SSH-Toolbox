//! Which ports to try, and what the well-known ones are called.

use crate::error::{AppError, ErrorCode};

/// What a quick look at a network tries: the services people usually run on their own machines.
pub const COMMON: &[u16] = &[
    21, 22, 23, 25, 53, 80, 110, 111, 135, 139, 143, 389, 443, 445, 465, 587, 631, 993, 995, 1433,
    1521, 2049, 2375, 3000, 3306, 3389, 5000, 5432, 5900, 5985, 6379, 8000, 8080, 8443, 9000,
    9090, 9200, 27017,
];

/// Enough to tell that a computer is there, and what kind: used to find hosts on a network.
pub const DISCOVERY: &[u16] = &[22, 80, 443, 445, 139, 135, 3389, 5900, 21, 23, 8080, 53, 631, 62078];

/// Most ports one scan may cover.
pub const MAX_PORTS: usize = 65_535;

pub fn service_name(port: u16) -> Option<&'static str> {
    Some(match port {
        20 => "FTP data",
        21 => "FTP",
        22 => "SSH",
        23 => "Telnet",
        25 => "SMTP",
        53 => "DNS",
        67 | 68 => "DHCP",
        80 => "HTTP",
        110 => "POP3",
        111 => "RPC",
        123 => "NTP",
        135 => "Windows RPC",
        137..=139 => "NetBIOS",
        143 => "IMAP",
        161 => "SNMP",
        389 => "LDAP",
        443 => "HTTPS",
        445 => "SMB",
        465 => "SMTPS",
        514 => "Syslog",
        587 => "SMTP (submission)",
        631 => "Printing (IPP)",
        636 => "LDAPS",
        873 => "rsync",
        993 => "IMAPS",
        995 => "POP3S",
        1433 => "SQL Server",
        1521 => "Oracle",
        1723 => "VPN (PPTP)",
        1883 => "MQTT",
        2049 => "NFS",
        2375 | 2376 => "Docker",
        3000 => "Web app",
        3306 => "MySQL",
        3389 => "Remote Desktop",
        5000 => "Web app",
        5060 => "SIP",
        5432 => "PostgreSQL",
        5672 => "RabbitMQ",
        5900..=5903 => "VNC",
        5985 | 5986 => "Windows remote management",
        6379 => "Redis",
        8000 | 8080 | 8888 => "HTTP (alternative)",
        8443 => "HTTPS (alternative)",
        9000 => "Web app",
        9090 => "Web app",
        9200 => "Elasticsearch",
        11211 => "Memcached",
        27017 => "MongoDB",
        62078 => "Apple device",
        _ => return None,
    })
}

/// `22,80,8000-8100`, or the words `common` and `all`.
pub fn parse(text: &str) -> Result<Vec<u16>, AppError> {
    let invalid = |m: String| AppError::new(ErrorCode::InvalidInput, m);
    let mut out: Vec<u16> = Vec::new();
    for part in text.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "common" => out.extend_from_slice(COMMON),
            "all" => out.extend(1..=65535u16),
            _ => {
                let number = |s: &str| -> Result<u16, AppError> {
                    match s.parse::<u32>() {
                        Ok(n @ 1..=65535) => Ok(n as u16),
                        _ => Err(invalid(format!("“{s}” isn't a port. Ports are numbers from 1 to 65535."))),
                    }
                };
                match part.split_once('-') {
                    Some((a, b)) => {
                        let (a, b) = (number(a)?, number(b)?);
                        if b < a {
                            return Err(invalid(format!("“{part}” runs backwards.")));
                        }
                        out.extend(a..=b);
                    }
                    None => out.push(number(part)?),
                }
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    if out.is_empty() {
        return Err(invalid("Enter ports such as 22,80,443 or 1-1024, or the word common.".into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_ranges_and_words_are_understood() {
        assert_eq!(parse("80, 22;443").unwrap(), [22, 80, 443]);
        assert_eq!(parse("8000-8003,8001").unwrap(), [8000, 8001, 8002, 8003]);
        assert_eq!(parse("common").unwrap().len(), COMMON.len());
        assert_eq!(parse("ALL").unwrap().len(), 65_535);
        assert_eq!(parse("22,common").unwrap().len(), COMMON.len());
    }

    #[test]
    fn bad_ports_are_refused() {
        for bad in ["", "0", "65536", "x", "80-", "90-80", "1-2-3", "-5"] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn well_known_ports_have_names() {
        assert_eq!(service_name(22), Some("SSH"));
        assert_eq!(service_name(3389), Some("Remote Desktop"));
        assert_eq!(service_name(5901), Some("VNC"));
        assert_eq!(service_name(40000), None);
        assert!(COMMON.iter().all(|p| service_name(*p).is_some()), "every common port is named");
    }
}
