use std::path::PathBuf;

use apollo_class_manager_config::config::{CachedClassStorageConfig, ClassHashDbConfig};
use apollo_class_manager_types::CachedClassStorageError;
use apollo_compile_to_casm_types::{RawClass, RawExecutableClass};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::felt;
use starknet_api::state::SierraContractClass;

use crate::class_storage::{
    CachedClassStorage,
    ClassHashStorage,
    ClassHashStorageConfig,
    ClassStorage,
    FsClassStorage,
    FsClassStorageError,
};

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
                max_readers: 1 << 13, // 8K readers
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
        std::fs::create_dir_all(persistent_root.path()).unwrap();
        Self { persistent_root: persistent_root.path().to_path_buf(), class_hash_storage }
    }
}

#[test]
fn fs_storage() {
    let persistent_root = tempfile::tempdir().unwrap();
    let class_hash_storage_path_prefix = tempfile::tempdir().unwrap();
    let mut storage =
        FsClassStorage::new_for_testing(&persistent_root, &class_hash_storage_path_prefix);

    // Non-existent class.
    let class_id = ClassHash(felt!("0x1234"));
    assert_eq!(storage.get_sierra(class_id), Ok(None));
    assert_eq!(storage.get_executable(class_id), Ok(None));
    assert_eq!(storage.get_executable_class_hash_v2(class_id), Ok(None));

    // Add new class.
    let class = RawClass::try_from(SierraContractClass::default()).unwrap();
    let executable_class =
        RawExecutableClass::try_from(ContractClass::test_casm_contract_class()).unwrap();
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
    let persistent_root = tempfile::tempdir().unwrap();
    let class_hash_storage_path_prefix = tempfile::tempdir().unwrap();
    let mut storage =
        FsClassStorage::new_for_testing(&persistent_root, &class_hash_storage_path_prefix);

    // Non-existent class.
    let class_id = ClassHash(felt!("0x1234"));
    assert_eq!(storage.get_deprecated_class(class_id), Ok(None));

    // Add new class.
    let executable_class =
        RawExecutableClass::try_from(ContractClass::test_casm_contract_class()).unwrap();
    storage.set_deprecated_class(class_id, executable_class.clone()).unwrap();

    // Get class.
    assert_eq!(storage.get_deprecated_class(class_id).unwrap(), Some(executable_class.clone()));

    // Add existing class.
    storage.set_deprecated_class(class_id, executable_class).unwrap();
}

#[test]
fn temp_dir_location_and_atomic_write_layout() {
    let persistent_root = tempfile::tempdir().unwrap();
    let class_hash_storage_path_prefix = tempfile::tempdir().unwrap();
    let storage =
        FsClassStorage::new_for_testing(&persistent_root, &class_hash_storage_path_prefix);

    let class_id = ClassHash(felt!("0x1234"));
    let persistent_dir = storage.persistent_root.join({
        let hex = hex::encode(class_id.to_bytes_be());
        let (a, b) = (&hex[..2], &hex[2..4]);
        PathBuf::from(a).join(b).join(hex)
    });
    let parent_dir = persistent_dir.parent().unwrap().to_path_buf();

    // Create tmp dir via the atomic writer and ensure it resides under parent_dir.
    let class = RawClass::try_from(SierraContractClass::default()).unwrap();
    let executable_class =
        RawExecutableClass::try_from(ContractClass::test_casm_contract_class()).unwrap();
    storage.write_class_atomically(class_id, class.clone(), executable_class.clone()).unwrap();

    // After atomic write and rename, the persistent dir should exist and contain files.
    assert!(persistent_dir.exists());
    assert!(parent_dir.exists());
    assert!(persistent_dir.join("sierra").exists());
    assert!(persistent_dir.join("casm").exists());
}

#[test]
fn fs_storage_nonexistent_persistent_root_is_created() {
    let parent_dir = tempfile::tempdir().unwrap();
    let nonexistent_root = parent_dir.path().join("nonexistent_root");
    assert!(!nonexistent_root.exists());

    let class_hash_storage_path_prefix = tempfile::tempdir().unwrap();
    let class_hash_storage = ClassHashStorage::new_for_testing(&class_hash_storage_path_prefix);
    let mut storage =
        FsClassStorage { persistent_root: nonexistent_root.clone(), class_hash_storage };

    // Write a new class, which should create the persistent root directories as needed.
    let class_id = ClassHash(felt!("0x1234"));
    let class = RawClass::try_from(SierraContractClass::default()).unwrap();
    let executable_class =
        RawExecutableClass::try_from(ContractClass::test_casm_contract_class()).unwrap();
    let executable_class_hash_v2 = CompiledClassHash(felt!("0x5678"));
    storage
        .set_class(class_id, class.clone(), executable_class_hash_v2, executable_class.clone())
        .unwrap();

    assert!(nonexistent_root.exists());

    assert_eq!(storage.get_sierra(class_id).unwrap(), Some(class));
    assert_eq!(storage.get_executable(class_id).unwrap(), Some(executable_class));
}

// TODO(Elin): add unimplemented skeletons for test above and rest of missing tests.

