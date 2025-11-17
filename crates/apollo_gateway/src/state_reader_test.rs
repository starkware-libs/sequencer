use std::sync::Arc;

use apollo_class_manager_types::{
    ClassManagerClientResult,
    ExecutableClass,
    MockClassManagerClient,
    SharedClassManagerClient,
};
use apollo_state_sync_types::communication::{
    MockStateSyncClient,
    SharedStateSyncClient,
    StateSyncClientResult,
};
use apollo_state_sync_types::state_sync_types::SyncBlock;
use apollo_test_utils::{get_rng, GetTestInstance};
use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::errors::StateError;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::state::state_reader_and_contract_manager::StateReaderAndContractManager;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use lazy_static::lazy_static;
use mockall::predicate;
use rstest::rstest;
use starknet_api::block::{
    BlockHeaderWithoutHash,
    BlockInfo,
    BlockNumber,
    BlockTimestamp,
    GasPricePerToken,
    GasPriceVector,
    GasPrices,
    NonzeroGasPrice,
};
use starknet_api::contract_class::{ContractClass, SierraVersion};
use starknet_api::core::{ClassHash, SequencerContractAddress};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::state::SierraContractClass;
use starknet_api::{class_hash, contract_address, felt, nonce, storage_key};

use crate::state_reader::{GatewayStateReaderWithCompiledClasses, MempoolStateReader};
use crate::sync_state_reader::SyncStateReader;

fn state_reader_and_contract_manager(
    state_sync_client: SharedStateSyncClient,
    class_manager_client: SharedClassManagerClient,
    contract_class_manager: ContractClassManager,
    block_number: BlockNumber,
    runtime: tokio::runtime::Handle,
) -> StateReaderAndContractManager<Box<dyn GatewayStateReaderWithCompiledClasses>> {
    let state_sync_reader = SyncStateReader::from_number(
        state_sync_client,
        class_manager_client,
        block_number,
        runtime,
    );
    StateReaderAndContractManager {
        state_reader: Box::new(state_sync_reader),
        contract_class_manager: contract_class_manager.clone(),
    }
}

#[tokio::test]
async fn test_get_block_info() {
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let mock_class_manager_client = MockClassManagerClient::new();
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());
    let block_number = BlockNumber(1);
    let block_timestamp = BlockTimestamp(2);
    let sequencer_address = contract_address!("0x3");
    let l1_gas_price = GasPricePerToken { price_in_wei: 4_u8.into(), price_in_fri: 5_u8.into() };
    let l1_data_gas_price =
        GasPricePerToken { price_in_wei: 6_u8.into(), price_in_fri: 7_u8.into() };
    let l2_gas_price = GasPricePerToken { price_in_wei: 8_u8.into(), price_in_fri: 9_u8.into() };
    let l1_da_mode = L1DataAvailabilityMode::get_test_instance(&mut get_rng());

    mock_state_sync_client.expect_get_block().times(1).with(predicate::eq(block_number)).returning(
        move |_| {
            Ok(SyncBlock {
                state_diff: Default::default(),
                account_transaction_hashes: Default::default(),
                l1_transaction_hashes: Default::default(),
                block_header_without_hash: BlockHeaderWithoutHash {
                    block_number,
                    l1_gas_price,
                    l1_data_gas_price,
                    l2_gas_price,
                    sequencer: SequencerContractAddress(sequencer_address),
                    timestamp: block_timestamp,
                    l1_da_mode,
                    ..Default::default()
                },
            })
        },
    );

    let state_reader_and_contract_manager = state_reader_and_contract_manager(
        Arc::new(mock_state_sync_client),
        Arc::new(mock_class_manager_client),
        contract_class_manager.clone(),
        block_number,
        tokio::runtime::Handle::current(),
    );
    let result = state_reader_and_contract_manager.get_block_info().unwrap();

    assert_eq!(
        result,
        BlockInfo {
            block_number,
            block_timestamp,
            sequencer_address,
            gas_prices: GasPrices {
                eth_gas_prices: GasPriceVector {
                    l1_gas_price: NonzeroGasPrice::new_unchecked(l1_gas_price.price_in_wei),
                    l1_data_gas_price: NonzeroGasPrice::new_unchecked(
                        l1_data_gas_price.price_in_wei
                    ),
                    l2_gas_price: NonzeroGasPrice::new_unchecked(l2_gas_price.price_in_wei),
                },
                strk_gas_prices: GasPriceVector {
                    l1_gas_price: NonzeroGasPrice::new_unchecked(l1_gas_price.price_in_fri),
                    l1_data_gas_price: NonzeroGasPrice::new_unchecked(
                        l1_data_gas_price.price_in_fri
                    ),
                    l2_gas_price: NonzeroGasPrice::new_unchecked(l2_gas_price.price_in_fri),
                },
            },
            use_kzg_da: match l1_da_mode {
                L1DataAvailabilityMode::Blob => true,
                L1DataAvailabilityMode::Calldata => false,
            },
        }
    );
}

