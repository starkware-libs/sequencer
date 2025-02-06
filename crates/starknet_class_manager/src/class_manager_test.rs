use mockall::predicate::eq;
use starknet_api::core::CompiledClassHash;
use starknet_api::felt;
use starknet_api::state::SierraContractClass;
use starknet_class_manager_types::{CachedClassStorageError, ClassHashes, ClassManagerError};
use starknet_sierra_multicompile_types::{MockSierraCompilerClient, RawClass, RawExecutableClass};

use crate::class_manager::ClassManager;
use crate::class_storage::{CachedClassStorageConfig, FsClassStorage, FsClassStorageError};
use crate::config::ClassManagerConfig;
use crate::test_utils::FileHandles;

impl ClassManager<FsClassStorage> {
    fn new_for_testing(compiler: MockSierraCompilerClient) -> (Self, FileHandles) {
        use std::sync::Arc;

        use crate::class_storage::FsClassStorage;

        let cached_class_storage_config =
            CachedClassStorageConfig { class_cache_size: 10, deprecated_class_cache_size: 10 };
        let config = ClassManagerConfig { cached_class_storage_config };
        let (storage, handles) = FsClassStorage::new_for_testing();

        (ClassManager::new(config, Arc::new(compiler), storage), handles)
    }
}

#[tokio::test]
async fn class_manager() {
    // Setup.

    // Prepare mock compiler.
    let mut compiler = MockSierraCompilerClient::new();
    let class = RawClass::try_from(SierraContractClass::default()).unwrap();
    let expected_executable_class = RawExecutableClass(vec![4, 5, 6].into());
    let expected_executable_class_for_closure = expected_executable_class.clone();
    let expected_executable_class_hash = CompiledClassHash(felt!("0x5678"));
    compiler.expect_compile().with(eq(class.clone())).times(1).return_once(move |_| {
        Ok((expected_executable_class_for_closure, expected_executable_class_hash))
    });

    // Prepare class manager.
    let (mut class_manager, _handles) = ClassManager::new_for_testing(compiler);

    // Test.

    // Non-existent class.
    let class_id = SierraContractClass::try_from(class.clone()).unwrap().calculate_class_hash();
    let class_not_found_error: CachedClassStorageError<FsClassStorageError> =
        CachedClassStorageError::ClassNotFound { class_id };
    let class_not_found_error = ClassManagerError::from(class_not_found_error);
    assert_eq!(class_manager.get_sierra(class_id).unwrap_err(), class_not_found_error);
    assert_eq!(class_manager.get_executable(class_id).unwrap_err(), class_not_found_error);

    // Add new class.
    let class_hashes = class_manager.add_class(class.clone()).await.unwrap();
    let expected_class_hashes =
        ClassHashes { class_hash: class_id, executable_class_hash: expected_executable_class_hash };
    assert_eq!(class_hashes, expected_class_hashes);

    // Get class.
    assert_eq!(class_manager.get_sierra(class_id).unwrap(), class);
    assert_eq!(class_manager.get_executable(class_id).unwrap(), expected_executable_class);

    // Add existing class; response returned immediately, without invoking compilation.
    let class_hashes = class_manager.add_class(class).await.unwrap();
    assert_eq!(class_hashes, expected_class_hashes);
}
