use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use assert_json_diff::{assert_json_matches_no_panic, CompareMode, Config};
use num_enum::IntoPrimitive;
use serde::Serialize;
use socket2::{Domain, Socket, Type};
use tracing::{info, instrument};

const PORTS_PER_INSTANCE: u16 = 60;
pub const MAX_NUMBER_OF_INSTANCES_PER_TEST: u16 = 28;
const MAX_NUMBER_OF_TESTS: u16 = 10;
const BASE_PORT: u16 = 11000;

// Ensure available ports don't exceed u16::MAX.
const _: () = {
    assert!(
        BASE_PORT + MAX_NUMBER_OF_TESTS * MAX_NUMBER_OF_INSTANCES_PER_TEST * PORTS_PER_INSTANCE
            < u16::MAX,
        "Port numbers potentially exceeding u16::MAX"
    );
};

#[repr(u16)]
#[derive(Debug, Copy, Clone, IntoPrimitive)]
// TODO(Nadin): Come up with a better name for this enum.
pub enum TestIdentifier {
    EndToEndFlowTest,
    EndToEndFlowTestBootstrapDeclare,
    EndToEndFlowTestManyTxs,
    EndToEndFlowTestCustomInvokeTxs,
    InfraUnitTests,
    PositiveFlowIntegrationTest,
    RestartFlowIntegrationTest,
    RevertFlowIntegrationTest,
    SystemTestDumpSingleNodeConfig,
    HttpServerUnitTests,
    SyncFlowIntegrationTest,
}

#[derive(Debug)]
pub struct AvailablePorts {
    start_port: u16,
    current_port: u16,
    max_port: u16,
}

impl AvailablePorts {
    pub fn new(test_unique_index: u16, instance_index: u16) -> Self {
        assert!(
            test_unique_index < MAX_NUMBER_OF_TESTS,
            "Test unique index {test_unique_index:?} exceeded bound {MAX_NUMBER_OF_TESTS:?}"
        );
        assert!(
            instance_index < MAX_NUMBER_OF_INSTANCES_PER_TEST,
            "Instance index {instance_index:?} exceeded bound {MAX_NUMBER_OF_INSTANCES_PER_TEST:?}",
        );

        let test_offset: u16 =
            test_unique_index * MAX_NUMBER_OF_INSTANCES_PER_TEST * PORTS_PER_INSTANCE;
        let instance_in_test_offset: u16 = instance_index * PORTS_PER_INSTANCE;
        let current_port = BASE_PORT + test_offset + instance_in_test_offset;
        let max_port: u16 = current_port + PORTS_PER_INSTANCE;

        AvailablePorts { start_port: current_port, current_port, max_port }
    }

    #[instrument]
    pub fn get_next_port(&mut self) -> u16 {
        while self.current_port < self.max_port {
            let port = self.current_port;
            self.current_port += 1;

            if is_port_in_use(port) {
                info!(
                    "Skipping occupied port: {} in range [{},{}]",
                    port, self.start_port, self.max_port
                );
            } else {
                info!("Allocated port: {} in range [{},{}]", port, self.start_port, self.max_port);
                return port;
            }
        }

        panic!("No available ports found in range [{},{}]", self.start_port, self.max_port);
    }

    pub fn get_next_ports(&mut self, n: usize) -> Vec<u16> {
        std::iter::repeat_with(|| self.get_next_port()).take(n).collect()
    }

    #[instrument]
    pub fn get_next_local_host_socket(&mut self) -> SocketAddr {
        SocketAddr::new(IpAddr::from(Ipv4Addr::LOCALHOST), self.get_next_port())
    }
}

// Checks if a port is occupied, without side effects.
fn is_port_in_use(port: u16) -> bool {
    let addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port);
    let socket =
        Socket::new(Domain::IPV4, Type::STREAM, None).expect("Should be able to create a socket.");
    // Enable SO_REUSEADDR, which enables later binding to the address
    socket.set_reuse_address(true).expect("Should be able to set socket properties.");
    socket.bind(&addr.into()).is_err()
}

#[derive(Debug)]
pub struct AvailablePortsGenerator {
    test_unique_id: u16,
    instance_index: u16,
}

impl AvailablePortsGenerator {
    pub fn new(test_unique_id: u16) -> Self {
        Self { test_unique_id, instance_index: 0 }
    }
}

impl Iterator for AvailablePortsGenerator {
    type Item = AvailablePorts;

    #[instrument]
    fn next(&mut self) -> Option<Self::Item> {
        let res = Some(AvailablePorts::new(self.test_unique_id, self.instance_index));
        self.instance_index += 1;
        res
    }
}

/// Compare two JSON values for an exact match.
///
/// Extends the functionality of [`assert_json_diff::assert_json_eq`] by also adding a customizable
/// error message print. Uses [`assert_json_matches_no_panic`].
pub fn assert_json_eq<Lhs, Rhs>(lhs: &Lhs, rhs: &Rhs, message: String)
where
    Lhs: Serialize,
    Rhs: Serialize,
{
    if let Err(error) = assert_json_matches_no_panic(lhs, rhs, Config::new(CompareMode::Strict)) {
        let printed_error = format!("\n\n{message}\n{error}\n\n");
        panic!("{}", printed_error);
    }
}