#[tokio::test]
async fn test_get_storage_at() {
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let mock_class_manager_client = MockClassManagerClient::new();
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());
    let block_number = BlockNumber(1);
    let contract_address = contract_address!("0x2");
    let storage_key = storage_key!("0x3");
    let value = felt!("0x4");
    mock_state_sync_client
        .expect_get_storage_at()
        .times(1)
        .with(
            predicate::eq(block_number),
            predicate::eq(contract_address),
            predicate::eq(storage_key),
        )
        .returning(move |_, _, _| Ok(value));

    let state_reader_and_contract_manager = state_reader_and_contract_manager(
        Arc::new(mock_state_sync_client),
        Arc::new(mock_class_manager_client),
        contract_class_manager.clone(),
        block_number,
        tokio::runtime::Handle::current(),
    );
    let result = tokio::task::spawn_blocking(move || {
        state_reader_and_contract_manager.get_storage_at(contract_address, storage_key)
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result, value);
}

#[tokio::test]
async fn test_get_nonce_at() {
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let mock_class_manager_client = MockClassManagerClient::new();
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());
    let block_number = BlockNumber(1);
    let contract_address = contract_address!("0x2");
    let expected_result = nonce!(0x3);

    mock_state_sync_client
        .expect_get_nonce_at()
        .times(1)
        .with(predicate::eq(block_number), predicate::eq(contract_address))
        .returning(move |_, _| Ok(expected_result));

    let state_reader_and_contract_manager = state_reader_and_contract_manager(
        Arc::new(mock_state_sync_client),
        Arc::new(mock_class_manager_client),
        contract_class_manager.clone(),
        block_number,
        tokio::runtime::Handle::current(),
    );

    let result = tokio::task::spawn_blocking(move || {
        state_reader_and_contract_manager.get_nonce_at(contract_address)
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result, expected_result);
}

#[tokio::test]
async fn test_get_class_hash_at() {
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let mock_class_manager_client = MockClassManagerClient::new();
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());
    let block_number = BlockNumber(1);
    let contract_address = contract_address!("0x2");
    let expected_result = class_hash!("0x3");

    mock_state_sync_client
        .expect_get_class_hash_at()
        .times(1)
        .with(predicate::eq(block_number), predicate::eq(contract_address))
        .returning(move |_, _| Ok(expected_result));

    let state_reader_and_contract_manager = state_reader_and_contract_manager(
        Arc::new(mock_state_sync_client),
        Arc::new(mock_class_manager_client),
        contract_class_manager.clone(),
        block_number,
        tokio::runtime::Handle::current(),
    );

    let result = tokio::task::spawn_blocking(move || {
        state_reader_and_contract_manager.get_class_hash_at(contract_address)
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result, expected_result);
}

