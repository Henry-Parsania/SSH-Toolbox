//! Library check: suppaftp builds with `tokio-rustls-ring` and `deprecated`, and the types
//! needed for explicit and implicit TLS exist.
use suppaftp::tokio::{AsyncRustlsConnector, AsyncRustlsFtpStream};

fn main() {
    // Only names the types: a real login is tested against the vsftpd container in phase 3.
    let _explicit: Option<AsyncRustlsFtpStream> = None;
    let _connector: Option<AsyncRustlsConnector> = None;
    println!("suppaftp OK: async rustls types compile");
}
