use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::hash::StarkHash;

use crate::block_hash::{BlockHashStorageReader, BlockHashStorageWriter};
use crate::test_utils::get_test_storage;

/// Tests that a read-write transaction can read both:
/// 1. Previously committed data from disk
/// 2. Uncommitted writes made within the same transaction
///
/// This verifies libmdbx's "read-your-own-writes" behavior, which is a standard database feature
/// where a transaction's reads automatically see its own uncommitted writes.
#[test]
fn read_your_own_writes() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    let block_number = BlockNumber(42);
    let hash_a = BlockHash(StarkHash::from(100_u128));
    let hash_b = BlockHash(StarkHash::from(200_u128));
    let hash_c = BlockHash(StarkHash::from(300_u128));

    // First, write Val A in a separate transaction and commit it.
    writer.begin_rw_txn().unwrap().set_block_hash(&block_number, hash_a).unwrap().commit().unwrap();

    // Now open a new RW transaction.
    let txn = writer.begin_rw_txn().unwrap();

    // Read (expect Val A- the committed value).
    assert_eq!(txn.get_block_hash(&block_number).unwrap(), Some(hash_a));

    // Write Val B.
    let txn = txn.revert_block_hash(&block_number).unwrap();
    let txn = txn.set_block_hash(&block_number, hash_b).unwrap();

    // Read (expect Val B- our uncommitted write).
    assert_eq!(txn.get_block_hash(&block_number).unwrap(), Some(hash_b));

    // Write Val C.
    let txn = txn.revert_block_hash(&block_number).unwrap();
    let txn = txn.set_block_hash(&block_number, hash_c).unwrap();

    // Read (expect Val C- our uncommitted write).
    assert_eq!(txn.get_block_hash(&block_number).unwrap(), Some(hash_c));

    // Commit.
    txn.commit().unwrap();

    // Verify the final value is persisted.
    let rtxn = reader.begin_ro_txn().unwrap();
    assert_eq!(rtxn.get_block_hash(&block_number).unwrap(), Some(hash_c));
}