/// This scenario simulates a (manual) DB corruption; e.g., files were deleted.
// TODO(Elin): should this flow return an error?
#[test]
fn fs_storage_partial_write_only_atomic_marker() {
    let persistent_root = tempfile::tempdir().unwrap();
    let class_hash_storage_path_prefix = tempfile::tempdir().unwrap();
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
    let persistent_root = tempfile::tempdir().unwrap();
    let class_hash_storage_path_prefix = tempfile::tempdir().unwrap();
    let storage =
        FsClassStorage::new_for_testing(&persistent_root, &class_hash_storage_path_prefix);

    // Fully write class files, without atomic marker.
    let class_id = ClassHash(felt!("0x1234"));
    let class = RawClass::try_from(SierraContractClass::default()).unwrap();
    let executable_class =
        RawExecutableClass::try_from(ContractClass::test_casm_contract_class()).unwrap();
    storage.write_class_atomically(class_id, class, executable_class).unwrap();
    assert_eq!(storage.get_executable_class_hash_v2(class_id), Ok(None));

    // Query class, should be considered non-existent.
    assert_eq!(storage.get_sierra(class_id), Ok(None));
    assert_eq!(storage.get_executable(class_id), Ok(None));
}

#[test]
fn cached_storage_none_flows_do_not_cache() {
    let persistent_root = tempfile::tempdir().unwrap();
    let class_hash_storage_path_prefix = tempfile::tempdir().unwrap();
    let fs_storage =
        FsClassStorage::new_for_testing(&persistent_root, &class_hash_storage_path_prefix);

    let cached = CachedClassStorage::new(CachedClassStorageConfig::default(), fs_storage);

    let class_id = ClassHash(felt!("0x1111"));
    // Neither Cairo 1 nor Cairo 0 class exists.
    assert_eq!(cached.get_executable(class_id), Ok(None));
    assert_eq!(cached.get_deprecated_class(class_id), Ok(None));
    assert_eq!(cached.get_executable_class_hash_v2(class_id), Ok(None));
}

#[test]
fn cached_storage_cairo1_marker_only_returns_error() {
    let persistent_root = tempfile::tempdir().unwrap();
    let class_hash_storage_path_prefix = tempfile::tempdir().unwrap();
    let mut fs_storage =
        FsClassStorage::new_for_testing(&persistent_root, &class_hash_storage_path_prefix);

    let cached = CachedClassStorage::new(CachedClassStorageConfig::default(), fs_storage.clone());

    let class_id = ClassHash(felt!("0x1111"));
    let executable_class_hash_v2 = CompiledClassHash(felt!("0x2222"));
    // Simulate marker exists without files.
    fs_storage.mark_class_id_as_existent(class_id, executable_class_hash_v2).unwrap();

    let expected_err =
        CachedClassStorageError::Storage(FsClassStorageError::ClassNotFound { class_id });
    assert_eq!(cached.get_executable(class_id).unwrap_err(), expected_err);
}

#[test]
fn cached_storage_cairo1_get_executable_and_hash() {
    let persistent_root = tempfile::tempdir().unwrap();
    let class_hash_storage_path_prefix = tempfile::tempdir().unwrap();
    let mut fs_storage =
        FsClassStorage::new_for_testing(&persistent_root, &class_hash_storage_path_prefix);

    let cached = CachedClassStorage::new(CachedClassStorageConfig::default(), fs_storage.clone());

    let class_id = ClassHash(felt!("0x1111"));
    let class = RawClass::try_from(SierraContractClass::default()).unwrap();
    let executable_class =
        RawExecutableClass::try_from(ContractClass::test_casm_contract_class()).unwrap();
    let executable_class_hash_v2 = CompiledClassHash(felt!("0x2222"));

    fs_storage
        .set_class(class_id, class, executable_class_hash_v2, executable_class.clone())
        .unwrap();

    assert_eq!(cached.get_executable(class_id).unwrap(), Some(executable_class));
    assert_eq!(
        cached.get_executable_class_hash_v2(class_id).unwrap(),
        Some(executable_class_hash_v2)
    );
    // No deprecated class for Cairo 1.
    assert_eq!(cached.get_deprecated_class(class_id).unwrap(), None);
}

#[test]
fn cached_storage_cairo0_get_executable_and_no_hash() {
    let persistent_root = tempfile::tempdir().unwrap();
    let class_hash_storage_path_prefix = tempfile::tempdir().unwrap();
    let mut fs_storage =
        FsClassStorage::new_for_testing(&persistent_root, &class_hash_storage_path_prefix);

    let cached = CachedClassStorage::new(CachedClassStorageConfig::default(), fs_storage.clone());

    let class_id = ClassHash(felt!("0x1111"));
    let deprecated_executable_class =
        RawExecutableClass::try_from(ContractClass::V0(DeprecatedContractClass::default()))
            .unwrap();

    fs_storage.set_deprecated_class(class_id, deprecated_executable_class.clone()).unwrap();

    assert_eq!(cached.get_executable(class_id).unwrap(), Some(deprecated_executable_class));
    assert_eq!(cached.get_executable_class_hash_v2(class_id).unwrap(), None);
}
