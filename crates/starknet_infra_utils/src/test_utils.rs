use std::net::{IpAddr, Ipv4Addr, SocketAddr};

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
pub enum TestIdentifier {
    EndToEndIntegrationTest,
    EndToEndFlowTest,
    MempoolSendsTxToOtherPeerTest,
    MempoolReceivesTxFromOtherPeerTest,
    InfraUnitTests,
}

impl From<TestIdentifier> for u16 {
    fn from(variant: TestIdentifier) -> Self {
        match variant {
            TestIdentifier::EndToEndIntegrationTest => 0,
            TestIdentifier::EndToEndFlowTest => 1,
            TestIdentifier::MempoolSendsTxToOtherPeerTest => 2,
            TestIdentifier::MempoolReceivesTxFromOtherPeerTest => 3,
            TestIdentifier::InfraUnitTests => 4,
        }
    }
}

#[derive(Debug)]
pub struct AvailablePorts {
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

        AvailablePorts { current_port, max_port }
    }

    pub fn get_next_port(&mut self) -> u16 {
        let port = self.current_port;
        self.current_port += 1;
        assert!(self.current_port < self.max_port, "Exceeded available ports.");

        port
    }

    pub fn get_next_ports(&mut self, n: usize) -> Vec<u16> {
        std::iter::repeat_with(|| self.get_next_port()).take(n).collect()
    }

    pub fn get_next_local_host_socket(&mut self) -> SocketAddr {
        SocketAddr::new(IpAddr::from(Ipv4Addr::LOCALHOST), self.get_next_port())
    }
}
