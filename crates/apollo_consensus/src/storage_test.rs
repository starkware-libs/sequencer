use assert_matches::assert_matches;
use starknet_api::block::BlockNumber;

use crate::storage::{get_voted_height_storage, HeightVotedStorageError, HeightVotedStorageTrait};
use crate::test_utils::get_new_storage_config;

#[test]
fn read_last_height_when_no_last_height_in_storage() {
    let storage = get_voted_height_storage(get_new_storage_config());
    assert!(storage.get_prev_voted_height().unwrap().is_none());
}

#[test]
fn read_last_height_when_existing_last_height_in_storage() {
    let mut storage = get_voted_height_storage(get_new_storage_config());
    storage.set_prev_voted_height(BlockNumber(1)).unwrap();
    assert_eq!(storage.get_prev_voted_height().unwrap(), Some(BlockNumber(1)));
}

#[test]
fn write_last_height_when_no_last_height_in_storage() {
    let mut storage = get_voted_height_storage(get_new_storage_config());
    assert!(storage.get_prev_voted_height().unwrap().is_none());
    storage.set_prev_voted_height(BlockNumber(1)).unwrap();
    assert_eq!(storage.get_prev_voted_height().unwrap(), Some(BlockNumber(1)));
}

#[test]
fn write_last_height_when_previous_last_height_in_storage() {
    let mut storage = get_voted_height_storage(get_new_storage_config());
    storage.set_prev_voted_height(BlockNumber(1)).unwrap();
    assert_eq!(storage.get_prev_voted_height().unwrap(), Some(BlockNumber(1)));
    storage.set_prev_voted_height(BlockNumber(2)).unwrap();
    assert_eq!(storage.get_prev_voted_height().unwrap(), Some(BlockNumber(2)));
}

#[test]
fn write_last_height_return_error_when_previous_last_height_is_equal() {
    let mut storage = get_voted_height_storage(get_new_storage_config());
    storage.set_prev_voted_height(BlockNumber(2)).unwrap();
    assert_eq!(storage.get_prev_voted_height().unwrap(), Some(BlockNumber(2)));
    assert_matches!(
        storage.set_prev_voted_height(BlockNumber(1)),
        Err(HeightVotedStorageError::InconsistentStorageState { error_msg: _ })
    );
}

#[test]
fn revert_height_when_no_last_height_in_storage_does_nothing() {
    let mut storage = get_voted_height_storage(get_new_storage_config());
    assert!(storage.get_prev_voted_height().unwrap().is_none());
    storage.revert_height(BlockNumber(1)).unwrap();
    assert!(storage.get_prev_voted_height().unwrap().is_none());
}

#[test]
fn revert_height_when_last_height_in_storage_is_lower_than_height_to_revert_to_does_nothing() {
    const HEIGHT_TO_REVERT_TO: BlockNumber = BlockNumber(2);
    // Storage has a lower height than what we revert (so should be a no-op)
    let last_height_in_storage = HEIGHT_TO_REVERT_TO.prev().unwrap();

    let mut storage = get_voted_height_storage(get_new_storage_config());
    storage.set_prev_voted_height(last_height_in_storage).unwrap();
    storage.revert_height(HEIGHT_TO_REVERT_TO).unwrap();
    assert_eq!(storage.get_prev_voted_height().unwrap(), Some(last_height_in_storage));
}

#[test]
fn revert_height_when_last_height_in_storage_is_higher_than_revert_height_reverts_the_given_height()
{
    const HEIGH_TO_REVERT_TO: BlockNumber = BlockNumber(2);
    // Storage has a higher height than what we're reverting.
    let last_height_in_storage = HEIGH_TO_REVERT_TO.unchecked_next().unchecked_next();

    let mut storage = get_voted_height_storage(get_new_storage_config());
    storage.set_prev_voted_height(last_height_in_storage).unwrap();
    storage.revert_height(HEIGH_TO_REVERT_TO).unwrap();
    assert_eq!(storage.get_prev_voted_height().unwrap(), Some(HEIGH_TO_REVERT_TO.prev().unwrap()));
}

#[test]
fn revert_height_when_last_height_in_storage_is_equal_to_revert_height_reverts_the_given_height() {
    const HEIGHT_TO_REVERT_TO: BlockNumber = BlockNumber(2);

    let mut storage = get_voted_height_storage(get_new_storage_config());
    storage.set_prev_voted_height(HEIGHT_TO_REVERT_TO).unwrap();
    storage.revert_height(HEIGHT_TO_REVERT_TO).unwrap();
    assert_eq!(storage.get_prev_voted_height().unwrap(), Some(HEIGHT_TO_REVERT_TO.prev().unwrap()));
}

#[test]
fn revert_height_to_0_clears_the_last_height_in_storage() {
    let mut storage = get_voted_height_storage(get_new_storage_config());
    storage.set_prev_voted_height(BlockNumber(5)).unwrap();
    storage.revert_height(BlockNumber(0)).unwrap();
    assert!(storage.get_prev_voted_height().unwrap().is_none());
}
