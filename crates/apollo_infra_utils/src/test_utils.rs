use std::env;
use std::fs::{create_dir_all, File, OpenOptions, TryLockError};
use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::Mutex;

use assert_json_diff::{assert_json_matches_no_panic, CompareMode, Config};
use num_enum::IntoPrimitive;
use serde::Serialize;
use socket2::{Domain, Socket, Type};
use strum::EnumCount;
use tracing::instrument;

#[cfg(test)]
#[path = "test_utils_test.rs"]
mod test_utils_test;

const PORTS_PER_INSTANCE: u16 = 80;
const BASE_PORT: u16 = 11000;

/// Lowest port the kernel hands out for outbound connections, per the default
/// `/proc/sys/net/ipv4/ip_local_port_range` of 32768..60999.
///
/// Every port handed out stays below this. A port inside the ephemeral range can be claimed by an
/// outbound connection between `is_port_in_use` returning false and the child process binding it,
/// which surfaces as `Os { code: 98, kind: AddrInUse }` and takes the whole run down.
const LOWEST_EPHEMERAL_PORT: u16 = 32768;

/// Number of port ranges that fit below the ephemeral range.
const NUM_PORT_SLOTS: u16 = (LOWEST_EPHEMERAL_PORT - BASE_PORT) / PORTS_PER_INSTANCE;

const PORT_SLOT_LEASE_DIR_NAME: &str = "apollo_test_port_slots";

const _: () = {
    assert!(NUM_PORT_SLOTS > 0, "No port slots fit below the ephemeral port range.");
    assert!(
        BASE_PORT + NUM_PORT_SLOTS * PORTS_PER_INSTANCE <= LOWEST_EPHEMERAL_PORT,
        "Port slots reach into the ephemeral port range."
    );
};

/// Slots leased by this process, held until it exits.
///
/// Leases are deliberately not released when an `AvailablePorts` is dropped. Call sites take the
/// ports they need and let the `AvailablePorts` go while those ports stay in use for the rest of
/// the test, as `create_hybrid_component_configs` does, so releasing on drop would hand a live
/// range to another test. The OS releases these locks when the process exits, so a crashed test
/// cannot leak a slot either.
static LEASED_PORT_SLOTS: Mutex<Vec<File>> = Mutex::new(Vec::new());

/// Leases a port range that no other process holds, and returns its first port.
///
/// The lease is an exclusive lock on a file per slot, taken in a directory shared machine-wide, so
/// concurrent test processes, including ones run from different checkouts, cannot be handed the
/// same range. `label` identifies the holder in the log line.
fn lease_port_slot(label: &str) -> u16 {
    let lease_dir = env::temp_dir().join(PORT_SLOT_LEASE_DIR_NAME);
    create_dir_all(&lease_dir)
        .unwrap_or_else(|error| panic!("Failed to create {lease_dir:?}: {error}"));

    for slot_index in 0..NUM_PORT_SLOTS {
        let lease_path = lease_dir.join(format!("slot_{slot_index}"));
        let lease_file = open_lease_file(&lease_path);

        match lease_file.try_lock() {
            Ok(()) => {
                let start_port = BASE_PORT + slot_index * PORTS_PER_INSTANCE;
                println!(
                    "Leased port slot {slot_index} [{start_port},{}) for {label}",
                    start_port + PORTS_PER_INSTANCE
                );
                LEASED_PORT_SLOTS
                    .lock()
                    .expect("Port slot lease registry was poisoned.")
                    .push(lease_file);
                return start_port;
            }
            Err(TryLockError::WouldBlock) => continue,
            Err(TryLockError::Error(error)) => {
                panic!("Failed to lock {lease_path:?}: {error}")
            }
        }
    }

    panic!(
        "All {NUM_PORT_SLOTS} port slots below the ephemeral range are leased. Either too many \
         test processes are running at once, or a slot is held by a process that outlived its \
         test."
    );
}

/// Opens a slot's lease file for locking, preferring a read-only handle.
///
/// `flock` does not care whether the handle is writable, and the lease directory is shared by every
/// user on the machine: a file another user created is typically mode 0644, so asking for write
/// access would fail with a permission error on a slot that is perfectly lockable.
fn open_lease_file(lease_path: &Path) -> File {
    match File::open(lease_path) {
        Ok(lease_file) => lease_file,
        Err(error) if error.kind() == ErrorKind::NotFound => OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(lease_path)
            .unwrap_or_else(|error| panic!("Failed to create {lease_path:?}: {error}")),
        Err(error) => panic!("Failed to open {lease_path:?}: {error}"),
    }
}

#[repr(u16)]
#[derive(Debug, Copy, Clone, IntoPrimitive, EnumCount)]
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
    ProofFlowIntegrationTest,
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
    L1EventsProviderUnitTests,
    AnvilStartsWithNoContractTest,
    ClassManagerUnitTests,
    ValidationOnlyNodeNeededForQuorumTest,
}

#[derive(Debug)]
pub struct AvailablePorts {
    start_port: u16,
    current_port: u16,
    max_port: u16,
}

impl AvailablePorts {
    /// Leases a port range for this instance. `test_unique_index` and `instance_index` identify the
    /// holder in the lease log line; the range itself comes from whichever slot is free, so tests
    /// no longer need a statically partitioned budget large enough for every test at once.
    pub fn new(test_unique_index: u16, instance_index: u16) -> Self {
        let start_port =
            lease_port_slot(&format!("test {test_unique_index} instance {instance_index}"));

        AvailablePorts {
            start_port,
            current_port: start_port,
            max_port: start_port + PORTS_PER_INSTANCE,
        }
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
//
// Probes the unspecified address, which is what the servers under test bind. Probing
// `127.0.0.1` only reports a conflict with a holder on the loopback interface, so a port held on
// any other interface passes the probe and then collides on bind with
// `Os { code: 98, kind: AddrInUse }`. CI runners have several interfaces.
fn is_port_in_use(port: u16) -> bool {
    let addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port);
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
