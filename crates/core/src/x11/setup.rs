//! The first message a program sends to an X server, and the cookie in it.
//!
//! The server we log in to is given a made-up cookie. When a program there connects back through
//! the tunnel, we check it brought that cookie and swap in the real one for the local X server,
//! so the real cookie never leaves this computer.

use tokio::io::{AsyncRead, AsyncReadExt};

pub const COOKIE_PROTOCOL: &str = "MIT-MAGIC-COOKIE-1";
pub const COOKIE_LEN: usize = 16;

/// The fixed part of the message, before the name and data of the login.
const HEAD: usize = 12;

fn pad4(n: usize) -> usize {
    (n + 3) & !3
}

fn number(bytes: &[u8], big_endian: bool) -> usize {
    let pair = [bytes[0], bytes[1]];
    usize::from(if big_endian {
        u16::from_be_bytes(pair)
    } else {
        u16::from_le_bytes(pair)
    })
}

/// Reads one whole connection request from a program.
pub async fn read_setup<R: AsyncRead + Unpin>(reader: &mut R) -> std::io::Result<Vec<u8>> {
    let mut head = [0u8; HEAD];
    reader.read_exact(&mut head).await?;
    let big = match head[0] {
        b'B' => true,
        b'l' => false,
        _ => return Err(std::io::Error::other("not an X11 connection")),
    };
    let total = HEAD + pad4(number(&head[6..8], big)) + pad4(number(&head[8..10], big));
    let mut packet = head.to_vec();
    packet.resize(total, 0);
    reader.read_exact(&mut packet[HEAD..]).await?;
    Ok(packet)
}

fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

/// Checks that the request carries `fake` and returns the request to send to the local server:
/// the same, with `real` in place of the made-up cookie. With no real cookie (a local server
/// that lets anyone in) the request is passed on as it is.
///
/// `None` means the program didn't bring the cookie, and the connection must be refused.
pub fn rewrite(packet: &[u8], fake: &[u8], real: Option<&[u8]>) -> Option<Vec<u8>> {
    if packet.len() < HEAD {
        return None;
    }
    let big = match packet[0] {
        b'B' => true,
        b'l' => false,
        _ => return None,
    };
    let name_len = number(&packet[6..8], big);
    let data_len = number(&packet[8..10], big);
    let data_at = HEAD + pad4(name_len);
    if packet.len() < data_at + data_len {
        return None;
    }
    let name = &packet[HEAD..HEAD + name_len];
    let data = &packet[data_at..data_at + data_len];
    if name != COOKIE_PROTOCOL.as_bytes() || !same(data, fake) {
        return None;
    }
    let Some(real) = real else {
        return Some(packet.to_vec());
    };

    let size = |n: usize| {
        let n = n as u16;
        if big { n.to_be_bytes() } else { n.to_le_bytes() }
    };
    let mut out = packet[..6].to_vec();
    out.extend_from_slice(&size(COOKIE_PROTOCOL.len()));
    out.extend_from_slice(&size(real.len()));
    out.extend_from_slice(&packet[10..HEAD]);
    out.extend_from_slice(COOKIE_PROTOCOL.as_bytes());
    out.resize(HEAD + pad4(COOKIE_PROTOCOL.len()), 0);
    out.extend_from_slice(real);
    out.resize(HEAD + pad4(COOKIE_PROTOCOL.len()) + pad4(real.len()), 0);
    Some(out)
}

#[cfg(test)]
pub(crate) fn request(big: bool, cookie: &[u8]) -> Vec<u8> {
    let size = |n: usize| {
        let n = n as u16;
        if big { n.to_be_bytes() } else { n.to_le_bytes() }
    };
    let mut p = vec![if big { b'B' } else { b'l' }, 0];
    p.extend_from_slice(&if big { 11u16.to_be_bytes() } else { 11u16.to_le_bytes() });
    p.extend_from_slice(&[0, 0]);
    p.extend_from_slice(&size(COOKIE_PROTOCOL.len()));
    p.extend_from_slice(&size(cookie.len()));
    p.extend_from_slice(&[0, 0]);
    p.extend_from_slice(COOKIE_PROTOCOL.as_bytes());
    p.resize(HEAD + pad4(COOKIE_PROTOCOL.len()), 0);
    p.extend_from_slice(cookie);
    p.resize(HEAD + pad4(COOKIE_PROTOCOL.len()) + pad4(cookie.len()), 0);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE: [u8; 16] = [7; 16];
    const REAL: [u8; 16] = [9; 16];

    #[test]
    fn swaps_the_cookie_in_both_byte_orders() {
        for big in [false, true] {
            let out = rewrite(&request(big, &FAKE), &FAKE, Some(&REAL)).unwrap();
            assert_eq!(out, request(big, &REAL), "big endian: {big}");
        }
    }

    #[test]
    fn refuses_a_wrong_or_missing_cookie() {
        assert!(rewrite(&request(false, &[1; 16]), &FAKE, Some(&REAL)).is_none());
        assert!(rewrite(&request(false, &[]), &FAKE, Some(&REAL)).is_none());
        assert!(rewrite(b"GET / HTTP/1.1\r\n\r\n", &FAKE, Some(&REAL)).is_none());
        assert!(rewrite(&[], &FAKE, None).is_none());
    }

    #[test]
    fn passes_the_request_on_when_the_local_server_has_no_cookie() {
        let packet = request(false, &FAKE);
        assert_eq!(rewrite(&packet, &FAKE, None).unwrap(), packet);
    }

    #[test]
    fn a_cut_short_request_is_refused() {
        let packet = request(false, &FAKE);
        assert!(rewrite(&packet[..packet.len() - 8], &FAKE, Some(&REAL)).is_none());
    }

    #[tokio::test]
    async fn reads_exactly_one_request() {
        let mut bytes = request(false, &FAKE);
        let length = bytes.len();
        bytes.extend_from_slice(b"extra");
        let mut reader = &bytes[..];
        let packet = read_setup(&mut reader).await.unwrap();
        assert_eq!(packet.len(), length);
        assert_eq!(reader, b"extra");
        assert!(read_setup(&mut &b"nonsense nonsense"[..]).await.is_err());
    }
}
