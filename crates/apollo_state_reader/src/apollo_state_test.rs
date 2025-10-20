use core::panic;

use apollo_storage::class::ClassStorageWriter;
use apollo_storage::state::StateStorageWriter;
use assert_matches::assert_matches;
use blockifier::execution::call_info::CallExecution;
use blockifier::execution::entry_point::CallEntryPoint;
use blockifier::retdata;
use blockifier::state::cached_state::CachedState;
use blockifier::state::state_api::StateReader;
use blockifier::test_utils::contracts::FeatureContractTrait;
use blockifier::test_utils::trivial_external_entry_point_new;
use blockifier_test_utils::cairo_versions::CairoVersion;
use blockifier_test_utils::contracts::FeatureContract;
use indexmap::IndexMap;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::BlockNumber;
use starknet_api::contract_class::ContractClass;
use starknet_api::state::{StateDiff, StorageKey};
use starknet_api::{calldata, felt};

use crate::apollo_state::ApolloReader;

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
    let apollo_reader = ApolloReader::new(storage_reader, block_number);
    let mut state = CachedState::from(apollo_reader);

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
