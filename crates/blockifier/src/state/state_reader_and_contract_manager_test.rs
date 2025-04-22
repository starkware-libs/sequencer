use assert_matches::assert_matches;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use rstest::rstest;

use crate::blockifier::config::ContractClassManagerConfig;
use crate::context::BlockContext;
use crate::execution::contract_class::RunnableCompiledClass;
use crate::state::contract_class_manager::ContractClassManager;
#[cfg(feature = "cairo_native")]
use crate::state::global_cache::{CachedCairoNative, CompiledClasses};
use crate::state::state_api::StateReader;
use crate::state::state_reader_and_contract_manager::StateReaderAndContractManager;
use crate::test_utils::contracts::FeatureContractTrait;
use crate::test_utils::dict_state_reader::DictStateReader;
use crate::test_utils::initial_test_state::setup_test_state;
use crate::test_utils::BALANCE;
use crate::transaction::test_utils::block_context;

fn build_reader_and_declare_contract(
    contract: FeatureContract,
    contract_manager_config: ContractClassManagerConfig,
    block_context: BlockContext,
) -> StateReaderAndContractManager<DictStateReader> {
    let mut reader = DictStateReader::default();

    // Hack to declare the contract in the storage.
    let erc20_version = CairoVersion::Cairo0;
    setup_test_state(
        &block_context.chain_info,
        BALANCE,
        &[(contract.into(), 1)],
        erc20_version,
        &mut reader,
    );

    StateReaderAndContractManager {
        state_reader: reader,
        contract_class_manager: ContractClassManager::start(contract_manager_config),
    }
}

#[rstest]
#[case::dont_run_cairo_native(false, false)]
#[cfg_attr(feature = "cairo_native", case::run_cairo_native_without_waiting(true, false))]
#[cfg_attr(feature = "cairo_native", case::run_cairo_native_and_wait(true, true))]
fn test_get_compiled_class_without_native_in_cache(
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1(RunnableCairo1::Casm))]
    cairo_version: CairoVersion,
    #[case] run_cairo_native: bool,
    #[case] wait_on_native_compilation: bool,
    block_context: BlockContext,
) {
    // Sanity checks.
    if !run_cairo_native {
        assert!(!wait_on_native_compilation);
    }
    #[cfg(not(feature = "cairo_native"))]
    assert!(!run_cairo_native);

    let test_contract = FeatureContract::TestContract(cairo_version);
    let test_class_hash = test_contract.get_class_hash();
    let contract_manager_config = ContractClassManagerConfig::create_for_testing(
        run_cairo_native,
        wait_on_native_compilation,
    );

    let state_reader =
        build_reader_and_declare_contract(test_contract, contract_manager_config, block_context);

    // Sanity check - the cache is empty.
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
fn test_get_compiled_class_when_native_is_cached(block_context: BlockContext) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native));
    let test_class_hash = test_contract.get_class_hash();
    let contract_manager_config = ContractClassManagerConfig::create_for_testing(true, true);

    let state_reader =
        build_reader_and_declare_contract(test_contract, contract_manager_config, block_context);

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
