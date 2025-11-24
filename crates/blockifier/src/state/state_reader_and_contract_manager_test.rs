use std::sync::LazyLock;

use assert_matches::assert_matches;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use rstest::rstest;
use starknet_api::class_hash;
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::core::ClassHash;

use crate::blockifier::config::ContractClassManagerConfig;
use crate::execution::contract_class::RunnableCompiledClass;
use crate::state::contract_class_manager::ContractClassManager;
use crate::state::errors::StateError;
#[cfg(feature = "cairo_native")]
use crate::state::global_cache::{CachedCairoNative, CompiledClasses};
use crate::state::state_api::{StateReader, StateResult};
use crate::state::state_reader_and_contract_manager::StateReaderAndContractManager;
use crate::test_utils::contracts::{FeatureContractData, FeatureContractTrait};
use crate::test_utils::dict_state_reader::DictStateReader;
use crate::test_utils::initial_test_state::state_reader_and_contract_manager_for_testing;

fn build_reader_and_declare_contract(
    contract: FeatureContractData,
    contract_manager_config: ContractClassManagerConfig,
) -> StateReaderAndContractManager<DictStateReader> {
    let mut reader = DictStateReader::default();

    // Declare the contract in the storage.
    reader.add_class(&contract, &HashVersion::V2);

    state_reader_and_contract_manager_for_testing(
        reader,
        ContractClassManager::start(contract_manager_config),
    )
}

#[rstest]
#[case::no_cairo_native(false, false)]
#[cfg_attr(feature = "cairo_native", case::cairo_native_no_wait(true, false))]
#[cfg_attr(feature = "cairo_native", case::cairo_native_and_wait(true, true))]
fn test_get_compiled_class_without_native_in_cache(
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1(RunnableCairo1::Casm))]
    cairo_version: CairoVersion,
    #[case] run_cairo_native: bool,
    #[case] wait_on_native_compilation: bool,
) {
    // Sanity check: If native compilation is disabled, waiting on it is not allowed.
    if !run_cairo_native {
        assert!(!wait_on_native_compilation);
    }
    // Sanity check: If the cairo_native feature is off, running native compilation is not allowed.
    #[cfg(not(feature = "cairo_native"))]
    assert!(!run_cairo_native);

    let test_contract = FeatureContract::TestContract(cairo_version);
    let test_class_hash = test_contract.get_class_hash();
    let contract_manager_config = ContractClassManagerConfig::create_for_testing(
        run_cairo_native,
        wait_on_native_compilation,
    );

    let state_reader =
        build_reader_and_declare_contract(test_contract.into(), contract_manager_config);

    // Sanity check - the class manager's cache is empty.
    assert!(state_reader.contract_class_manager.get_runnable(&test_class_hash).is_none());

    let compiled_class = state_reader.get_compiled_class(test_class_hash).unwrap();

    match cairo_version {
        CairoVersion::Cairo1(_) => {
            // TODO(Meshi): Test that a compilation request was sent.
            if wait_on_native_compilation {
                #[cfg(feature = "cairo_native")]
                assert_matches!(
                    compiled_class,
                    RunnableCompiledClass::V1Native(_),
                    "We should have waited to the native class."
                );
            } else {
                assert_matches!(
                    compiled_class,
                    RunnableCompiledClass::V1(_),
                    "We do not wait for native, return the cairo1 casm."
                );
            }
        }
        CairoVersion::Cairo0 => {
            assert_eq!(
                compiled_class,
                test_contract.get_runnable_class(),
                "`get_compiled_class` should return the casm."
            );
        }
    }
}

#[cfg(feature = "cairo_native")]
#[rstest]
fn test_get_compiled_class_when_native_is_cached() {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native));
    let test_class_hash = test_contract.get_class_hash();
    let contract_manager_config = ContractClassManagerConfig::create_for_testing(true, true);

    let state_reader =
        build_reader_and_declare_contract(test_contract.into(), contract_manager_config);

    if let RunnableCompiledClass::V1Native(native_compiled_class) =
        test_contract.get_runnable_class()
    {
        state_reader.contract_class_manager.set_and_compile(
            test_class_hash,
            CompiledClasses::V1Native(CachedCairoNative::Compiled(native_compiled_class)),
        );
    } else {
        panic!("Expected NativeCompiledClassV1");
    }

    let compiled_class = state_reader.get_compiled_class(test_class_hash).unwrap();
    assert_matches!(compiled_class, RunnableCompiledClass::V1Native(_));
}

