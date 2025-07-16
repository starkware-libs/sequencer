use std::sync::Arc;

use apollo_class_manager_types::{ClassHashes, ClassManagerError};
use apollo_compile_to_casm_types::{MockSierraCompilerClient, RawClass, RawExecutableClass};
use assert_matches::assert_matches;
use mockall::predicate::eq;
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::felt;
use starknet_api::state::SierraContractClass;

use crate::class_manager::ClassManager;
use crate::class_storage::{create_tmp_dir, CachedClassStorageConfig, FsClassStorage};
use crate::config::ClassManagerConfig;

impl ClassManager<FsClassStorage> {
    fn new_for_testing(compiler: MockSierraCompilerClient, config: ClassManagerConfig) -> Self {
        let storage =
            FsClassStorage::new_for_testing(&create_tmp_dir().unwrap(), &create_tmp_dir().unwrap());

        ClassManager::new(config, Arc::new(compiler), storage)
    }
}

fn mock_compile_expectations(
    compiler: &mut MockSierraCompilerClient,
    class: RawClass,
) -> (RawExecutableClass, CompiledClassHash) {
    let compile_output = (
        RawExecutableClass::try_from(ContractClass::test_casm_contract_class()).unwrap(),
        CompiledClassHash(felt!("0x5678")),
    );
    let cloned_compiled_output = compile_output.clone();

    compiler
        .expect_compile()
        .with(eq(class.clone()))
        .times(1)
        .return_once(move |_| Ok(compile_output));

    cloned_compiled_output
}

// TODO(Elin): consider sharing setup code, keeping it clear for the test reader how the compiler is
// mocked per test.

#[tokio::test]
async fn class_manager() {
    // Setup.

    // Prepare mock compiler.
    let mut compiler = MockSierraCompilerClient::new();
    let class = RawClass::try_from(SierraContractClass::default()).unwrap();
    let (expected_executable_class, expected_executable_class_hash_v2) =
        mock_compile_expectations(&mut compiler, class.clone());

    // Prepare class manager.
    let cached_class_storage_config =
        CachedClassStorageConfig { class_cache_size: 10, deprecated_class_cache_size: 10 };
    let mut class_manager = ClassManager::new_for_testing(
        compiler,
        ClassManagerConfig { cached_class_storage_config, ..Default::default() },
    );

    // Test.

    // Non-existent class.
    let class_id = SierraContractClass::try_from(class.clone()).unwrap().calculate_class_hash();
    assert_eq!(class_manager.get_sierra(class_id), Ok(None));
    assert_eq!(class_manager.get_executable(class_id), Ok(None));

    // Add new class.
    let class_hashes = class_manager.add_class(class.clone()).await.unwrap();
    let expected_class_hashes = ClassHashes {
        class_hash: class_id,
        executable_class_hash_v2: expected_executable_class_hash_v2,
    };
    assert_eq!(class_hashes, expected_class_hashes);

    // Get class.
    assert_eq!(class_manager.get_sierra(class_id).unwrap(), Some(class.clone()));
    assert_eq!(class_manager.get_executable(class_id).unwrap(), Some(expected_executable_class));

    // Add existing class; response returned immediately, without invoking compilation.
    let class_hashes = class_manager.add_class(class).await.unwrap();
    assert_eq!(class_hashes, expected_class_hashes);
}

#[tokio::test]
#[ignore = "Test deprecated class API"]
async fn class_manager_deprecated_class_api() {
    todo!("Test deprecated class API");
}

#[tokio::test]
async fn class_manager_get_executable() {
    // Setup.

    // Prepare mock compiler.
    let mut compiler = MockSierraCompilerClient::new();
    let class = RawClass::try_from(SierraContractClass::default()).unwrap();
    let (expected_executable_class, _) = mock_compile_expectations(&mut compiler, class.clone());

    // Prepare class manager.
    let cached_class_storage_config =
        CachedClassStorageConfig { class_cache_size: 10, deprecated_class_cache_size: 10 };
    let mut class_manager = ClassManager::new_for_testing(
        compiler,
        ClassManagerConfig { cached_class_storage_config, ..Default::default() },
    );

    // Test.

    // Add classes: deprecated and non-deprecated, under different hashes.
    let ClassHashes { class_hash, executable_class_hash_v2 } =
        class_manager.add_class(class.clone()).await.unwrap();

    let deprecated_class_hash = ClassHash(felt!("0x1806"));
    let deprecated_executable_class = RawExecutableClass::new_unchecked(vec![1, 2, 3].into());
    class_manager
        .add_deprecated_class(deprecated_class_hash, deprecated_executable_class.clone())
        .unwrap();

    // Get both executable classes.
    assert_eq!(class_manager.get_executable(class_hash).unwrap(), Some(expected_executable_class));
    assert_eq!(
        class_manager.get_executable(deprecated_class_hash).unwrap(),
        Some(deprecated_executable_class)
    );
    // TODO(Meshi): Consider computing the blake class hash here.
    assert_eq!(
        class_manager.get_executable_class_hash_v2(class_hash).unwrap(),
        Some(executable_class_hash_v2)
    );
}

#[tokio::test]
async fn class_manager_class_length_validation() {
    // Setup.

    // Prepare mock compiler.
    let mut compiler = MockSierraCompilerClient::new();
    let class = RawClass::try_from(SierraContractClass::default()).unwrap();
    let (expected_executable_class, _) = mock_compile_expectations(&mut compiler, class.clone());

    // Prepare class manager.
    let mut class_manager = ClassManager::new_for_testing(
        compiler,
        ClassManagerConfig {
            max_compiled_contract_class_object_size: expected_executable_class.size().unwrap() - 1,
            ..Default::default()
        },
    );

    // Test.
    assert_matches!(
        class_manager.add_class(class).await,
        Err(ClassManagerError::ContractClassObjectSizeTooLarge { .. })
    );
}
