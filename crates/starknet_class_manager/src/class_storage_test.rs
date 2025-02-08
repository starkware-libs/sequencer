use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::felt;
use starknet_api::state::SierraContractClass;
use starknet_sierra_multicompile_types::{RawClass, RawExecutableClass};

use crate::class_storage::{
    create_tmp_dir,
    ClassHashStorage,
    ClassHashStorageConfig,
    ClassHashStorageError,
    ClassStorage,
    FsClassStorage,
    FsClassStorageError,
};

#[cfg(test)]
impl ClassHashStorage {
    pub fn new_for_testing(path_prefix: &tempfile::TempDir) -> Self {
        let config = ClassHashStorageConfig {
            path_prefix: path_prefix.path().to_path_buf(),
            enforce_file_exists: false,
            max_size: 1 << 20, // 1MB.
        };
        Self::new(config).unwrap()
    }
}

#[cfg(test)]
impl FsClassStorage {
    pub fn new_for_testing(
        persistent_root: &tempfile::TempDir,
        class_hash_storage_path_prefix: &tempfile::TempDir,
    ) -> Self {
        let class_hash_storage = ClassHashStorage::new_for_testing(class_hash_storage_path_prefix);
        Self { persistent_root: persistent_root.path().to_path_buf(), class_hash_storage }
    }
}

#[test]
fn fs_storage() {
    let persistent_root = create_tmp_dir().unwrap();
    let class_hash_storage_path_prefix = create_tmp_dir().unwrap();
    let mut storage =
        FsClassStorage::new_for_testing(&persistent_root, &class_hash_storage_path_prefix);

    // Non-existent class.
    let class_id = ClassHash(felt!("0x1234"));
    let class_not_found_error = FsClassStorageError::ClassNotFound { class_id };
    assert_eq!(storage.get_sierra(class_id).unwrap_err(), class_not_found_error);
    assert_eq!(storage.get_executable(class_id).unwrap_err(), class_not_found_error);

    let class_not_found_error =
        FsClassStorageError::ClassHashStorage(ClassHashStorageError::ClassNotFound { class_id });
    assert_eq!(storage.get_executable_class_hash(class_id).unwrap_err(), class_not_found_error);

    // Add new class.
    let class = RawClass::try_from(SierraContractClass::default()).unwrap();
    // TODO(Elin): consider creating an empty Casm instead of vec (doesn't implement default).
    let executable_class = RawExecutableClass(vec![4, 5, 6].into());
    let executable_class_hash = CompiledClassHash(felt!("0x5678"));
    storage
        .set_class(class_id, class.clone(), executable_class_hash, executable_class.clone())
        .unwrap();

    // Get class.
    assert_eq!(storage.get_sierra(class_id).unwrap(), class);
    assert_eq!(storage.get_executable(class_id).unwrap(), executable_class);
    assert_eq!(storage.get_executable_class_hash(class_id).unwrap(), executable_class_hash);

    // Add existing class.
    storage
        .set_class(class_id, class.clone(), executable_class_hash, executable_class.clone())
        .unwrap();
}

// TODO(Elin): check a nonexistent persistent root (should be created).
// TODO(Elin): add unimplemented skeletons for test above and rest of missing tests.
