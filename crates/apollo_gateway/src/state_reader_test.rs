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
use blockifier::test_utils::initial_test_state::state_reader_and_contract_manager_for_testing;
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
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, SequencerContractAddress};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::state::SierraContractClass;
use starknet_api::{class_hash, contract_address, felt, nonce, storage_key};

use crate::state_reader::{GatewayStateReaderWithCompiledClasses, MempoolStateReader};
use crate::sync_state_reader::SyncStateReader;

fn sync_state_reader_and_contract_manager(
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
    state_reader_and_contract_manager_for_testing(
        Box::new(state_sync_reader),
        contract_class_manager,
    )
}

struct GetCompiledClassTestScenario {
    expectations: GetCompiledClassTestExpectation,

    // Test result.
    expected_result: StateResult<RunnableCompiledClass>,
}

struct GetCompiledClassTestExpectation {
    // Class manager client.
    get_executable_result: ClassManagerClientResult<Option<ExecutableClass>>,
    n_calls_to_get_executable: usize,
    get_sierra_result: ClassManagerClientResult<Option<SierraContractClass>>,
    n_calls_to_get_sierra: usize,

    // State sync client.
    is_class_declared_at_result: Option<StateSyncClientResult<bool>>,
    is_cairo_1_class_declared_at_result: Option<StateSyncClientResult<bool>>,
}

fn add_expectation_to_mock_state_sync_client_and_mock_class_manager_client(
    mock_class_manager_client: &mut MockClassManagerClient,
    mock_state_sync_client: &mut MockStateSyncClient,
    expectation: GetCompiledClassTestExpectation,
) {
    add_expectation_to_mock_class_manager_client(
        mock_class_manager_client,
        expectation.get_executable_result,
        expectation.n_calls_to_get_executable,
        expectation.get_sierra_result,
        expectation.n_calls_to_get_sierra,
    );
    add_expectation_to_mock_state_sync_client(
        mock_state_sync_client,
        expectation.is_class_declared_at_result,
        expectation.is_cairo_1_class_declared_at_result,
    );
}

fn add_expectation_to_mock_state_sync_client(
    mock_state_sync_client: &mut MockStateSyncClient,
    is_class_declared_at_result: Option<StateSyncClientResult<bool>>,
    is_cairo_1_class_declared_at_result: Option<StateSyncClientResult<bool>>,
) {
    if let Some(is_class_declared_at_result) = is_class_declared_at_result {
        mock_state_sync_client
            .expect_is_class_declared_at()
            .times(1)
            .return_once(move |_, _| is_class_declared_at_result);
    }
    if let Some(is_cairo_1_class_declared_at_result) = is_cairo_1_class_declared_at_result {
        mock_state_sync_client
            .expect_is_cairo_1_class_declared_at()
            .times(1)
            .return_once(move |_, _| is_cairo_1_class_declared_at_result);
    }
}

fn add_expectation_to_mock_class_manager_client(
    mock_class_manager_client: &mut MockClassManagerClient,
    get_executable_result: ClassManagerClientResult<Option<ExecutableClass>>,
    n_calls_to_get_executable: usize,
    get_sierra_result: ClassManagerClientResult<Option<SierraContractClass>>,
    n_calls_to_get_sierra: usize,
) {
    mock_class_manager_client
        .expect_get_executable()
        .times(n_calls_to_get_executable)
        .return_once(move |_| get_executable_result);

    mock_class_manager_client
        .expect_get_sierra()
        .times(n_calls_to_get_sierra)
        .return_once(move |_| get_sierra_result);
}

const CACHED_EXPECTATION: GetCompiledClassTestExpectation = GetCompiledClassTestExpectation {
    get_executable_result: Ok(None), // Not called due to caching.
    n_calls_to_get_executable: 0,
    get_sierra_result: Ok(None), // Not called due to caching.
    n_calls_to_get_sierra: 0,
    is_class_declared_at_result: None, // Not called due to caching.
    is_cairo_1_class_declared_at_result: None,
};

