use std::fs::{
    create_dir,
    metadata,
    read_to_string,
    set_permissions,
    File,
    OpenOptions,
    Permissions,
    TryLockError,
};
use std::io::ErrorKind;
use std::mem;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::ops::{Range, RangeInclusive};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::LazyLock;

use assert_json_diff::{assert_json_matches_no_panic, CompareMode, Config};
use num_enum::IntoPrimitive;
use serde::Serialize;
use socket2::{Domain, Socket, Type};
use tracing::instrument;

#[cfg(test)]
#[path = "test_utils_test.rs"]
mod test_utils_test;

const PORTS_PER_INSTANCE: u16 = 80;
const BASE_PORT: u16 = 11000;

// Default lower bound of the kernel's ephemeral port range. A port inside that range can be
// assigned to an outbound connection between `is_port_in_use` reporting it free and the server
// under test binding it.
const DEFAULT_LOWEST_EPHEMERAL_PORT: u16 = 32768;
// Holds the ephemeral port range of a Linux host.
const EPHEMERAL_PORT_RANGE_PATH: &str = "/proc/sys/net/ipv4/ip_local_port_range";

// Number of port slots that fit below the default ephemeral port range.
const NUM_PORT_SLOTS: u16 = (DEFAULT_LOWEST_EPHEMERAL_PORT - BASE_PORT) / PORTS_PER_INSTANCE;

// Shared by every process that sees this `/tmp`, whatever its `TMPDIR`. Processes that share a
// network namespace but not this directory can lease the same slot.
const PORT_SLOT_LEASE_DIR: &str = "/tmp/apollo_test_port_slots";
// Writable by every user, with the sticky bit, like `/tmp`.
const PORT_SLOT_LEASE_DIR_MODE: u32 = 0o1777;
const PORT_SLOT_FILE_MODE: u32 = 0o644;

const _: () = {
    assert!(NUM_PORT_SLOTS > 0, "No port slots fit below the default ephemeral port range.");
    assert!(
        BASE_PORT + NUM_PORT_SLOTS * PORTS_PER_INSTANCE <= DEFAULT_LOWEST_EPHEMERAL_PORT,
        "Port slots reach into the default ephemeral port range."
    );
};

static PORT_SLOT_LEASE_DIR_STATUS: LazyLock<Result<(), String>> =
    LazyLock::new(|| prepare_lease_dir(Path::new(PORT_SLOT_LEASE_DIR)));
static EPHEMERAL_PORT_RANGE: LazyLock<RangeInclusive<u16>> = LazyLock::new(ephemeral_port_range);
// Slot index at which the next lease of this process starts its search.
static NEXT_PORT_SLOT_INDEX: AtomicU16 = AtomicU16::new(0);

#[repr(u16)]
#[derive(Debug, Copy, Clone, IntoPrimitive)]
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
    /// Leases a free port range. `test_unique_index` and `instance_index` only identify the holder
    /// in the lease log line.
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

// Checks if a port is occupied, without side effects. Probes the unspecified address, so a port
// held on any interface is reported in use.
fn is_port_in_use(port: u16) -> bool {
    let addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port);
    let socket =
        Socket::new(Domain::IPV4, Type::STREAM, None).expect("Should be able to create a socket.");
    // Enable SO_REUSEADDR, which enables later binding to the address
    socket.set_reuse_address(true).expect("Should be able to set socket properties.");
    socket.bind(&addr.into()).is_err()
}

// Leases a port slot that no other process holds, and returns its first port. `label` identifies
// the holder in the log line.
//
// The lease is an exclusive lock on the slot's file in `PORT_SLOT_LEASE_DIR`. It is held until the
// process exits, since callers drop their `AvailablePorts` while its ports are still in use. It
// relies on the slot files not being deleted while held.
fn lease_port_slot(label: &str) -> u16 {
    if let Err(lease_dir_error) = &*PORT_SLOT_LEASE_DIR_STATUS {
        panic!("{lease_dir_error}");
    }
    let first_slot_index = NEXT_PORT_SLOT_INDEX.load(Ordering::Relaxed);
    let slot_index = lock_free_slot(Path::new(PORT_SLOT_LEASE_DIR), first_slot_index)
        .unwrap_or_else(|| {
            panic!(
                "None of the {NUM_PORT_SLOTS} port slots in {PORT_SLOT_LEASE_DIR} can be leased: \
                 each is held by a running process, or its file cannot be opened."
            )
        });
    NEXT_PORT_SLOT_INDEX.store((slot_index + 1) % NUM_PORT_SLOTS, Ordering::Relaxed);

    let port_range = port_slot_range(slot_index);
    println!("Leased port slot {slot_index} [{},{}) for {label}", port_range.start, port_range.end);
    if overlaps_ephemeral_port_range(&port_range, &EPHEMERAL_PORT_RANGE) {
        eprintln!(
            "Warning: port slot {slot_index} overlaps the ephemeral port range [{},{}] of this \
             host ({EPHEMERAL_PORT_RANGE_PATH}). An outbound connection can take one of its ports \
             before the test binds it.",
            EPHEMERAL_PORT_RANGE.start(),
            EPHEMERAL_PORT_RANGE.end()
        );
    }
    port_range.start
}

