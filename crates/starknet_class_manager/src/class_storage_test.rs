use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::felt;
use starknet_api::state::SierraContractClass;
use starknet_sierra_multicompile_types::{RawClass, RawExecutableClass};

use crate::class_storage::{
    ClassHashStorage,
    ClassHashStorageConfig,
    ClassStorage,
    FsClassStorage,
};

#[cfg(test)]
impl ClassHashStorage {
    pub fn new_for_testing() -> Self {
        let config = ClassHashStorageConfig {
            path_prefix: tempfile::tempdir().unwrap().path().to_path_buf(),
            enforce_file_exists: false,
            max_size: 1 << 20, // 1MB.
        };

        Self::new(config).unwrap()
    }
}

#[cfg(test)]
impl FsClassStorage {
    pub fn new_for_testing() -> Self {
        let test_persistent_root = tempfile::tempdir().unwrap().path().to_path_buf();
        let class_hash_storage = ClassHashStorage::new_for_testing();

        Self { persistent_root: test_persistent_root, class_hash_storage }
    }
}

#[test]
fn fs_storage() {
    let mut storage = FsClassStorage::new_for_testing();

    // Non-existent class.
    let class_id = ClassHash(felt!("0x1234"));
    assert_eq!(storage.get_sierra(class_id), Ok(None));
    assert_eq!(storage.get_executable(class_id), Ok(None));
    assert_eq!(storage.get_executable_class_hash(class_id), Ok(None));

    // Add new class.
    let class = RawClass::try_from(SierraContractClass::default()).unwrap();
    // TODO(Elin): consider creating an empty Casm instead of vec (doesn't implement default).
    let executable_class = RawExecutableClass(vec![4, 5, 6].into());
    let executable_class_hash = CompiledClassHash(felt!("0x5678"));
    storage
        .set_class(class_id, class.clone(), executable_class_hash, executable_class.clone())
        .unwrap();

    // Get class.
    assert_eq!(storage.get_sierra(class_id).unwrap(), Some(class.clone()));
    assert_eq!(storage.get_executable(class_id).unwrap(), Some(executable_class.clone()));
    assert_eq!(storage.get_executable_class_hash(class_id).unwrap(), Some(executable_class_hash));

    // Add existing class.
    storage
        .set_class(class_id, class.clone(), executable_class_hash, executable_class.clone())
        .unwrap();
}
