use starknet_api::block::BlockNumber;

use crate::class_manager::{ClassManagerStorageReader, ClassManagerStorageWriter};
use crate::test_utils::get_test_storage;

#[tokio::test]
async fn rw_class_manager_marker() {
    let (reader, mut writer) = get_test_storage().0;

    let initial_marker = reader.begin_ro_txn().unwrap().get_class_manager_block_marker().unwrap();
    assert_eq!(initial_marker, BlockNumber(0));

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

#[test]
fn try_revert_class_manager_marker() {
    let (reader, mut writer) = get_test_storage().0;

    writer
        .begin_rw_txn()
        .unwrap()
        .update_class_manager_block_marker(&BlockNumber(2))
        .unwrap()
        .try_revert_class_manager_marker(BlockNumber(2))
        .unwrap()
        .commit()
        .unwrap();

    let cur_marker = reader.begin_ro_txn().unwrap().get_class_manager_block_marker().unwrap();
    assert_eq!(cur_marker, BlockNumber(2));

    writer
        .begin_rw_txn()
        .unwrap()
        .try_revert_class_manager_marker(BlockNumber(3))
        .unwrap()
        .commit()
        .unwrap();
    let cur_marker = reader.begin_ro_txn().unwrap().get_class_manager_block_marker().unwrap();
    assert_eq!(cur_marker, BlockNumber(2));

    writer
        .begin_rw_txn()
        .unwrap()
        .try_revert_class_manager_marker(BlockNumber(1))
        .unwrap()
        .commit()
        .unwrap();
    let cur_marker = reader.begin_ro_txn().unwrap().get_class_manager_block_marker().unwrap();
    assert_eq!(cur_marker, BlockNumber(1));
}