// Factory functions for different scenarios.
fn cairo_1_declared_scenario() -> GetCompiledClassTestScenario {
    GetCompiledClassTestScenario {
        expectations: GetCompiledClassTestExpectation {
            get_executable_result: Ok(Some(DUMMY_CONTRACT_CLASS.clone())),
            n_calls_to_get_executable: 1,
            get_sierra_result: Ok(Some(SierraContractClass::default())),
            n_calls_to_get_sierra: 1,
            is_class_declared_at_result: Some(Ok(true)),
            is_cairo_1_class_declared_at_result: None,
        },
        expected_result: Ok(DUMMY_COMPILED_CLASS.clone()),
    }
}

fn cairo_0_declared_scenario() -> GetCompiledClassTestScenario {
    GetCompiledClassTestScenario {
        expectations: GetCompiledClassTestExpectation {
            get_executable_result: Ok(Some(DUMMY_CONTRACT_CLASS_V0.clone())),
            n_calls_to_get_executable: 1,
            get_sierra_result: Ok(None), // Cairo 0 doesn't use Sierra.
            n_calls_to_get_sierra: 0,
            is_class_declared_at_result: Some(Ok(true)),
            is_cairo_1_class_declared_at_result: None,
        },
        expected_result: Ok(DUMMY_COMPILED_CLASS_V0.clone()),
    }
}

fn not_declared_scenario() -> GetCompiledClassTestScenario {
    GetCompiledClassTestScenario {
        expectations: GetCompiledClassTestExpectation {
            get_executable_result: Ok(None), // Not called since not declared.
            n_calls_to_get_executable: 0,
            get_sierra_result: Ok(None), // Not called since not declared.
            n_calls_to_get_sierra: 0,
            is_class_declared_at_result: Some(Ok(false)),
            is_cairo_1_class_declared_at_result: None,
        },
        expected_result: Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)),
    }
}

fn cached_cairo_1_declared_scenario() -> GetCompiledClassTestScenario {
    GetCompiledClassTestScenario {
        expectations: GetCompiledClassTestExpectation {
            is_cairo_1_class_declared_at_result: Some(Ok(true)), // Verification call.
            ..CACHED_EXPECTATION
        },
        expected_result: Ok(DUMMY_COMPILED_CLASS.clone()),
    }
}

fn cached_cairo_0_declared_scenario() -> GetCompiledClassTestScenario {
    GetCompiledClassTestScenario {
        expectations: GetCompiledClassTestExpectation {
            is_cairo_1_class_declared_at_result: None, // Not called for Cairo 0.
            ..CACHED_EXPECTATION
        },
        expected_result: Ok(DUMMY_COMPILED_CLASS_V0.clone()),
    }
}

fn cached_but_verification_failed_after_reorg_scenario() -> GetCompiledClassTestScenario {
    GetCompiledClassTestScenario {
        expectations: GetCompiledClassTestExpectation {
            is_cairo_1_class_declared_at_result: Some(Ok(false)), // Verification fails.
            ..CACHED_EXPECTATION
        },
        expected_result: Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)),
    }
}

fn not_declared_but_in_manager_scenario() -> GetCompiledClassTestScenario {
    GetCompiledClassTestScenario {
        expectations: GetCompiledClassTestExpectation {
            get_executable_result: Ok(Some(DUMMY_CONTRACT_CLASS.clone())), /* In manager but not
                                                                            * declared. */
            n_calls_to_get_executable: 0, // Not called since not declared.
            get_sierra_result: Ok(Some(SierraContractClass::default())),
            n_calls_to_get_sierra: 0, // Not called since not declared.
            is_class_declared_at_result: Some(Ok(false)),
            is_cairo_1_class_declared_at_result: None,
        },
        expected_result: Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)),
    }
}

