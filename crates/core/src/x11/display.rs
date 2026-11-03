//! Where the local X server is, and the cookie that lets us in.

use std::path::{Path, PathBuf};

use super::setup::{COOKIE_LEN, COOKIE_PROTOCOL};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    Tcp(String, u16),
    #[cfg(unix)]
    Unix(PathBuf),
}

/// An X server on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDisplay {
    pub endpoint: Endpoint,
    /// The display number (the 0 in `:0`).
    pub number: u32,
    pub screen: u32,
    /// The cookie the server wants. `None` for a server that lets anyone on this computer in.
    pub cookie: Option<Vec<u8>>,
}

/// Reads a `DISPLAY` value such as `:0`, `:0.1` or `localhost:10.0`.
pub fn parse_display(text: &str) -> Option<LocalDisplay> {
    let (host, rest) = text.rsplit_once(':')?;
    let (number, screen) = match rest.split_once('.') {
        Some((n, s)) => (n, s),
        None => (rest, "0"),
    };
    let number: u32 = number.parse().ok()?;
    let screen: u32 = screen.parse().ok()?;
    let port = u16::try_from(6000u32.checked_add(number)?).ok()?;
    let endpoint = if host.is_empty() || host == "unix" {
        #[cfg(unix)]
        {
            Endpoint::Unix(PathBuf::from(format!("/tmp/.X11-unix/X{number}")))
        }
        #[cfg(not(unix))]
        {
            Endpoint::Tcp("127.0.0.1".into(), port)
        }
    } else {
        Endpoint::Tcp(host.to_string(), port)
    };
    Some(LocalDisplay {
        endpoint,
        number,
        screen,
        cookie: None,
    })
}

/// One entry of an Xauthority file.
#[derive(Debug, PartialEq, Eq)]
struct Entry {
    family: u16,
    address: Vec<u8>,
    number: Vec<u8>,
    name: Vec<u8>,
    data: Vec<u8>,
}

fn parse_authority(bytes: &[u8]) -> Vec<Entry> {
    fn take<'a>(bytes: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
        if bytes.len() < n {
            return None;
        }
        let (head, tail) = bytes.split_at(n);
        *bytes = tail;
        Some(head)
    }
    fn counted<'a>(bytes: &mut &'a [u8]) -> Option<&'a [u8]> {
        let n = take(bytes, 2)?;
        take(bytes, usize::from(u16::from_be_bytes([n[0], n[1]])))
    }
    let mut rest = bytes;
    let mut entries = Vec::new();
    while !rest.is_empty() {
        let Some(entry) = (|| {
            let family = take(&mut rest, 2)?;
            Some(Entry {
                family: u16::from_be_bytes([family[0], family[1]]),
                address: counted(&mut rest)?.to_vec(),
                number: counted(&mut rest)?.to_vec(),
                name: counted(&mut rest)?.to_vec(),
                data: counted(&mut rest)?.to_vec(),
            })
        })() else {
            break;
        };
        entries.push(entry);
    }
    entries
}

/// The cookie for display `number` in an Xauthority file, if there is one.
pub fn read_cookie(file: &Path, number: u32) -> Option<Vec<u8>> {
    let entries = parse_authority(&std::fs::read(file).ok()?);
    let wanted = number.to_string();
    entries
        .into_iter()
        .rev()
        .find(|e| {
            e.name == COOKIE_PROTOCOL.as_bytes()
                && !e.data.is_empty()
                && (e.number.is_empty() || e.number == wanted.as_bytes())
        })
        .map(|e| e.data)
}

/// Writes an Xauthority file holding one cookie, for an X server we start ourselves. The entry
/// matches any address, and the display number is the server's own.
pub fn write_authority(file: &Path, number: u32, cookie: &[u8]) -> std::io::Result<()> {
    fn put(out: &mut Vec<u8>, bytes: &[u8]) {
        out.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        out.extend_from_slice(bytes);
    }
    let mut out = 0xFFFFu16.to_be_bytes().to_vec(); // any address
    put(&mut out, b"");
    put(&mut out, number.to_string().as_bytes());
    put(&mut out, COOKIE_PROTOCOL.as_bytes());
    put(&mut out, cookie);
    std::fs::write(file, out)
}

/// The Xauthority file a desktop session uses.
pub fn default_authority() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("XAUTHORITY") {
        return Some(path.into());
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".Xauthority"))
}

/// A new random cookie.
pub fn new_cookie() -> Vec<u8> {
    use rand::Rng;
    let mut cookie = vec![0u8; COOKIE_LEN];
    rand::rng().fill_bytes(&mut cookie);
    cookie
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_display_values() {
        let d = parse_display("localhost:10.0").unwrap();
        assert_eq!(d.endpoint, Endpoint::Tcp("localhost".into(), 6010));
        assert_eq!((d.number, d.screen), (10, 0));
        let d = parse_display("192.168.1.5:1.2").unwrap();
        assert_eq!(d.endpoint, Endpoint::Tcp("192.168.1.5".into(), 6001));
        assert_eq!((d.number, d.screen), (1, 2));
        let d = parse_display(":0").unwrap();
        assert_eq!(d.number, 0);
        #[cfg(unix)]
        assert_eq!(d.endpoint, Endpoint::Unix("/tmp/.X11-unix/X0".into()));
        #[cfg(not(unix))]
        assert_eq!(d.endpoint, Endpoint::Tcp("127.0.0.1".into(), 6000));
        assert!(parse_display("nonsense").is_none());
        assert!(parse_display(":x").is_none());
        assert!(parse_display(":70000").is_none());
    }

    #[test]
    fn authority_file_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("xauth");
        let cookie = vec![5u8; 16];
        write_authority(&file, 3, &cookie).unwrap();
        assert_eq!(read_cookie(&file, 3), Some(cookie));
        assert_eq!(read_cookie(&file, 4), None);
    }

    #[test]
    fn finds_the_cookie_for_the_right_display() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("xauth");
        let mut bytes = Vec::new();
        for (family, address, number, data) in [
            (256u16, "host", "0", [1u8; 16]),
            (256, "host", "1", [2u8; 16]),
        ] {
            bytes.extend_from_slice(&family.to_be_bytes());
            for part in [address.as_bytes(), number.as_bytes(), COOKIE_PROTOCOL.as_bytes(), &data] {
                bytes.extend_from_slice(&(part.len() as u16).to_be_bytes());
                bytes.extend_from_slice(part);
            }
        }
        std::fs::write(&file, bytes).unwrap();
        assert_eq!(read_cookie(&file, 1), Some(vec![2u8; 16]));
        assert_eq!(read_cookie(&file, 0), Some(vec![1u8; 16]));
        assert_eq!(read_cookie(&file, 2), None);
        assert_eq!(read_cookie(&dir.path().join("missing"), 0), None);
    }

    #[test]
    fn a_damaged_file_gives_no_cookie() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("xauth");
        std::fs::write(&file, [0u8, 1, 0, 9, 1]).unwrap();
        assert_eq!(read_cookie(&file, 0), None);
    }
}