enum GetCompiledClassTestScenario {
    ClassIsDeclared(CairoVersion),
    ClassNotDeclared,
}

impl GetCompiledClassTestScenario {
    fn add_class_to_state_reader_and_get_request_and_expected_result(
        &self,
        reader: &mut DictStateReader,
    ) -> (ClassHash, StateResult<RunnableCompiledClass>) {
        match self {
            Self::ClassIsDeclared(cairo_version) => {
                let test_contract = FeatureContract::TestContract(*cairo_version);
                let test_class_hash = test_contract.get_class_hash();
                let expected_class = test_contract.get_runnable_class();
                reader.add_class(&test_contract.into(), &HashVersion::V2);
                (test_class_hash, Ok(expected_class))
            }
            Self::ClassNotDeclared => {
                (*DUMMY_CLASS_HASH, Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)))
            }
        }
    }
}

fn cairo_1_declared_scenario() -> GetCompiledClassTestScenario {
    GetCompiledClassTestScenario::ClassIsDeclared(CairoVersion::Cairo1(RunnableCairo1::Casm))
}

fn cairo_0_declared_scenario() -> GetCompiledClassTestScenario {
    GetCompiledClassTestScenario::ClassIsDeclared(CairoVersion::Cairo0)
}

fn not_declared_scenario() -> GetCompiledClassTestScenario {
    GetCompiledClassTestScenario::ClassNotDeclared
}

fn assert_eq_state_result(
    a: &StateResult<RunnableCompiledClass>,
    b: &StateResult<RunnableCompiledClass>,
) {
    match (a, b) {
        (Ok(a), Ok(b)) => {
            assert_eq!(a, b);
        }
        (Err(StateError::UndeclaredClassHash(a)), Err(StateError::UndeclaredClassHash(b))) => {
            assert_eq!(a, b)
        }
        _ => panic!("StateResult mismatch (or unsupported comparison): {a:?} vs {b:?}"),
    }
}

static DUMMY_CLASS_HASH: LazyLock<ClassHash> = LazyLock::new(|| class_hash!(2_u32));

#[rstest]
#[case::cairo_0_declared_and_cached(cairo_0_declared_scenario(), cairo_0_declared_scenario())]
#[case::cairo_1_declared_and_cached(cairo_1_declared_scenario(), cairo_1_declared_scenario())]
#[case::not_declared_then_declared(not_declared_scenario(), cairo_1_declared_scenario())]
#[case::not_declared_both_rounds(not_declared_scenario(), not_declared_scenario())]
fn test_get_compiled_class_caching_scenarios(
    #[case] first_scenario: GetCompiledClassTestScenario,
    #[case] second_scenario: GetCompiledClassTestScenario,
) {
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());

    // First execution.
    let mut first_reader = DictStateReader::default();
    let (first_class_hash, expected_first_result) = first_scenario
        .add_class_to_state_reader_and_get_request_and_expected_result(&mut first_reader);
    let first_state_reader_and_manager =
        state_reader_and_contract_manager_for_testing(first_reader, contract_class_manager.clone());

    let first_result = first_state_reader_and_manager.get_compiled_class(first_class_hash);

    // Second execution.
    let mut second_reader = DictStateReader::default();
    let (test_class_hash, expected_second_result) = second_scenario
        .add_class_to_state_reader_and_get_request_and_expected_result(&mut second_reader);
    let second_state_reader_and_manager =
        state_reader_and_contract_manager_for_testing(second_reader, contract_class_manager);

    let second_result = second_state_reader_and_manager.get_compiled_class(test_class_hash);

    // Verify results.
    assert_eq_state_result(&first_result, &expected_first_result);
    assert_eq_state_result(&second_result, &expected_second_result);
}