fn declared_but_not_in_manager_scenario() -> GetCompiledClassTestScenario {
    GetCompiledClassTestScenario {
        expectations: GetCompiledClassTestExpectation {
            get_executable_result: Ok(None),             // Not in manager
            n_calls_to_get_executable: 1,                // Called once since declared
            get_sierra_result: Ok(None),                 // Not in manager
            n_calls_to_get_sierra: 1,                    // Called once since declared
            is_class_declared_at_result: Some(Ok(true)), // Declared but not in manager
            is_cairo_1_class_declared_at_result: None,   // Not called since no executable
        },
        expected_result: Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)), /* Should panic, not return error */
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

    let state_reader_and_contract_manager = sync_state_reader_and_contract_manager(
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

    let state_reader_and_contract_manager = sync_state_reader_and_contract_manager(
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

    let state_reader_and_contract_manager = sync_state_reader_and_contract_manager(
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

    let state_reader_and_contract_manager = sync_state_reader_and_contract_manager(
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

lazy_static! {
    static ref DUMMY_CLASS_HASH: ClassHash = class_hash!("0x2");
    static ref DUMMY_CONTRACT_CLASS: ContractClass = ContractClass::test_casm_contract_class();
    static ref DUMMY_CONTRACT_CLASS_V0: ContractClass =
        ContractClass::test_deprecated_casm_contract_class();
    static ref DUMMY_COMPILED_CLASS: RunnableCompiledClass =
        DUMMY_CONTRACT_CLASS.clone().try_into().unwrap();
    static ref DUMMY_COMPILED_CLASS_V0: RunnableCompiledClass =
        DUMMY_CONTRACT_CLASS_V0.clone().try_into().unwrap();
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

#[rstest]
#[case::cairo_0_declared_and_cached(
    cairo_0_declared_scenario(),
    cached_cairo_0_declared_scenario()
)]
#[case::cairo_1_declared_and_cached(
    cairo_1_declared_scenario(),
    cached_cairo_1_declared_scenario()
)]
#[case::cairo_1_declared_then_verification_failed_after_reorg(
    cairo_1_declared_scenario(),
    cached_but_verification_failed_after_reorg_scenario()
)]
#[case::not_declared_but_in_manager_then_declared(
    not_declared_but_in_manager_scenario(),
    cairo_1_declared_scenario()
)]
#[case::not_declared_then_declared(not_declared_scenario(), cairo_1_declared_scenario())]
#[case::not_declared_both_rounds(not_declared_scenario(), not_declared_scenario())]
#[tokio::test]
async fn test_get_compiled_class(
    #[case] first_scenario: GetCompiledClassTestScenario,
    #[case] second_scenario: GetCompiledClassTestScenario,
) {
    let block_number = BlockNumber(0); // Not used in the test.
    let class_hash = *DUMMY_CLASS_HASH;

    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());

    // First execution.
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let mut mock_class_manager_client = MockClassManagerClient::new();
    add_expectation_to_mock_state_sync_client_and_mock_class_manager_client(
        &mut mock_class_manager_client,
        &mut mock_state_sync_client,
        first_scenario.expectations,
    );

    let first_state_reader_and_class_manager = sync_state_reader_and_contract_manager(
        Arc::new(mock_state_sync_client),
        Arc::new(mock_class_manager_client),
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

    // Second execution.
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let mut mock_class_manager_client = MockClassManagerClient::new();
    add_expectation_to_mock_state_sync_client_and_mock_class_manager_client(
        &mut mock_class_manager_client,
        &mut mock_state_sync_client,
        second_scenario.expectations,
    );

    let second_state_reader_and_class_manager = sync_state_reader_and_contract_manager(
        Arc::new(mock_state_sync_client),
        Arc::new(mock_class_manager_client),
        contract_class_manager,
        block_number,
        tokio::runtime::Handle::current(),
    );
    let second_result = tokio::task::spawn_blocking({
        let state_reader = second_state_reader_and_class_manager;
        move || state_reader.get_compiled_class(class_hash)
    })
    .await
    .unwrap();

    // Verify results
    assert_eq_state_result(&first_result, &first_scenario.expected_result);
    assert_eq_state_result(&second_result, &second_scenario.expected_result);
}

#[tokio::test]
#[should_panic(expected = "Class with hash {class_hash:?} doesn't appear in class manager even \
                           though it was declared")]
async fn test_get_compiled_class_panics_when_class_exists_in_sync_but_not_in_class_manager() {
    // Use the caching scenarios test with a panic scenario as first (second scenario won't be
    // reached)
    test_get_compiled_class(
        declared_but_not_in_manager_scenario(),
        not_declared_scenario(), // This won't be reached due to panic
    )
    .await;
}
