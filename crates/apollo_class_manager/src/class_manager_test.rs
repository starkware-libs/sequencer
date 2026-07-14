use std::sync::Arc;

use apollo_class_manager_config::config::{
    CachedClassStorageConfig,
    ClassManagerConfig,
    ClassManagerDynamicConfig,
    ClassManagerStaticConfig,
    FsClassManagerConfig,
    FsClassStorageConfig,
};
use apollo_class_manager_types::{ClassHashes, ClassManagerError};
use apollo_compile_to_casm_types::{MockSierraCompilerClient, RawClass, RawExecutableClass};
use apollo_config_manager_types::communication::MockConfigManagerClient;
use assert_matches::assert_matches;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use cairo_lang_starknet_classes::casm_contract_class::{
    CasmContractClass,
    CasmContractEntryPoint,
    CasmContractEntryPoints,
};
use mockall::predicate::eq;
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedContractClass;
use starknet_api::felt;
use starknet_api::state::SierraContractClass;

use crate::class_manager::{all_entry_points, validate_casm_builtins, ClassManager};
use crate::class_storage::FsClassStorage;

impl ClassManager<FsClassStorage> {
    fn new_for_testing(compiler: MockSierraCompilerClient, config: ClassManagerConfig) -> Self {
        let persistent_root = tempfile::tempdir().unwrap();
        let class_hash_storage_path_prefix = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(persistent_root.path()).unwrap();
        std::fs::create_dir_all(class_hash_storage_path_prefix.path()).unwrap();
        let storage =
            FsClassStorage::new_for_testing(&persistent_root, &class_hash_storage_path_prefix);

        let fs_class_manager_config = FsClassManagerConfig {
            static_config: ClassManagerStaticConfig {
                class_manager_config: config,
                class_storage_config: FsClassStorageConfig::default(),
            },
            dynamic_config: ClassManagerDynamicConfig::default(),
        };

        let mock_config_manager_client = Arc::new(MockConfigManagerClient::new());

        ClassManager::new(
            fs_class_manager_config,
            Arc::new(compiler),
            storage,
            mock_config_manager_client,
        )
    }
}

fn mock_compile_expectations(
    compiler: &mut MockSierraCompilerClient,
    class: RawClass,
) -> (RawExecutableClass, CompiledClassHash) {
    let compile_output =
        (RawExecutableClass::test_casm_contract_class(), CompiledClassHash(felt!("0x5678")));
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
    let deprecated_executable_class =
        RawExecutableClass::try_from(ContractClass::V0(DeprecatedContractClass::default()))
            .unwrap();
    class_manager
        .add_deprecated_class(deprecated_class_hash, deprecated_executable_class.clone())
        .unwrap();

    // Get both executable classes.
    assert_eq!(class_manager.get_executable(class_hash).unwrap(), Some(expected_executable_class));
    assert_eq!(
        class_manager.get_executable(deprecated_class_hash).unwrap(),
        Some(deprecated_executable_class)
    );
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

#[tokio::test]
async fn class_manager_builtins_validation() {
    // Setup: the compiler returns a CASM whose entry point declares builtins that are not an
    // ordered subsequence of the supported builtins, so `add_class` must reject it.
    let mut compiler = MockSierraCompilerClient::new();
    let class = RawClass::try_from(SierraContractClass::default()).unwrap();
    let bad_casm = contract_class_with_builtins(vec![vec!["range_check", "pedersen"]], vec![]);
    let raw_executable_class = RawExecutableClass::try_from(bad_casm).unwrap();
    compiler
        .expect_compile()
        .with(eq(class.clone()))
        .times(1)
        .return_once(move |_| Ok((raw_executable_class, CompiledClassHash(felt!("0x5678")))));

    let mut class_manager =
        ClassManager::new_for_testing(compiler, ClassManagerConfig::default());

    // Test.
    assert_matches!(
        class_manager.add_class(class).await,
        Err(ClassManagerError::InvalidBuiltins { .. })
    );
}

fn casm_entry_points(builtins_per_entry_point: Vec<Vec<&str>>) -> Vec<CasmContractEntryPoint> {
    builtins_per_entry_point
        .into_iter()
        .map(|builtins| CasmContractEntryPoint {
            builtins: builtins.into_iter().map(String::from).collect(),
            ..Default::default()
        })
        .collect()
}

fn contract_class_with_builtins(
    external: Vec<Vec<&str>>,
    l1_handler: Vec<Vec<&str>>,
) -> ContractClass {
    let casm = CasmContractClass {
        prime: Default::default(),
        compiler_version: String::new(),
        bytecode: vec![],
        bytecode_segment_lengths: None,
        hints: vec![],
        pythonic_hints: None,
        entry_points_by_type: CasmContractEntryPoints {
            constructor: vec![],
            external: casm_entry_points(external),
            l1_handler: casm_entry_points(l1_handler),
        },
    };
    ContractClass::V1((casm, SierraVersion::new(0, 0, 0)))
}

// Loads the compiled (CASM) class of the Cairo 1 test contract (compiled on demand).
fn test_contract_class() -> ContractClass {
    let raw_casm =
        FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm)).get_raw_class();
    let casm: CasmContractClass =
        serde_json::from_str(&raw_casm).expect("Failed to deserialize the test contract CASM.");
    ContractClass::V1((casm, SierraVersion::new(0, 0, 0)))
}

