use assert_matches::assert_matches;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use rstest::rstest;
use starknet_api::contract_class::compiled_class_hash::HashVersion;

use crate::blockifier::config::ContractClassManagerConfig;
use crate::execution::contract_class::RunnableCompiledClass;
use crate::state::contract_class_manager::ContractClassManager;
use crate::state::state_api::StateReader;
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
mod native_tests {
    use assert_matches::assert_matches;
    use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
    use blockifier_test_utils::contracts::FeatureContract;
    use rstest::rstest;

    use super::build_reader_and_declare_contract;
    use crate::blockifier::config::ContractClassManagerConfig;
    use crate::execution::contract_class::RunnableCompiledClass;
    use crate::state::global_cache::{CachedCairoNative, CompiledClasses};
    use crate::state::state_api::StateReader;
    use crate::test_utils::contracts::FeatureContractTrait;

    #[rstest]
    fn test_get_compiled_class_when_native_is_cached() {
        let test_contract =
            FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native));
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
}

#[cfg(not(feature = "cairo_native"))]
mod non_native_tests {
    use std::sync::LazyLock;

    use rstest::rstest;
    use starknet_api::class_hash;
    use starknet_api::core::ClassHash;

    use crate::blockifier::config::CairoNativeRunConfig;
    use crate::execution::contract_class::RunnableCompiledClass;
    use crate::state::errors::StateError;
    use crate::state::global_cache::CompiledClasses;
    use crate::state::state_api::{StateReader, StateResult};
    use crate::state::state_api_test_utils::assert_eq_state_result;
    use crate::state::state_reader_and_contract_manager::ContractClassManager;
    use crate::state::state_reader_and_contract_manager_test_utils::MockFetchCompiledClasses;
    use crate::test_utils::initial_test_state::state_reader_and_contract_manager_for_testing;

    static DUMMY_CLASS_HASH: LazyLock<ClassHash> = LazyLock::new(|| class_hash!(2_u32));

    struct GetCompiledClassTestScenario {
        expectations: GetCompiledClassTestExpectation,

        // Test result.
        expected_result: StateResult<RunnableCompiledClass>,
    }

    struct GetCompiledClassTestExpectation {
        get_compiled_classes_result: Option<StateResult<CompiledClasses>>,
        is_declared_result: Option<StateResult<bool>>,
    }

    fn add_expectation_to_mock_fetch_compiled_classes(
        mock_fetch_compiled_classes: &mut MockFetchCompiledClasses,
        expectations: GetCompiledClassTestExpectation,
    ) {
        if let Some(get_compiled_classes_result) = expectations.get_compiled_classes_result {
            mock_fetch_compiled_classes
                .expect_get_compiled_classes()
                .times(1)
                .return_once(move |_| get_compiled_classes_result);
        }

        if let Some(is_declared_result) = expectations.is_declared_result {
            mock_fetch_compiled_classes
                .expect_is_declared()
                .times(1)
                .return_once(|_| is_declared_result);
        }
    }

    fn cairo_1_declared_scenario() -> GetCompiledClassTestScenario {
        GetCompiledClassTestScenario {
            expectations: GetCompiledClassTestExpectation {
                get_compiled_classes_result: Some(Ok(CompiledClasses::from_runnable_for_testing(
                    RunnableCompiledClass::test_casm_contract_class(),
                ))),
                is_declared_result: None,
            },
            expected_result: Ok(RunnableCompiledClass::test_casm_contract_class()),
        }
    }

    fn cairo_1_cached_scenario() -> GetCompiledClassTestScenario {
        GetCompiledClassTestScenario {
            expectations: GetCompiledClassTestExpectation {
                get_compiled_classes_result: None,
                is_declared_result: Some(Ok(true)), // Verification call for cached Cairo1 class.
            },
            expected_result: Ok(RunnableCompiledClass::test_casm_contract_class()),
        }
    }

    fn cached_but_verification_failed_after_reorg_scenario() -> GetCompiledClassTestScenario {
        GetCompiledClassTestScenario {
            expectations: GetCompiledClassTestExpectation {
                get_compiled_classes_result: None,
                is_declared_result: Some(Ok(false)), // Verification fails after reorg.
            },
            expected_result: Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)),
        }
    }

    fn cairo_0_declared_scenario() -> GetCompiledClassTestScenario {
        GetCompiledClassTestScenario {
            expectations: GetCompiledClassTestExpectation {
                get_compiled_classes_result: Some(Ok(CompiledClasses::from_runnable_for_testing(
                    RunnableCompiledClass::test_deprecated_casm_contract_class(),
                ))),
                is_declared_result: None,
            },
            expected_result: Ok(RunnableCompiledClass::test_deprecated_casm_contract_class()),
        }
    }

    fn cairo_0_cached_scenario() -> GetCompiledClassTestScenario {
        GetCompiledClassTestScenario {
            expectations: GetCompiledClassTestExpectation {
                get_compiled_classes_result: None,
                is_declared_result: None,
            },
            expected_result: Ok(RunnableCompiledClass::test_deprecated_casm_contract_class()),
        }
    }

    fn not_declared_scenario() -> GetCompiledClassTestScenario {
        GetCompiledClassTestScenario {
            expectations: GetCompiledClassTestExpectation {
                get_compiled_classes_result: Some(Err(StateError::UndeclaredClassHash(
                    *DUMMY_CLASS_HASH,
                ))),
                is_declared_result: None,
            },
            expected_result: Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)),
        }
    }

    #[rstest]
    #[case::cairo_0_declared_and_cached(cairo_0_declared_scenario(), cairo_0_cached_scenario())]
    #[case::cairo_1_declared_and_cached(cairo_1_declared_scenario(), cairo_1_cached_scenario())]
    #[case::cairo_1_declared_then_verification_failed_after_reorg(
        cairo_1_declared_scenario(),
        cached_but_verification_failed_after_reorg_scenario()
    )]
    #[case::not_declared_then_declared(not_declared_scenario(), cairo_1_declared_scenario())]
    #[case::not_declared_both_rounds(not_declared_scenario(), not_declared_scenario())]
    fn test_get_compiled_class_caching_scenarios(
        #[case] first_scenario: GetCompiledClassTestScenario,
        #[case] second_scenario: GetCompiledClassTestScenario,
    ) {
        use crate::blockifier::config::ContractClassManagerConfig;

        let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig {
            cairo_native_run_config: CairoNativeRunConfig {
                wait_on_native_compilation: false,
                ..Default::default()
            },
            ..Default::default()
        });
        let class_hash = *DUMMY_CLASS_HASH;

        // First execution.
        let mut first_reader = MockFetchCompiledClasses::new();
        add_expectation_to_mock_fetch_compiled_classes(
            &mut first_reader,
            first_scenario.expectations,
        );
        let first_state_reader_and_manager = state_reader_and_contract_manager_for_testing(
            first_reader,
            contract_class_manager.clone(),
        );

        let first_result = first_state_reader_and_manager.get_compiled_class(class_hash);

        // Second execution.
        let mut second_reader = MockFetchCompiledClasses::new();
        add_expectation_to_mock_fetch_compiled_classes(
            &mut second_reader,
            second_scenario.expectations,
        );
        let second_state_reader_and_manager =
            state_reader_and_contract_manager_for_testing(second_reader, contract_class_manager);

        let second_result = second_state_reader_and_manager.get_compiled_class(class_hash);

        // Verify results.
        assert_eq_state_result(&first_result, &first_scenario.expected_result);
        assert_eq_state_result(&second_result, &second_scenario.expected_result);
    }
}
