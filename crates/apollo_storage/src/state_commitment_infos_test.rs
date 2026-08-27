use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use starknet_api::block::BlockNumber;

use crate::state_commitment_infos::{
    CompressedPayload,
    CompressedStateCommitmentInfos,
    PrunedStateCommitmentInfosPointers,
    StateCommitmentInfosStorageReader,
    StateCommitmentInfosStorageWriter,
    STATE_COMMITMENT_INFOS_VERSION,
};
use crate::test_utils::get_test_storage;
use crate::{StorageReader, StorageWriter};

/// Non-default version, so the round-trip proves the field is stored rather than assumed.
fn dummy_state_commitment_infos() -> CompressedStateCommitmentInfos {
    CompressedStateCommitmentInfos {
        version: STATE_COMMITMENT_INFOS_VERSION + 1,
        payload: CompressedPayload(b"compressed-state-commitment-infos".to_vec()),
    }
}

#[test]
fn append_and_get_state_commitment_infos() {
    let (reader, mut writer) = get_test_storage().0;
    let height = BlockNumber(5);
    let state_commitment_infos = dummy_state_commitment_infos();

    // No infos stored for the height yet.
    assert_eq!(reader.begin_ro_txn().unwrap().get_state_commitment_infos(height).unwrap(), None);

    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_commitment_infos(height, &state_commitment_infos)
        .unwrap()
        .commit()
        .unwrap();

    assert_eq!(
        reader.begin_ro_txn().unwrap().get_state_commitment_infos(height).unwrap(),
        Some(state_commitment_infos)
    );
    // A different height is still empty.
    assert_eq!(
        reader.begin_ro_txn().unwrap().get_state_commitment_infos(BlockNumber(6)).unwrap(),
        None
    );
}

#[test]
fn revert_state_commitment_infos() {
    let (reader, mut writer) = get_test_storage().0;
    let height = BlockNumber(5);

    writer
        .begin_rw_txn()
        .unwrap()
        .append_state_commitment_infos(height, &dummy_state_commitment_infos())
        .unwrap()
        .commit()
        .unwrap();

    assert!(reader.begin_ro_txn().unwrap().has_state_commitment_infos(height).unwrap());

    writer.begin_rw_txn().unwrap().revert_state_commitment_infos(height).unwrap().commit().unwrap();

    assert_eq!(reader.begin_ro_txn().unwrap().get_state_commitment_infos(height).unwrap(), None);
    assert!(!reader.begin_ro_txn().unwrap().has_state_commitment_infos(height).unwrap());

    // Reverting a height with no stored infos is a no-op.
    writer
        .begin_rw_txn()
        .unwrap()
        .revert_state_commitment_infos(BlockNumber(99))
        .unwrap()
        .commit()
        .unwrap();
}

fn store_state_commitment_infos(
    writer: &mut StorageWriter,
    heights: impl IntoIterator<Item = u64>,
    state_commitment_infos: &CompressedStateCommitmentInfos,
) {
    for height in heights {
        writer
            .begin_rw_txn()
            .unwrap()
            .append_state_commitment_infos(BlockNumber(height), state_commitment_infos)
            .unwrap()
            .commit()
            .unwrap();
    }
}

fn prune_state_commitment_infos_pointers(
    writer: &mut StorageWriter,
    prune_below: u64,
    max_deletions: usize,
) -> Option<PrunedStateCommitmentInfosPointers> {
    let (txn, pruned) = writer
        .begin_rw_txn()
        .unwrap()
        .prune_state_commitment_infos_pointers(BlockNumber(prune_below), max_deletions)
        .unwrap();
    txn.commit().unwrap();
    pruned
}

fn stored_heights(reader: &StorageReader, heights: impl IntoIterator<Item = u64>) -> Vec<u64> {
    let txn = reader.begin_ro_txn().unwrap();
    heights
        .into_iter()
        .filter(|height| txn.has_state_commitment_infos(BlockNumber(*height)).unwrap())
        .collect()
}

