//! Library check: tokio-serial builds and lists ports.
fn main() {
    let ports = tokio_serial::available_ports().expect("list ports");
    println!("tokio-serial OK: {} port(s) found", ports.len());
    for p in ports {
        println!("  {} ({:?})", p.port_name, p.port_type);
    }
}
