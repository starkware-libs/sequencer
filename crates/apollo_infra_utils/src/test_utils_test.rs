use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};

use super::{is_port_in_use, AvailablePorts, LOWEST_EPHEMERAL_PORT, PORTS_PER_INSTANCE};

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

/// Ranges leased at the same time must not overlap, which is the property that lets concurrent test
/// processes bind their own ports without coordinating.
#[test]
fn leases_held_at_once_do_not_overlap() {
    let leases: Vec<AvailablePorts> =
        (0..8).map(|instance_index| AvailablePorts::new(0, instance_index)).collect();

    let mut ranges: Vec<(u16, u16)> =
        leases.iter().map(|lease| (lease.start_port, lease.max_port)).collect();
    ranges.sort_unstable();

    for adjacent_ranges in ranges.windows(2) {
        let (_, earlier_end) = adjacent_ranges[0];
        let (later_start, _) = adjacent_ranges[1];
        assert!(
            earlier_end <= later_start,
            "Leased ranges overlap: {:?} and {:?}",
            adjacent_ranges[0],
            adjacent_ranges[1]
        );
    }
}

/// The kernel can hand a port at or above `LOWEST_EPHEMERAL_PORT` to an outbound connection while a
/// test is still about to bind it, so no allocated port may fall there.
#[test]
fn allocated_ports_are_below_the_ephemeral_range() {
    let mut available_ports = AvailablePorts::new(0, 0);

    for _ in 0..8 {
        let port = available_ports.get_next_port();
        assert!(
            port < LOWEST_EPHEMERAL_PORT,
            "Allocated port {port} is inside the ephemeral range starting at \
             {LOWEST_EPHEMERAL_PORT}"
        );
    }
}

#[test]
#[should_panic(expected = "No available ports found in range")]
fn exhausting_a_leased_range_panics() {
    let mut available_ports = AvailablePorts::new(0, 0);

    available_ports.get_next_ports(usize::from(PORTS_PER_INSTANCE) + 1);
}
