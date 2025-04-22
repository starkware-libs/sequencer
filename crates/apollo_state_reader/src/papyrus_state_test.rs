use core::panic;

use apollo_storage::class::ClassStorageWriter;
use apollo_storage::compiled_class::CasmStorageWriter;
use apollo_storage::state::StateStorageWriter;
use assert_matches::assert_matches;
use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::execution::call_info::CallExecution;
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::execution::entry_point::CallEntryPoint;
use blockifier::retdata;
use blockifier::state::cached_state::CachedState;
use blockifier::state::contract_class_manager::ContractClassManager;
#[cfg(feature = "cairo_native")]
use blockifier::state::global_cache::{CachedCairoNative, CachedClass};
use blockifier::state::state_api::StateReader;
use blockifier::state::state_reader_and_contract_manager::StateReaderAndContractManager;
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier::test_utils::trivial_external_entry_point_new;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::contracts::FeatureContract;
use indexmap::IndexMap;
use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::ContractClass;
use starknet_api::state::{StateDiff, StorageKey, ThinStateDiff};
use starknet_api::{calldata, felt};

use crate::papyrus_state::PapyrusReader;

#[test]
fn test_entry_point_with_papyrus_state() -> apollo_storage::StorageResult<()> {
    let ((storage_reader, mut storage_writer), _) = apollo_storage::test_utils::get_test_storage();

    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo0);
    let test_class_hash = test_contract.get_class_hash();
    let test_class = assert_matches!(
        test_contract.get_class(), ContractClass::V0(contract_class) => contract_class
    );

    // Initialize Storage: add test contract and class.
    let deployed_contracts =
        IndexMap::from([(test_contract.get_instance_address(0), test_class_hash)]);
    let state_diff = StateDiff {
        deployed_contracts,
        deprecated_declared_classes: IndexMap::from([(test_class_hash, test_class.clone())]),
        ..Default::default()
    };

    let block_number = BlockNumber::default();
    storage_writer
        .begin_rw_txn()?
        .append_state_diff(block_number, state_diff.into())?
        .append_classes(block_number, Default::default(), &[(test_class_hash, &test_class)])?
        .commit()?;

    // BlockNumber is 1 due to the initialization step above.
    let block_number = BlockNumber(1);
    let papyrus_reader = PapyrusReader::new(storage_reader, block_number);
    let mut state = CachedState::from(papyrus_reader);

    // Call entrypoint that want to write to storage, which updates the cached state's write cache.
    let key = felt!(1234_u16);
    let value = felt!(18_u8);
    let calldata = calldata![key, value];
    let entry_point_call = CallEntryPoint {
        calldata,
        entry_point_selector: selector_from_name("test_storage_read_write"),
        ..trivial_external_entry_point_new(test_contract)
    };
    let storage_address = entry_point_call.storage_address;
    assert_eq!(
        entry_point_call.execute_directly(&mut state).unwrap().execution,
        CallExecution::from_retdata(retdata![value])
    );

    // Verify that the state has changed.
    let storage_key = StorageKey::try_from(key).unwrap();
    let value_from_state = state.get_storage_at(storage_address, storage_key).unwrap();
    assert_eq!(value_from_state, value);

    Ok(())
}

fn build_apollo_state_reader_and_declare_contract(
    contract: FeatureContract,
    contract_manager_config: ContractClassManagerConfig,
) -> StateReaderAndContractManager<PapyrusReader> {
    let class_hash = contract.get_class_hash();
    let ((storage_reader, mut storage_writer), _) = apollo_storage::test_utils::get_test_storage();
    let test_compiled_class_hash = contract.get_compiled_class_hash();
    let block_number = BlockNumber::default();

    // Hack to declare the contract in the storage.
    match contract.get_class() {
        ContractClass::V1((casm_class, _)) => {
            let thin_state_diff = ThinStateDiff {
                declared_classes: IndexMap::from([(class_hash, test_compiled_class_hash)]),
                ..Default::default()
            };
            storage_writer
                .begin_rw_txn()
                .unwrap()
                .append_state_diff(block_number, thin_state_diff)
                .unwrap()
                .append_classes(block_number, &[(class_hash, &contract.get_sierra())], &[])
                .unwrap()
                .append_casm(&class_hash, &casm_class)
                .unwrap()
                .commit()
                .unwrap();
        }

        ContractClass::V0(deprecated_contract_class) => {
            let thin_state_diff = ThinStateDiff {
                deprecated_declared_classes: vec![class_hash],
                ..Default::default()
            };
            storage_writer
                .begin_rw_txn()
                .unwrap()
                .append_state_diff(block_number, thin_state_diff)
                .unwrap()
                .append_classes(block_number, &[], &[(class_hash, &deprecated_contract_class)])
                .unwrap()
                .commit()
                .unwrap();
        }
    }

    let papyrus_reader = PapyrusReader::new(storage_reader, BlockNumber(1));

    StateReaderAndContractManager {
        state_reader: papyrus_reader,
        contract_class_manager: ContractClassManager::start(contract_manager_config),
    }
}

// TODO(AvivG): Move native test logic to the blockifier
#[rstest]
#[case::dont_run_cairo_native(false, false)]
#[cfg_attr(feature = "cairo_native", case::run_cairo_native_without_waiting(true, false))]
#[cfg_attr(feature = "cairo_native", case::run_cairo_native_and_wait(true, true))]
fn test_get_compiled_class_without_native_in_cache(
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1(RunnableCairo1::Casm))]
    cairo_version: CairoVersion,
    #[case] run_cairo_native: bool,
    #[case] wait_on_native_compilation: bool,
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
        build_apollo_state_reader_and_declare_contract(test_contract, contract_manager_config);
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

// TODO(AvivG): Move native test logic to the blockifier
#[cfg(feature = "cairo_native")]
#[test]
fn test_get_compiled_class_when_native_is_cached() {
    let ((storage_reader, _), _) = apollo_storage::test_utils::get_test_storage();
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Native));
    let test_class_hash = test_contract.get_class_hash();
    let contract_manager_config = ContractClassManagerConfig::create_for_testing(true, true);
    let papyrus_reader = PapyrusReader::new(storage_reader, BlockNumber::default());
    let state_reader = StateReaderAndContractManager {
        state_reader: papyrus_reader,
        contract_class_manager: ContractClassManager::start(contract_manager_config),
    };
    if let RunnableCompiledClass::V1Native(native_compiled_class) =
        test_contract.get_runnable_class()
    {
        state_reader.contract_class_manager.set_and_compile(
            test_class_hash,
            CachedClass::V1Native(CachedCairoNative::Compiled(native_compiled_class)),
        );
    } else {
        panic!("Expected NativeCompiledClassV1");
    }
    let compiled_class = state_reader.get_compiled_class(test_class_hash).unwrap();
    assert_matches!(compiled_class, RunnableCompiledClass::V1Native(_));
}
