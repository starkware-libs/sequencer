use std::env;
use std::fs::{create_dir, metadata, read_to_string, write, File};
use std::net::{Ipv4Addr, TcpListener};
use std::ops::RangeInclusive;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use std::process::Command;

use nix::sys::stat::{umask, Mode};
use rstest::rstest;
use tempfile::TempDir;

use super::{
    is_port_in_use,
    lock_free_slot,
    overlaps_ephemeral_port_range,
    parse_ephemeral_port_range,
    port_slot_range,
    prepare_lease_dir,
    AvailablePorts,
    DEFAULT_LOWEST_EPHEMERAL_PORT,
    NUM_PORT_SLOTS,
    PORTS_PER_INSTANCE,
    PORT_SLOT_FILE_MODE,
    PORT_SLOT_LEASE_DIR_MODE,
};

// Set for the child process of `port_slot_held_by_another_process_is_not_leased`, to the directory
// that holds the lease directory and the child's result file.
const CHILD_PROCESS_DIR_ENV_VAR: &str = "APOLLO_TEST_PORT_SLOT_CHILD_PROCESS_DIR";
const CHILD_PROCESS_TEST_NAME: &str =
    "test_utils::test_utils_test::child_process_locks_a_free_port_slot";
const CHILD_PROCESS_LEASE_DIR_NAME: &str = "port_slots";
const CHILD_PROCESS_SLOT_INDEX_FILE_NAME: &str = "child_slot_index";

/// A port that is free on every interface when returned. Another process can take it before use.
fn free_port() -> u16 {
    TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap().local_addr().unwrap().port()
}