fn declares_builtins(contract_class: &ContractClass) -> bool {
    let ContractClass::V1((casm, _)) = contract_class else {
        return false;
    };
    all_entry_points(&casm.entry_points_by_type)
        .any(|entry_point| !entry_point.builtins.is_empty())
}

#[test]
fn validate_casm_builtins_accepts_supported_ordered_builtins() {
    let contract_class = contract_class_with_builtins(
        vec![vec!["pedersen", "range_check", "poseidon"], vec![]],
        vec![vec!["range_check", "add_mod", "mul_mod"]],
    );
    assert_matches!(validate_casm_builtins(ClassHash::default(), &contract_class), Ok(()));
}

#[test]
fn validate_casm_builtins_rejects_bad_builtins() {
    // Each case is a builtin list that is not an ordered subsequence of the supported builtins:
    // wrong order, a valid builtin that is unsupported here, a duplicate, an unsupported (Cairo 0)
    // builtin, and an unparsable builtin name (exercises the failed-parse arm).
    for bad_builtins in [
        vec!["range_check", "pedersen"],
        vec!["range_check", "keccak"],
        vec!["range_check", "range_check"],
        vec!["ecdsa"],
        vec!["foobar"],
    ] {
        let expected_builtins: Vec<String> =
            bad_builtins.iter().map(|builtin| builtin.to_string()).collect();
        let contract_class =
            contract_class_with_builtins(vec![vec!["pedersen"]], vec![bad_builtins]);

        assert_matches!(
            validate_casm_builtins(ClassHash::default(), &contract_class),
            Err(ClassManagerError::InvalidBuiltins { builtins, .. }) if builtins == expected_builtins
        );
    }
}

// A concrete compiled contract (the test contract) passes, and it actually exercises the check.
#[test]
fn validate_casm_builtins_accepts_real_contract() {
    let contract_class = test_contract_class();
    assert!(
        declares_builtins(&contract_class),
        "Expected the test contract to declare builtins in some entry point."
    );

    assert_matches!(validate_casm_builtins(ClassHash::default(), &contract_class), Ok(()));
}

// Negative flow: injecting noise (an unsupported builtin) into the real contract's builtin lists is
// rejected.
#[test]
fn validate_casm_builtins_rejects_real_contract_with_noise() {
    let mut contract_class = test_contract_class();
    let ContractClass::V1((casm, _)) = &mut contract_class else {
        panic!("Expected a Cairo 1 contract class.");
    };
    let entry_points = &mut casm.entry_points_by_type;
    for entry_point in entry_points
        .constructor
        .iter_mut()
        .chain(entry_points.external.iter_mut())
        .chain(entry_points.l1_handler.iter_mut())
    {
        entry_point.builtins.insert(0, "keccak".to_string());
    }

    assert_matches!(
        validate_casm_builtins(ClassHash::default(), &contract_class),
        Err(ClassManagerError::InvalidBuiltins { .. })
    );
}