fn port_slot_range(slot_index: u16) -> Range<u16> {
    let start_port = BASE_PORT + slot_index * PORTS_PER_INSTANCE;
    start_port..start_port + PORTS_PER_INSTANCE
}

fn overlaps_ephemeral_port_range(
    port_range: &Range<u16>,
    ephemeral_port_range: &RangeInclusive<u16>,
) -> bool {
    port_range.start <= *ephemeral_port_range.end()
        && *ephemeral_port_range.start() < port_range.end
}

// Locks the first free slot at or after `first_slot_index`, wrapping around, and returns its index.
// The lock is held until the process exits. `None` if no slot can be locked.
fn lock_free_slot(lease_dir: &Path, first_slot_index: u16) -> Option<u16> {
    for slot_offset in 0..NUM_PORT_SLOTS {
        let slot_index = (first_slot_index + slot_offset) % NUM_PORT_SLOTS;
        let slot_path = lease_dir.join(format!("slot_{slot_index}"));
        let Some(slot_file) = open_slot_file(&slot_path) else {
            continue;
        };
        match slot_file.try_lock() {
            Ok(()) => {
                // The lock lives as long as the open file.
                mem::forget(slot_file);
                return Some(slot_index);
            }
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(error)) => panic!("Failed to lock {slot_path:?}: {error}"),
        }
    }
    None
}

// Opens a slot file, creating it if it does not exist. `None` if it cannot be opened.
fn open_slot_file(slot_path: &Path) -> Option<File> {
    // Read-only, which suffices for locking and works on a file another user owns.
    if let Ok(slot_file) = File::open(slot_path) {
        return Some(slot_file);
    }
    if let Ok(slot_file) = OpenOptions::new().write(true).create_new(true).open(slot_path) {
        // The creation mode is masked by the umask. A failure leaves a file that other users skip.
        let _ = slot_file.set_permissions(Permissions::from_mode(PORT_SLOT_FILE_MODE));
        return Some(slot_file);
    }
    // Another process can create the file between the two attempts above.
    File::open(slot_path).ok()
}

// Ensures the lease directory exists with at least `PORT_SLOT_LEASE_DIR_MODE`. The error says how
// to repair a directory that another user owns with a narrower mode.
fn prepare_lease_dir(lease_dir: &Path) -> Result<(), String> {
    match create_dir(lease_dir) {
        Err(error) if error.kind() != ErrorKind::AlreadyExists => {
            return Err(format!(
                "Failed to create the port lease directory {lease_dir:?}: {error}"
            ));
        }
        _ => {}
    }
    let lease_dir_metadata = metadata(lease_dir).map_err(|error| {
        format!("Failed to inspect the port lease directory {lease_dir:?}: {error}")
    })?;
    if !lease_dir_metadata.is_dir() {
        return Err(format!("The port lease path {lease_dir:?} is not a directory. Remove it."));
    }
    let lease_dir_mode = lease_dir_metadata.permissions().mode();
    if lease_dir_mode & PORT_SLOT_LEASE_DIR_MODE == PORT_SLOT_LEASE_DIR_MODE {
        return Ok(());
    }
    // The creation mode is masked by the umask, and only the owner can change it.
    set_permissions(lease_dir, Permissions::from_mode(PORT_SLOT_LEASE_DIR_MODE)).map_err(|error| {
        format!(
            "The port lease directory {lease_dir:?} has mode {:o} and setting it to \
             {PORT_SLOT_LEASE_DIR_MODE:o} failed: {error}. As its owner or root, remove it or run \
             `chmod {PORT_SLOT_LEASE_DIR_MODE:o} {}`.",
            lease_dir_mode & 0o7777,
            lease_dir.display()
        )
    })
}

// The ephemeral port range of this host. Falls back to the default lower bound where
// `EPHEMERAL_PORT_RANGE_PATH` cannot be read or parsed.
fn ephemeral_port_range() -> RangeInclusive<u16> {
    read_to_string(EPHEMERAL_PORT_RANGE_PATH)
        .ok()
        .and_then(|contents| parse_ephemeral_port_range(&contents))
        .unwrap_or(DEFAULT_LOWEST_EPHEMERAL_PORT..=u16::MAX)
}

// Parses the contents of `EPHEMERAL_PORT_RANGE_PATH`: the lower and upper bound, both inclusive,
// separated by whitespace.
fn parse_ephemeral_port_range(contents: &str) -> Option<RangeInclusive<u16>> {
    let mut bounds = contents.split_whitespace().map(str::parse::<u16>);
    Some(bounds.next()?.ok()?..=bounds.next()?.ok()?)
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
