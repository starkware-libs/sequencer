use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use tracing::info;

const PORTS_PER_INSTANCE: u16 = 60;
pub const MAX_NUMBER_OF_INSTANCES_PER_TEST: u16 = 10;
const MAX_NUMBER_OF_TESTS: u16 = 10;
const BASE_PORT: u16 = 55000;

// Ensure available ports don't exceed u16::MAX.
const _: () = {
    assert!(
        BASE_PORT + MAX_NUMBER_OF_TESTS * MAX_NUMBER_OF_INSTANCES_PER_TEST * PORTS_PER_INSTANCE
            < u16::MAX,
        "Port numbers potentially exceeding u16::MAX"
    );
};

#[derive(Debug, Copy, Clone)]
// TODO(Nadin): Come up with a better name for this enum.
pub enum TestIdentifier {
    EndToEndFlowTest,
    EndToEndFlowTestManyTxs,
    InfraUnitTests,
    PositiveFlowIntegrationTest,
    RestartFlowIntegrationTest,
    RevertFlowIntegrationTest,
    SystemTestDumpSingleNodeConfig,
}

impl From<TestIdentifier> for u16 {
    fn from(variant: TestIdentifier) -> Self {
        match variant {
            TestIdentifier::EndToEndFlowTest => 0,
            TestIdentifier::EndToEndFlowTestManyTxs => 1,
            TestIdentifier::InfraUnitTests => 2,
            TestIdentifier::PositiveFlowIntegrationTest => 3,
            TestIdentifier::RestartFlowIntegrationTest => 4,
            TestIdentifier::RevertFlowIntegrationTest => 5,
            TestIdentifier::SystemTestDumpSingleNodeConfig => 6,
        }
    }
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
            "Test unique index {:?} exceeded bound {:?}",
            test_unique_index,
            MAX_NUMBER_OF_TESTS
        );
        assert!(
            instance_index < MAX_NUMBER_OF_INSTANCES_PER_TEST,
            "Instance index {:?} exceeded bound {:?}",
            instance_index,
            MAX_NUMBER_OF_INSTANCES_PER_TEST
        );

        let test_offset: u16 =
            test_unique_index * MAX_NUMBER_OF_INSTANCES_PER_TEST * PORTS_PER_INSTANCE;
        let instance_in_test_offset: u16 = instance_index * PORTS_PER_INSTANCE;
        let current_port = BASE_PORT + test_offset + instance_in_test_offset;
        let max_port: u16 = current_port + PORTS_PER_INSTANCE;

        AvailablePorts { start_port: current_port, current_port, max_port }
    }

    pub fn get_next_port(&mut self) -> u16 {
        let port = self.current_port;
        self.current_port += 1;
        assert!(self.current_port < self.max_port, "Exceeded available ports.");
        info!("Allocated port: {} in range [{},{}]", port, self.start_port, self.max_port);
        port
    }

    pub fn get_next_ports(&mut self, n: usize) -> Vec<u16> {
        std::iter::repeat_with(|| self.get_next_port()).take(n).collect()
    }

    pub fn get_next_local_host_socket(&mut self) -> SocketAddr {
        SocketAddr::new(IpAddr::from(Ipv4Addr::LOCALHOST), self.get_next_port())
    }
}

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

    fn next(&mut self) -> Option<Self::Item> {
        let res = Some(AvailablePorts::new(self.test_unique_id, self.instance_index));
        self.instance_index += 1;
        res
    }
}
