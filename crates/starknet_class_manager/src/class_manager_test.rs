use std::sync::Arc;

use mockall::predicate::eq;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::felt;
use starknet_api::state::SierraContractClass;
use starknet_class_manager_types::{CachedClassStorageError, ClassManagerError};
use starknet_sierra_multicompile_types::{MockSierraCompilerClient, RawClass, RawExecutableClass};

use crate::class_manager::ClassManager;
use crate::class_storage::FsClassStorageError;

#[tokio::test]
async fn class_manager() {
    // Setup.

    // Prepare mock compiler.
    let mut compiler = MockSierraCompilerClient::new();
    let class = RawClass::try_from(SierraContractClass::default()).unwrap();
    let expected_executable_class = Arc::new(RawExecutableClass(vec![4, 5, 6].into()));
    let expected_executable_class_for_closure = expected_executable_class.clone();
    let expected_executable_class_hash = CompiledClassHash(felt!("0x5678"));
    compiler.expect_compile().with(eq(class.clone())).times(1).returning(move |_| {
        Ok(((*expected_executable_class_for_closure).clone(), expected_executable_class_hash))
    });

    // Prepare class manager.
    let mut class_manager = ClassManager::new_for_testing(compiler);

    // Test.

    // Non-existent class.
    let class_id = ClassHash(felt!("0x1234"));
    let class_not_found_error: CachedClassStorageError<FsClassStorageError> =
        CachedClassStorageError::ClassNotFound { class_id };
    let class_not_found_error = ClassManagerError::from(class_not_found_error);
    assert_eq!(class_manager.get_sierra(class_id).unwrap_err(), class_not_found_error);
    assert_eq!(class_manager.get_executable(class_id).unwrap_err(), class_not_found_error);

    // Add new class.
    let executable_class_hash = class_manager.add_class(class_id, class.clone()).await.unwrap();
    assert_eq!(executable_class_hash, expected_executable_class_hash);

    // Get class.
    assert_eq!(class_manager.get_sierra(class_id).unwrap(), class);
    assert_eq!(
        class_manager.get_executable(class_id).unwrap(),
        (*expected_executable_class).clone()
    );

    // Add existing class; response returned immediately, without invoking compilation.
    let executable_class_hash = class_manager.add_class(class_id, class.clone()).await.unwrap();
    assert_eq!(executable_class_hash, expected_executable_class_hash);
}