fn dummy_casm_contract_class() -> CasmContractClass {
    CasmContractClass {
        compiler_version: "0.0.0".to_string(),
        prime: Default::default(),
        bytecode: Default::default(),
        bytecode_segment_lengths: Default::default(),
        hints: Default::default(),
        pythonic_hints: Default::default(),
        entry_points_by_type: Default::default(),
    }
}

lazy_static! {
    static ref DUMMY_CLASS_HASH: ClassHash = class_hash!("0x2");
    static ref DUMMY_CONTRACT_CLASS: ContractClass =
        ContractClass::V1((dummy_casm_contract_class(), SierraVersion::default()));
    static ref DUMMY_COMPILED_CLASS: RunnableCompiledClass = RunnableCompiledClass::V1(
        (dummy_casm_contract_class(), SierraVersion::default()).try_into().unwrap()
    );
}

fn assert_eq_state_result(
    a: &StateResult<RunnableCompiledClass>,
    b: &StateResult<RunnableCompiledClass>,
) {
    match (a, b) {
        (Ok(a), Ok(b)) => assert_eq!(a, b),
        (Err(StateError::UndeclaredClassHash(a)), Err(StateError::UndeclaredClassHash(b))) => {
            assert_eq!(a, b)
        }
        _ => panic!("StateResult mismatch (or unsupported comparison): {a:?} vs {b:?}"),
    }
}

// TODO(Arni): add test for class is Cairo 0.
#[rstest]
#[case::class_declared(
    Ok(Some(DUMMY_CONTRACT_CLASS.clone())),
    Ok(Some(SierraContractClass::default())),
    1,
    Ok(true),
    Ok(DUMMY_COMPILED_CLASS.clone()),
)]
#[case::class_not_declared_but_in_class_manager(
    Ok(Some(DUMMY_CONTRACT_CLASS.clone())),
    Ok(Some(SierraContractClass::default())),
    0,
    Ok(false),
    Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)),
)]
#[case::class_not_declared(
    Ok(None),
    Ok(None),
    0,
    Ok(false),
    Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)),
)]
#[tokio::test]
/// Test that the compiled class is returned correctly from the sync state reader and contract
/// manager struct.
/// Note that in different test cases, we simulate the state sync and class manager's state by using
/// mock clients, and deciding how many times each request is sent. This is a part of the tested
/// behavior.
async fn test_get_compiled_class(
    #[case] get_executable_result: ClassManagerClientResult<Option<ExecutableClass>>,
    #[case] get_sierra_result: ClassManagerClientResult<Option<SierraContractClass>>,
    #[case] n_calls_to_class_manager_client: usize,
    #[case] is_class_declared_at_result: StateSyncClientResult<bool>,
    #[case] expected_result: StateResult<RunnableCompiledClass>,
) {
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let mut mock_class_manager_client = MockClassManagerClient::new();
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());

    let block_number = BlockNumber(1);
    let class_hash = *DUMMY_CLASS_HASH;

    mock_class_manager_client
        .expect_get_executable()
        .times(n_calls_to_class_manager_client)
        .with(predicate::eq(class_hash))
        .return_once(move |_| get_executable_result);

    mock_class_manager_client
        .expect_get_sierra()
        .times(n_calls_to_class_manager_client)
        .with(predicate::eq(class_hash))
        .return_once(move |_| get_sierra_result);

    mock_state_sync_client
        .expect_is_class_declared_at()
        .times(1)
        .with(predicate::eq(block_number), predicate::eq(class_hash))
        .return_once(move |_, _| is_class_declared_at_result);

    let state_reader_and_contract_manager = state_reader_and_contract_manager(
        Arc::new(mock_state_sync_client),
        Arc::new(mock_class_manager_client),
        contract_class_manager.clone(),
        block_number,
        tokio::runtime::Handle::current(),
    );

    let result = tokio::task::spawn_blocking(move || {
        state_reader_and_contract_manager.get_compiled_class(class_hash)
    })
    .await
    .unwrap();

    assert_eq_state_result(&result, &expected_result);
}