#[test]
fn prune_state_commitment_infos_pointers_advances_lower_bound() {
    let (reader, mut writer) = get_test_storage().0;
    store_state_commitment_infos(&mut writer, 1..=6, &dummy_state_commitment_infos());

    // Capped by `max_deletions`, lowest heights first; the bound is one past the last removed.
    let pruned = prune_state_commitment_infos_pointers(&mut writer, 5, 2).unwrap();
    assert_eq!(pruned.new_lower_bound, BlockNumber(3));
    let entry_size = pruned.data_end_offset / 2;
    assert!(entry_size > 0);
    assert_eq!(stored_heights(&reader, 1..=6), vec![3, 4, 5, 6]);

    // Stops at `prune_below`; the removed infos end where the next stored ones begin.
    let pruned = prune_state_commitment_infos_pointers(&mut writer, 5, 10).unwrap();
    assert_eq!(pruned.new_lower_bound, BlockNumber(5));
    assert_eq!(pruned.data_end_offset, 4 * entry_size);
    assert_eq!(stored_heights(&reader, 1..=6), vec![5, 6]);

    // Nothing to remove.
    assert_eq!(prune_state_commitment_infos_pointers(&mut writer, 5, 10), None);
    assert_eq!(stored_heights(&reader, 1..=6), vec![5, 6]);

    // Heights that were never stored are skipped and don't count as deletions.
    let pruned = prune_state_commitment_infos_pointers(&mut writer, 100, 10).unwrap();
    assert_eq!(pruned.new_lower_bound, BlockNumber(7));
    assert_eq!(pruned.data_end_offset, 6 * entry_size);
    assert_eq!(stored_heights(&reader, 1..=6), Vec::<u64>::new());
}

fn allocated_bytes(path: &Path) -> usize {
    usize::try_from(std::fs::metadata(path).unwrap().blocks() * 512).unwrap()
}

#[test]
fn prune_state_commitment_infos_data_releases_disk_blocks() {
    let ((reader, mut writer), temp_dir) = get_test_storage();
    const ENTRY_SIZE: usize = 32 * 1024;
    let state_commitment_infos = CompressedStateCommitmentInfos {
        version: STATE_COMMITMENT_INFOS_VERSION,
        payload: CompressedPayload(vec![7; ENTRY_SIZE]),
    };
    store_state_commitment_infos(&mut writer, 1..=4, &state_commitment_infos);

    let file_path = find_file(temp_dir.path(), "state_commitment_infos.dat").unwrap();
    let before = allocated_bytes(&file_path);
    assert!(before >= 4 * ENTRY_SIZE);

    let pruned = prune_state_commitment_infos_pointers(&mut writer, 3, 10).unwrap();
    writer.prune_state_commitment_infos_data(pruned.data_end_offset).unwrap();

    let released = before - allocated_bytes(&file_path);
    assert!(released >= 2 * ENTRY_SIZE - 2 * page_size::get(), "released only {released} bytes");

    // The surviving infos are intact.
    let txn = reader.begin_ro_txn().unwrap();
    assert_eq!(txn.get_state_commitment_infos(BlockNumber(1)).unwrap(), None);
    assert_eq!(
        txn.get_state_commitment_infos(BlockNumber(3)).unwrap(),
        Some(state_commitment_infos.clone())
    );
    assert_eq!(
        txn.get_state_commitment_infos(BlockNumber(4)).unwrap(),
        Some(state_commitment_infos.clone())
    );
    drop(txn);

    // Punching the same range again changes nothing.
    let after = allocated_bytes(&file_path);
    writer.prune_state_commitment_infos_data(pruned.data_end_offset).unwrap();
    assert_eq!(allocated_bytes(&file_path), after);

    // Appending after the punch works as before.
    store_state_commitment_infos(&mut writer, [5], &state_commitment_infos);
    assert_eq!(
        reader.begin_ro_txn().unwrap().get_state_commitment_infos(BlockNumber(5)).unwrap(),
        Some(state_commitment_infos)
    );

    // Removing everything releases everything written.
    let pruned = prune_state_commitment_infos_pointers(&mut writer, 100, 10).unwrap();
    writer.prune_state_commitment_infos_data(pruned.data_end_offset).unwrap();
    assert!(allocated_bytes(&file_path) < ENTRY_SIZE);
}

fn find_file(dir: &Path, file_name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().path()).find_map(|path| {
        if path.is_dir() {
            find_file(&path, file_name)
        } else {
            (path.file_name()? == file_name).then_some(path)
        }
    })
}
