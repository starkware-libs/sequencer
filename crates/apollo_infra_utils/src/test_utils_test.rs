use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};

use super::is_port_in_use;

/// A free port, taken from the ephemeral range so it cannot collide with the ranges
/// `AvailablePorts` hands out.
fn free_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap().local_addr().unwrap().port()
}

#[test]
fn unheld_port_is_reported_free() {
    assert!(!is_port_in_use(free_port()));
}

#[test]
fn port_held_on_the_unspecified_address_is_reported_in_use() {
    let port = free_port();
    let _holder = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).unwrap();

    assert!(is_port_in_use(port));
}

/// A holder on an interface other than `127.0.0.1` still blocks a later bind on the unspecified
/// address, so the probe must report the port as in use. `127.0.0.2` stands in for the non-loopback
/// interfaces a CI runner has, and a probe limited to `127.0.0.1` would report this port as free.
///
/// Linux assigns all of `127.0.0.0/8` to loopback, so the holder binds there. macOS assigns only
/// `127.0.0.1` unless an alias was added, and this scenario cannot be expressed without a second
/// local address, so the test reports that and returns rather than failing on the environment. CI
/// runs on Linux, where the assertion always executes.
#[test]
fn port_held_on_another_interface_is_reported_in_use() {
    let port = free_port();
    let holder_address = SocketAddr::new(Ipv4Addr::new(127, 0, 0, 2).into(), port);
    let holder = match TcpListener::bind(holder_address) {
        Ok(holder) => holder,
        Err(error) if error.kind() == ErrorKind::AddrNotAvailable => {
            println!(
                "Skipping: {holder_address} is not a local address on this host, so a second \
                 interface cannot be simulated."
            );
            return;
        }
        Err(error) => panic!("Failed to bind {holder_address}: {error}"),
    };

    assert!(is_port_in_use(port));
    drop(holder);
}
