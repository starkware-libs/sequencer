use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::felt;

use crate::class_hash::{ClassHashStorageReader, ClassHashStorageWriter};
use crate::test_utils::get_test_storage;

#[test]
fn class_hash_storage() {
    let ((reader, mut writer), _temp_dir) = get_test_storage();

    // Non-existent entry.
    let class_hash = ClassHash(felt!("0x1234"));
    let executable_class_hash =
        reader.begin_ro_txn().unwrap().get_executable_class_hash_v2(&class_hash).unwrap();
    assert_eq!(executable_class_hash, None);

    // Insert an entry.
    let expected_executable_class_hash_v2 = CompiledClassHash(felt!("0x5678"));
    writer
        .begin_rw_txn()
        .unwrap()
        .set_executable_class_hash_v2(&class_hash, expected_executable_class_hash_v2)
        .unwrap()
        .commit()
        .unwrap();

    // Read the inserted entry.
    let executable_class_hash_v2 =
        reader.begin_ro_txn().unwrap().get_executable_class_hash_v2(&class_hash).unwrap();
    assert_eq!(executable_class_hash_v2, Some(expected_executable_class_hash_v2));
}
