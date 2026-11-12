use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use assert_json_diff::{assert_json_matches_no_panic, CompareMode, Config};
use num_enum::IntoPrimitive;
use serde::Serialize;
use socket2::{Domain, Socket, Type};
use strum::EnumCount;
use tracing::instrument;

const PORTS_PER_INSTANCE: u16 = 80;
pub const MAX_NUMBER_OF_INSTANCES_PER_TEST: u16 = 26;
#[allow(clippy::as_conversions)]
const MAX_NUMBER_OF_TESTS: u16 = TestIdentifier::COUNT as u16;
// Below the kernel's ephemeral port range (32768-60999 on Linux), from which outgoing connections
// get their local ports. A node's peers dial it before it listens, and a dial may get the target
// port itself as its local port and connect to itself, so the node fails to bind it.
const BASE_PORT: u16 = 1500;
const EPHEMERAL_PORT_RANGE_START: u16 = 32768;

// Ensure available ports don't exceed u16::MAX.
const _: () = {
    assert!(
        BASE_PORT + MAX_NUMBER_OF_TESTS * MAX_NUMBER_OF_INSTANCES_PER_TEST * PORTS_PER_INSTANCE
            < u16::MAX,
        "Port numbers potentially exceeding u16::MAX"
    );
};

// Ensure the tests whose nodes dial each other, up to `ValidationOnlyNodeNeededForQuorumTest`, use
// ports below the ephemeral port range.
#[allow(clippy::as_conversions)]
const _: () = {
    assert!(
        BASE_PORT
            + (TestIdentifier::ValidationOnlyNodeNeededForQuorumTest as u16 + 1)
                * MAX_NUMBER_OF_INSTANCES_PER_TEST
                * PORTS_PER_INSTANCE
            <= EPHEMERAL_PORT_RANGE_START,
        "Ports of tests with nodes dialing each other reach the ephemeral port range"
    );
};

#[repr(u16)]
#[derive(Debug, Copy, Clone, IntoPrimitive, EnumCount)]
// TODO(Nadin): Come up with a better name for this enum.
// Tests whose nodes dial each other must come up to `ValidationOnlyNodeNeededForQuorumTest`, so
// their ports are below the ephemeral port range.
pub enum TestIdentifier {
    EndToEndFlowTest,
    EndToEndFlowTestBootstrapDeclare,
    EndToEndFlowTestManyTxs,
    EndToEndFlowTestCustomSyscallInvokeTxs,
    EndToEndFlowTestCustomCairo0Txs,
    RevertedL1HandlerTx,
    InfraUnitTests,
    PositiveFlowIntegrationTest,
    ProofFlowIntegrationTest,
    RestartFlowIntegrationTest,
    RestartServiceMultipleNodesFlowIntegrationTest,
    RestartServiceSingleNodeFlowIntegrationTest,
    RevertFlowIntegrationTest,
    SyncFlowIntegrationTest,
    ValidationOnlyNodeNeededForQuorumTest,
    StorageReaderServerUnitTests,
    StorageReaderTypesUnitTests,
    L1EventsScraperEndToEndTest,
    MockedStarknetStateUpdateTest,
    LatestProvedBlockEthereumTest,
    EventsFromOtherContractsTest,
    L1EventsProviderUnitTests,
    AnvilStartsWithNoContractTest,
    ClassManagerUnitTests,
    HttpServerUnitTests,
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
                println!(
                    "Skipping occupied port: {port} in range [{},{}]",
                    self.start_port, self.max_port
                );
            } else {
                println!("Allocated port: {port} in range [{},{}]", self.start_port, self.max_port);
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
