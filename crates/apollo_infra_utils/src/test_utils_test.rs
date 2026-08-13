use std::net::{Ipv4Addr, TcpListener};

use super::is_port_in_use;

/// A port that is free on every interface when returned. Another process can take it before use.
fn free_port() -> u16 {
    TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap().local_addr().unwrap().port()
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

/// A holder on an interface other than `127.0.0.1` blocks a later bind on the unspecified address,
/// so the probe reports the port in use. Linux-only: Linux assigns all of `127.0.0.0/8` to
/// loopback, so `127.0.0.2` is a second local address.
#[cfg(target_os = "linux")]
#[test]
fn port_held_on_another_interface_is_reported_in_use() {
    let port = free_port();
    let _holder = TcpListener::bind((Ipv4Addr::new(127, 0, 0, 2), port)).unwrap();

    assert!(is_port_in_use(port));
}
