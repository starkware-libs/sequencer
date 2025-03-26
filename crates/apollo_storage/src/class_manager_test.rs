use rstest::rstest;
use starknet_api::block::BlockNumber;

use crate::class_manager::{ClassManagerStorageReader, ClassManagerStorageWriter};
use crate::test_utils::get_test_storage;

#[test]
fn get_class_manager_marker_initial_state() {
    let (reader, _) = get_test_storage().0;

    let marker = reader.begin_ro_txn().unwrap().get_class_manager_block_marker().unwrap();
    assert_eq!(marker, BlockNumber(0));
}

#[test]
fn update_class_manager_marker() {
    let (reader, mut writer) = get_test_storage().0;

    writer
        .begin_rw_txn()
        .unwrap()
        .update_class_manager_block_marker(&BlockNumber(2))
        .unwrap()
        .commit()
        .unwrap();
    let updated_marker = reader.begin_ro_txn().unwrap().get_class_manager_block_marker().unwrap();
    assert_eq!(updated_marker, BlockNumber(2));
}

#[rstest]
#[case::equal_to_current_minus_one(BlockNumber(2), BlockNumber(1), BlockNumber(1))]
#[case::smaller_than_current_minus_one(BlockNumber(2), BlockNumber(0), BlockNumber(2))]
#[case::equal_to_current(BlockNumber(2), BlockNumber(2), BlockNumber(2))]
#[case::larger_than_current(BlockNumber(2), BlockNumber(3), BlockNumber(2))]
fn try_revert_class_manager_marker(
    #[case] initial_block_marker: BlockNumber,
    #[case] target_block_marker: BlockNumber,
    #[case] expected_block_marker: BlockNumber,
) {
    let (reader, mut writer) = get_test_storage().0;

    writer
        .begin_rw_txn()
        .unwrap()
        .update_class_manager_block_marker(&initial_block_marker)
        .unwrap()
        .try_revert_class_manager_marker(target_block_marker)
        .unwrap()
        .commit()
        .unwrap();

    let cur_marker = reader.begin_ro_txn().unwrap().get_class_manager_block_marker().unwrap();
    assert_eq!(cur_marker, expected_block_marker);
}
