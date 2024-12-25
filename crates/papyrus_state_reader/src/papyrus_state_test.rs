use core::panic;

use assert_matches::assert_matches;
use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::execution::call_info::CallExecution;
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::execution::entry_point::CallEntryPoint;
use blockifier::retdata;
use blockifier::state::cached_state::CachedState;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::state_api::StateReader;
use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::{trivial_external_entry_point_new, CairoVersion, RunnableCairo1};
use indexmap::IndexMap;
use papyrus_storage::class::ClassStorageWriter;
use papyrus_storage::compiled_class::CasmStorageWriter;
use papyrus_storage::state::StateStorageWriter;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::ContractClass;
use starknet_api::core::Nonce;
use starknet_api::state::{StateDiff, StorageKey, ThinStateDiff};
use starknet_api::{calldata, felt};

use crate::papyrus_state::PapyrusReader;

#[test]
fn test_entry_point_with_papyrus_state() -> papyrus_storage::StorageResult<()> {
    let ((storage_reader, mut storage_writer), _) = papyrus_storage::test_utils::get_test_storage();

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
    let papyrus_reader = PapyrusReader::new(
        storage_reader,
        block_number,
        ContractClassManager::start(ContractClassManagerConfig::default()),
    );
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

#[cfg(feature = "cairo_native")]
#[test]
fn test_get_compiled_class() -> papyrus_storage::StorageResult<()> {
    use std::sync::Arc;

    use blockifier::state::global_cache::CachedCasm;

    let ((storage_reader, mut storage_writer), _) = papyrus_storage::test_utils::get_test_storage();
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let compiled_class = if let ContractClass::V1((casm_class, _)) = test_contract.get_class() {
        casm_class
    } else {
        panic!("expected a V1 class")
    };
    let test_class_hash = test_contract.get_class_hash();
    let test_compiled_class_hash = test_contract.get_compiled_class_hash();
    let block_number = BlockNumber::default();

    let thin_state_diff = ThinStateDiff {
        declared_classes: IndexMap::from([(test_class_hash, test_compiled_class_hash)]),
        nonces: IndexMap::from([(test_contract.get_instance_address(1), Nonce(1.into()))]),
        ..Default::default()
    };
    let _ = storage_writer
        .begin_rw_txn()?
        .append_state_diff(block_number, thin_state_diff)?
        .append_classes(block_number, &[(test_class_hash, &test_contract.get_sierra())], &[])?
        .append_casm(&test_class_hash, &compiled_class)?
        .commit();
    let contract_manager_config = ContractClassManagerConfig::create_for_testing(true, true);


    let papyrus_reader = PapyrusReader::new(
        storage_reader,
        block_number,
        ContractClassManager::start(contract_manager_config),
    );
    papyrus_reader.contract_class_manager.set_casm(test_class_hash, CachedCasm::WithSierra(test_contract.get_runnable_class(), Arc::new(test_contract.get_sierra())));
    let compiled_class = papyrus_reader.get_compiled_class(test_class_hash);
    assert!(compiled_class.is_ok(), "compilation should have succeeded in this case");
    assert!(false);
    assert_matches!(
        compiled_class,
        Ok(RunnableCompiledClass::V1Native(_)),
        "compilation should have succeeded in this case"
    );

    Ok(())
}
