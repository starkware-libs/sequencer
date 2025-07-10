use apollo_compile_to_casm_types::{RawClass, RawExecutableClass};
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::felt;
use starknet_api::state::SierraContractClass;

use crate::class_storage::{
    create_tmp_dir,
    ClassHashStorage,
    ClassHashStorageConfig,
    ClassStorage,
    FsClassStorage,
    FsClassStorageError,
};
use crate::config::ClassHashDbConfig;

// TODO(Elin): consider creating an empty Casm instead of vec (doesn't implement default).

#[cfg(test)]
impl ClassHashStorage {
    pub fn new_for_testing(path_prefix: &tempfile::TempDir) -> Self {
        let config = ClassHashStorageConfig {
            class_hash_db_config: ClassHashDbConfig {
                path_prefix: path_prefix.path().to_path_buf(),
                enforce_file_exists: false,
                max_size: 1 << 30,    // 1GB.
                min_size: 1 << 10,    // 1KB.
                growth_step: 1 << 26, // 64MB.
            },
            ..Default::default()
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
    assert_eq!(storage.get_sierra(class_id), Ok(None));
    assert_eq!(storage.get_executable(class_id), Ok(None));
    assert_eq!(storage.get_executable_class_hash_v2(class_id), Ok(None));

    // Add new class.
    let class = RawClass::try_from(SierraContractClass::default()).unwrap();
    let executable_class = RawExecutableClass::new_unchecked(vec![4, 5, 6].into());
    let executable_class_hash_v2 = CompiledClassHash(felt!("0x5678"));
    storage
        .set_class(class_id, class.clone(), executable_class_hash_v2, executable_class.clone())
        .unwrap();

    // Get class.
    assert_eq!(storage.get_sierra(class_id).unwrap(), Some(class.clone()));
    assert_eq!(storage.get_executable(class_id).unwrap(), Some(executable_class.clone()));
    assert_eq!(
        storage.get_executable_class_hash_v2(class_id).unwrap(),
        Some(executable_class_hash_v2)
    );

    // Add existing class.
    storage
        .set_class(class_id, class.clone(), executable_class_hash_v2, executable_class.clone())
        .unwrap();
}

#[test]
fn fs_storage_deprecated_class_api() {
    let persistent_root = create_tmp_dir().unwrap();
    let class_hash_storage_path_prefix = create_tmp_dir().unwrap();
    let mut storage =
        FsClassStorage::new_for_testing(&persistent_root, &class_hash_storage_path_prefix);

    // Non-existent class.
    let class_id = ClassHash(felt!("0x1234"));
    assert_eq!(storage.get_deprecated_class(class_id), Ok(None));

    // Add new class.
    let executable_class = RawExecutableClass::new_unchecked(vec![4, 5, 6].into());
    storage.set_deprecated_class(class_id, executable_class.clone()).unwrap();

    // Get class.
    assert_eq!(storage.get_deprecated_class(class_id).unwrap(), Some(executable_class.clone()));

    // Add existing class.
    storage.set_deprecated_class(class_id, executable_class).unwrap();
}

// TODO(Elin): check a nonexistent persistent root (should be created).
// TODO(Elin): add unimplemented skeletons for test above and rest of missing tests.

/// This scenario simulates a (manual) DB corruption; e.g., files were deleted.
// TODO(Elin): should this flow return an error?
#[test]
fn fs_storage_partial_write_only_atomic_marker() {
    let persistent_root = create_tmp_dir().unwrap();
    let class_hash_storage_path_prefix = create_tmp_dir().unwrap();
    let mut storage =
        FsClassStorage::new_for_testing(&persistent_root, &class_hash_storage_path_prefix);

    // Write only atomic marker, no class files.
    let class_id = ClassHash(felt!("0x1234"));
    let executable_class_hash_v2 = CompiledClassHash(felt!("0x5678"));
    storage.mark_class_id_as_existent(class_id, executable_class_hash_v2).unwrap();

    // Query class, should be considered an erroneous flow.
    let class_not_found_error = FsClassStorageError::ClassNotFound { class_id };
    assert_eq!(storage.get_sierra(class_id).unwrap_err(), class_not_found_error);
    assert_eq!(storage.get_executable(class_id).unwrap_err(), class_not_found_error);
}

#[test]
fn fs_storage_partial_write_no_atomic_marker() {
    let persistent_root = create_tmp_dir().unwrap();
    let class_hash_storage_path_prefix = create_tmp_dir().unwrap();
    let storage =
        FsClassStorage::new_for_testing(&persistent_root, &class_hash_storage_path_prefix);

    // Fully write class files, without atomic marker.
    let class_id = ClassHash(felt!("0x1234"));
    let class = RawClass::try_from(SierraContractClass::default()).unwrap();
    let executable_class = RawExecutableClass::new_unchecked(vec![4, 5, 6].into());
    storage.write_class_atomically(class_id, class, executable_class).unwrap();
    assert_eq!(storage.get_executable_class_hash_v2(class_id), Ok(None));

    // Query class, should be considered non-existent.
    assert_eq!(storage.get_sierra(class_id), Ok(None));
    assert_eq!(storage.get_executable(class_id), Ok(None));
}
