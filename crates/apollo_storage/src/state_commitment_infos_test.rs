use starknet_api::block::BlockNumber;

use crate::state_commitment_infos::{
    StateCommitmentInfosStorageReader,
    StateCommitmentInfosStorageWriter,
};
use crate::test_utils::get_test_storage;

// Storage persists the witness verbatim, so a plain string stands in for the real payload.
fn sample_state_commitment_infos() -> String {
    "compressed-state-commitment-infos".to_string()
}

#[test]
fn append_and_get_state_commitment_infos() {
    let (reader, mut writer) = get_test_storage().0;
    let height = BlockNumber(5);
    let state_commitment_infos = sample_state_commitment_infos();

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
        .append_state_commitment_infos(height, &sample_state_commitment_infos())
        .unwrap()
        .revert_state_commitment_infos(height)
        .unwrap()
        .commit()
        .unwrap();

    assert_eq!(reader.begin_ro_txn().unwrap().get_state_commitment_infos(height).unwrap(), None);

    // Reverting a height with no stored infos is a no-op.
    writer
        .begin_rw_txn()
        .unwrap()
        .revert_state_commitment_infos(BlockNumber(99))
        .unwrap()
        .commit()
        .unwrap();
}
