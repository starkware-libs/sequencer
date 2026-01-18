use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use assert_json_diff::{assert_json_matches_no_panic, CompareMode, Config};
use fs2::FileExt;
use num_enum::IntoPrimitive;
use serde::Serialize;
use socket2::{Domain, Socket, Type};
use strum::EnumCount;
use strum_macros::EnumCount as EnumCountMacro;
use tracing::instrument;

const PORTS_PER_INSTANCE: u16 = 80;
pub const MAX_NUMBER_OF_INSTANCES_PER_TEST: u16 = 28;
#[allow(clippy::as_conversions)]
const MAX_NUMBER_OF_TESTS: u16 = TestIdentifier::COUNT as u16;
const BASE_PORT: u16 = 11000;

// Ensure available ports don't exceed u16::MAX.
const _: () = {
    assert!(
        BASE_PORT + MAX_NUMBER_OF_TESTS * MAX_NUMBER_OF_INSTANCES_PER_TEST * PORTS_PER_INSTANCE
            < u16::MAX,
        "Port numbers potentially exceeding u16::MAX"
    );
};

pub type Port = u16;
const PORT_FILE_PATH: &str = "/tmp/apollo_infra_port_allocator_offset";
const INITIAL_PORT_OFFSET: Port = 11000;

#[derive(Debug, thiserror::Error)]
pub enum PortFactoryError {
    #[error("Failed to lock file: {0}")]
    LockError(std::io::Error),
    #[error("Failed to open port file: {0}")]
    OpenFileError(std::io::Error),
    #[error("Failed to read file: {0}")]
    ReadError(std::io::Error),
    #[error("Failed to seek file: {0}")]
    SeekError(std::io::Error),
    #[error("Failed to set length: {0}")]
    SetLenError(std::io::Error),
    #[error("Failed to unlock file: {0}")]
    UnlockError(std::io::Error),
    #[error("Failed to write file: {0}")]
    WriteError(std::io::Error),
}

pub struct PortFactory;

impl PortFactory {
    pub fn alloc() -> Result<Port, PortFactoryError> {
        // Open with read/write/create.
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            // Do not overwrite the file if it already exists.
            .truncate(false)
            .open(PORT_FILE_PATH)
            .map_err(PortFactoryError::OpenFileError)?;

        // 1. Lock the file.
        file.lock_exclusive().map_err(PortFactoryError::LockError)?;

        // 2 & 3. Read contents to P (treat empty/missing as 0).
        let mut contents = String::new();
        file.read_to_string(&mut contents).map_err(PortFactoryError::ReadError)?;
        let p: u16 = contents.trim().parse().unwrap_or(0);

        // 4. Increment the file contents.
        file.seek(SeekFrom::Start(0)).map_err(PortFactoryError::SeekError)?;
        file.set_len(0).map_err(PortFactoryError::SetLenError)?;
        write!(file, "{}", p + 1).map_err(PortFactoryError::WriteError)?;

        // 5. Release the lock.
        file.unlock().map_err(PortFactoryError::UnlockError)?;

        Ok(INITIAL_PORT_OFFSET + p)
    }
}

#[repr(u16)]
#[derive(Debug, Copy, Clone, IntoPrimitive, EnumCountMacro)]
// TODO(Nadin): Come up with a better name for this enum.
pub enum TestIdentifier {
    EndToEndFlowTest,
    EndToEndFlowTestBootstrapDeclare,
    EndToEndFlowTestManyTxs,
    EndToEndFlowTestCustomSyscallInvokeTxs,
    EndToEndFlowTestCustomCairo0Txs,
    RevertedL1HandlerTx,
    InfraUnitTests,
    PositiveFlowIntegrationTest,
    RestartFlowIntegrationTest,
    RestartServiceMultipleNodesFlowIntegrationTest,
    RestartServiceSingleNodeFlowIntegrationTest,
    RevertFlowIntegrationTest,
    HttpServerUnitTests,
    SyncFlowIntegrationTest,
    StorageReaderServerUnitTests,
    StorageReaderTypesUnitTests,
    L1EventsScraperEndToEndTest,
    MockedStarknetStateUpdateTest,
    LatestProvedBlockEthereumTest,
    EventsFromOtherContractsTest,
    L1ProviderUnitTests,
    AnvilStartsWithNoContractTest,
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