#[tokio::test]
#[should_panic(expected = "Class with hash {class_hash:?} doesn't appear in class manager even \
                           though it was declared")]
async fn test_get_compiled_class_panics_when_class_exists_in_sync_but_not_in_class_manager() {
    test_get_compiled_class(
        Ok(None),
        Ok(None),
        1,
        Ok(true),
        Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)),
    )
    .await;
}

#[rstest]
#[case::first_declared_second_declared(
    Ok(Some(DUMMY_CONTRACT_CLASS.clone())), // first_get_executable
    Ok(Some(SierraContractClass::default())), // first_get_sierra
    1,
    Ok(true), // first_is_class_declared_at
    None, // second_get_executable (not called due to caching)
    None, // second_get_sierra (not called due to caching)
    None, // second_is_class_declared_at (not called due to caching)
    Some(Ok(true)), // second_is_cairo_1_class_declared_at (verification call)
    Ok(DUMMY_COMPILED_CLASS.clone()), // expected_first_result
    Ok(DUMMY_COMPILED_CLASS.clone()), // expected_second_result
)]
#[case::first_declared_second_not_declared(
    Ok(Some(DUMMY_CONTRACT_CLASS.clone())), // first_get_executable
    Ok(Some(SierraContractClass::default())), // first_get_sierra
    1,
    Ok(true), // first_is_class_declared_at
    None, // second_get_executable (not called due to caching)
    None, // second_get_sierra (not called due to caching)
    None, // second_is_class_declared_at (not called due to caching)
    Some(Ok(false)), // second_is_cairo_1_class_declared_at (verification call)
    Ok(DUMMY_COMPILED_CLASS.clone()), // expected_first_result
    Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)), // expected_second_result
)]
#[case::first_not_declared_but_in_manager_second_declared(
    Ok(Some(DUMMY_CONTRACT_CLASS.clone())), // first_get_executable
    Ok(Some(SierraContractClass::default())), // first_get_sierra
    0,
    Ok(false), // first_is_class_declared_at
    Some(Ok(Some(DUMMY_CONTRACT_CLASS.clone()))), // second_get_executable
    Some(Ok(Some(SierraContractClass::default()))), // second_get_sierra
    Some(Ok(true)), // second_is_class_declared_at
    None, // second_is_cairo_1_class_declared_at (not called since no cached class)
    Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)), // expected_first_result
    Ok(DUMMY_COMPILED_CLASS.clone()), // expected_second_result
)]
#[case::first_not_declared_second_declared(
    Ok(None), // first_get_executable (not called since not declared)
    Ok(None), // first_get_sierra (not called since not declared)
    0,
    Ok(false), // first_is_class_declared_at
    Some(Ok(Some(DUMMY_CONTRACT_CLASS.clone()))), // second_get_executable
    Some(Ok(Some(SierraContractClass::default()))), // second_get_sierra
    Some(Ok(true)), // second_is_class_declared_at
    None, // second_is_cairo_1_class_declared_at (not called since no cached class)
    Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)), // expected_first_result
    Ok(DUMMY_COMPILED_CLASS.clone()), // expected_second_result
)]
#[case::first_not_declared_second_not_declared(
    Ok(None), // first_get_executable (not called since not declared)
    Ok(None), // first_get_sierra (not called since not declared)
    0,
    Ok(false), // first_is_class_declared_at
    None, // second_get_executable (not called since not declared)
    None, // second_get_sierra (not called since not declared)
    Some(Ok(false)), // second_is_class_declared_at
    None, // second_is_cairo_1_class_declared_at (not called since not declared)
    Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)), // expected_first_result
    Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)), // expected_second_result
)]
#[allow(clippy::too_many_arguments)]
#[tokio::test]
async fn test_get_compiled_class_caching_scenarios(
    #[case] first_get_executable_result: ClassManagerClientResult<Option<ExecutableClass>>,
    #[case] first_get_sierra_result: ClassManagerClientResult<Option<SierraContractClass>>,
    #[case] first_n_calls_get_executable: usize,
    #[case] first_is_class_declared_at_result: StateSyncClientResult<bool>,
    #[case] second_get_executable_result: Option<ClassManagerClientResult<Option<ExecutableClass>>>,
    #[case] second_get_sierra_result: Option<ClassManagerClientResult<Option<SierraContractClass>>>,
    #[case] second_is_class_declared_at_result: Option<StateSyncClientResult<bool>>,
    #[case] second_is_cairo_1_class_declared_at_result: Option<StateSyncClientResult<bool>>,
    #[case] expected_first_result: StateResult<RunnableCompiledClass>,
    #[case] expected_second_result: StateResult<RunnableCompiledClass>,
    #[values(BlockNumber(1), BlockNumber(2), BlockNumber(3))] other_block_number: BlockNumber,
) {
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let mut mock_class_manager_client = MockClassManagerClient::new();
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());

    let block_number = BlockNumber(2);
    let class_hash = *DUMMY_CLASS_HASH;

    // Setup mocks for first execution
    mock_class_manager_client
        .expect_get_executable()
        .times(first_n_calls_get_executable)
        .with(predicate::eq(class_hash))
        .return_once(move |_| first_get_executable_result);

    mock_class_manager_client
        .expect_get_sierra()
        .times(first_n_calls_get_executable)
        .with(predicate::eq(class_hash))
        .return_once(move |_| first_get_sierra_result);

    mock_state_sync_client
        .expect_is_class_declared_at()
        .times(1)
        .with(predicate::eq(block_number), predicate::eq(class_hash))
        .return_once(move |_, _| first_is_class_declared_at_result);

    // Setup mocks for second execution
    if let Some(result) = second_get_executable_result {
        mock_class_manager_client
            .expect_get_executable()
            .times(1)
            .with(predicate::eq(class_hash))
            .return_once(move |_| result);
    }

    if let Some(result) = second_get_sierra_result {
        mock_class_manager_client
            .expect_get_sierra()
            .times(1)
            .with(predicate::eq(class_hash))
            .return_once(move |_| result);
    }

    if let Some(result) = second_is_class_declared_at_result {
        mock_state_sync_client
            .expect_is_class_declared_at()
            .times(1)
            .with(predicate::eq(other_block_number), predicate::eq(class_hash))
            .return_once(move |_, _| result);
    }

    if let Some(result) = second_is_cairo_1_class_declared_at_result {
        mock_state_sync_client
            .expect_is_cairo_1_class_declared_at()
            .times(1)
            .with(predicate::eq(other_block_number), predicate::eq(class_hash))
            .return_once(move |_, _| result);
    }

    let shared_state_sync_client = Arc::new(mock_state_sync_client);
    let shared_class_manager_client = Arc::new(mock_class_manager_client);

    // First execution: block_number (BlockNumber(2))
    let first_state_reader_and_class_manager = state_reader_and_contract_manager(
        shared_state_sync_client.clone(),
        shared_class_manager_client.clone(),
        contract_class_manager.clone(),
        block_number,
        tokio::runtime::Handle::current(),
    );

    let first_result = tokio::task::spawn_blocking({
        let state_reader = first_state_reader_and_class_manager;
        move || state_reader.get_compiled_class(class_hash)
    })
    .await
    .unwrap();

    // Second execution: other_block_number (using same ContractClassManager for caching)
    let second_state_reader_and_class_manager = state_reader_and_contract_manager(
        shared_state_sync_client,
        shared_class_manager_client,
        contract_class_manager,
        other_block_number,
        tokio::runtime::Handle::current(),
    );

    let second_result = tokio::task::spawn_blocking({
        let state_reader = second_state_reader_and_class_manager;
        move || state_reader.get_compiled_class(class_hash)
    })
    .await
    .unwrap();

    // Verify results
    assert_eq_state_result(&first_result, &expected_first_result);
    assert_eq_state_result(&second_result, &expected_second_result);
}
