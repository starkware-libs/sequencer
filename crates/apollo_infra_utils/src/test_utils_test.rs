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
/// interfaces a CI runner has: all of `127.0.0.0/8` is loopback and bindable, and a probe limited
/// to `127.0.0.1` would report this port as free.
#[test]
fn port_held_on_another_interface_is_reported_in_use() {
    let port = free_port();
    let holder_address = SocketAddr::new(Ipv4Addr::new(127, 0, 0, 2).into(), port);
    let _holder = TcpListener::bind(holder_address).unwrap();

    assert!(is_port_in_use(port));
}