fn mode_bits(path: &Path) -> u32 {
    metadata(path).unwrap().permissions().mode() & 0o7777
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

/// The kernel can hand a port at or above `DEFAULT_LOWEST_EPHEMERAL_PORT` to an outbound connection
/// while a test is still about to bind it, so no allocated port may fall there.
#[test]
fn allocated_ports_are_below_the_ephemeral_range() {
    let mut available_ports = AvailablePorts::new(0, 0);

    for _ in 0..8 {
        let port = available_ports.get_next_port();
        assert!(
            port < DEFAULT_LOWEST_EPHEMERAL_PORT,
            "Allocated port {port} is inside the ephemeral range starting at \
             {DEFAULT_LOWEST_EPHEMERAL_PORT}"
        );
    }
}

#[test]
fn top_port_slot_ends_below_the_default_ephemeral_range() {
    assert!(port_slot_range(NUM_PORT_SLOTS - 1).end <= DEFAULT_LOWEST_EPHEMERAL_PORT);
}

#[test]
#[should_panic(expected = "No available ports found in range")]
fn exhausting_a_leased_range_panics() {
    let mut available_ports = AvailablePorts::new(0, 0);

    available_ports.get_next_ports(usize::from(PORTS_PER_INSTANCE) + 1);
}

#[test]
fn port_slot_held_by_another_process_is_not_leased() {
    let child_process_dir = TempDir::new().unwrap();
    let lease_dir = child_process_dir.path().join(CHILD_PROCESS_LEASE_DIR_NAME);
    prepare_lease_dir(&lease_dir).unwrap();
    assert_eq!(lock_free_slot(&lease_dir, 0), Some(0));

    let child_output = Command::new(env::current_exe().unwrap())
        .args(["--exact", CHILD_PROCESS_TEST_NAME])
        .env(CHILD_PROCESS_DIR_ENV_VAR, child_process_dir.path())
        .output()
        .unwrap();

    assert!(child_output.status.success(), "{}", String::from_utf8_lossy(&child_output.stdout));
    let child_slot_index =
        read_to_string(child_process_dir.path().join(CHILD_PROCESS_SLOT_INDEX_FILE_NAME)).unwrap();
    assert_eq!(child_slot_index, "1");
}

/// Runs in the child process of `port_slot_held_by_another_process_is_not_leased`, and does nothing
/// otherwise.
#[test]
fn child_process_locks_a_free_port_slot() {
    let Some(child_process_dir) = env::var_os(CHILD_PROCESS_DIR_ENV_VAR) else {
        return;
    };
    let child_process_dir = Path::new(&child_process_dir);

    let slot_index =
        lock_free_slot(&child_process_dir.join(CHILD_PROCESS_LEASE_DIR_NAME), 0).unwrap();

    write(child_process_dir.join(CHILD_PROCESS_SLOT_INDEX_FILE_NAME), slot_index.to_string())
        .unwrap();
}

#[test]
fn slot_search_wraps_around_to_the_first_slot() {
    let lease_dir = TempDir::new().unwrap();
    let top_slot_index = NUM_PORT_SLOTS - 1;

    assert_eq!(lock_free_slot(lease_dir.path(), top_slot_index), Some(top_slot_index));
    assert_eq!(lock_free_slot(lease_dir.path(), top_slot_index), Some(0));
}

/// A symlink to itself fails to open for every user, root included.
#[test]
fn slot_file_that_cannot_be_opened_is_skipped() {
    let lease_dir = TempDir::new().unwrap();
    symlink("slot_0", lease_dir.path().join("slot_0")).unwrap();

    assert_eq!(lock_free_slot(lease_dir.path(), 0), Some(1));
}

#[test]
fn lease_dir_and_slot_file_modes_do_not_depend_on_the_umask() {
    let temp_dir = TempDir::new().unwrap();
    let lease_dir = temp_dir.path().join("port_slots");

    let original_umask = umask(Mode::from_bits_truncate(0o077));
    let lease_dir_status = prepare_lease_dir(&lease_dir);
    let slot_index = lock_free_slot(&lease_dir, 0);
    umask(original_umask);

    assert_eq!(lease_dir_status, Ok(()));
    assert_eq!(slot_index, Some(0));
    assert_eq!(mode_bits(&lease_dir), PORT_SLOT_LEASE_DIR_MODE);
    assert_eq!(mode_bits(&lease_dir.join("slot_0")), PORT_SLOT_FILE_MODE);
}

#[test]
fn existing_lease_dir_with_a_narrower_mode_is_widened() {
    let temp_dir = TempDir::new().unwrap();
    let lease_dir = temp_dir.path().join("port_slots");
    create_dir(&lease_dir).unwrap();
    assert_ne!(mode_bits(&lease_dir), PORT_SLOT_LEASE_DIR_MODE);

    assert_eq!(prepare_lease_dir(&lease_dir), Ok(()));

    assert_eq!(mode_bits(&lease_dir), PORT_SLOT_LEASE_DIR_MODE);
}

#[test]
fn lease_path_that_is_not_a_directory_is_reported() {
    let temp_dir = TempDir::new().unwrap();
    let lease_path = temp_dir.path().join("port_slots");
    File::create(&lease_path).unwrap();

    let lease_dir_error = prepare_lease_dir(&lease_path).unwrap_err();

    assert!(lease_dir_error.contains("is not a directory"), "{lease_dir_error}");
}

#[rstest]
#[case::default_range("32768\t60999\n", Some(32768..=60999))]
#[case::widened_range("1024 65535", Some(1024..=65535))]
#[case::missing_upper_bound("32768\n", None)]
#[case::bound_that_is_not_a_port("32768 70000", None)]
#[case::empty("", None)]
fn ephemeral_port_range_parsing(
    #[case] contents: &str,
    #[case] expected_range: Option<RangeInclusive<u16>>,
) {
    assert_eq!(parse_ephemeral_port_range(contents), expected_range);
}

#[rstest]
#[case::default_range_and_top_slot(NUM_PORT_SLOTS - 1, DEFAULT_LOWEST_EPHEMERAL_PORT..=60999, false)]
#[case::range_covering_the_slot(0, 1024..=65535, true)]
#[case::range_ending_at_the_first_port_of_the_slot(0, 1024..=11000, true)]
#[case::range_ending_below_the_slot(0, 1024..=10999, false)]
#[case::range_starting_at_the_last_port_of_the_slot(0, 11079..=60999, true)]
#[case::range_starting_above_the_slot(0, 11080..=60999, false)]
fn port_slot_overlap_with_the_ephemeral_port_range(
    #[case] slot_index: u16,
    #[case] ephemeral_port_range: RangeInclusive<u16>,
    #[case] expected_overlap: bool,
) {
    assert_eq!(
        overlaps_ephemeral_port_range(&port_slot_range(slot_index), &ephemeral_port_range),
        expected_overlap
    );
}
