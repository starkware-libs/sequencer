use std::sync::{Arc, LazyLock};

use apollo_class_manager_types::{
    ClassManagerClientResult,
    ExecutableClass,
    MockClassManagerClient,
};
use apollo_state_sync_types::communication::{MockStateSyncClient, StateSyncClientResult};
use apollo_state_sync_types::state_sync_types::SyncBlock;
use apollo_test_utils::{get_rng, GetTestInstance};
use blockifier::execution::contract_class::RunnableCompiledClass;
use blockifier::state::errors::StateError;
use blockifier::state::global_cache::CompiledClasses;
use blockifier::state::state_api::{StateReader, StateResult};
use blockifier::state::state_api_test_utils::assert_eq_state_result;
use blockifier::state::state_reader_and_contract_manager::FetchCompiledClasses;
use mockall::predicate;
use rstest::rstest;
use starknet_api::block::{
    BlockHeaderWithoutHash,
    BlockInfo,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPricePerToken,
    GasPriceVector,
    GasPrices,
    NonzeroGasPrice,
};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, ContractAddress, SequencerContractAddress};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::state::SierraContractClass;
use starknet_api::{class_hash, contract_address, felt, nonce, storage_key};
use starknet_types_core::felt::Felt;

use crate::gateway_fixed_block_state_reader::{
    GatewayFixedBlockStateReader,
    GatewayFixedBlockSyncStateClient,
};
use crate::state_reader::StateReaderFactory;
use crate::sync_state_reader::{SyncStateReader, SyncStateReaderFactory};

static DUMMY_CLASS_HASH: LazyLock<ClassHash> = LazyLock::new(|| class_hash!(2_u32));

#[tokio::test]
async fn test_get_block_info() {
    let mut mock_state_sync_client = MockStateSyncClient::new();
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

    let gateway_fixed_block_sync_state_client =
        GatewayFixedBlockSyncStateClient::new(Arc::new(mock_state_sync_client), block_number);
    let result = gateway_fixed_block_sync_state_client.get_block_info().await.unwrap();

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

    let state_sync_reader = SyncStateReader::from_number(
        Arc::new(mock_state_sync_client),
        Arc::new(mock_class_manager_client),
        block_number,
        tokio::runtime::Handle::current(),
    );

    let result = tokio::task::spawn_blocking(move || {
        state_sync_reader.get_storage_at(contract_address, storage_key)
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
    let block_number = BlockNumber(1);
    let contract_address = contract_address!("0x2");
    let expected_result = nonce!(0x3);

    mock_state_sync_client
        .expect_get_nonce_at()
        .times(1)
        .with(predicate::eq(block_number), predicate::eq(contract_address))
        .returning(move |_, _| Ok(expected_result));

    let state_sync_reader = SyncStateReader::from_number(
        Arc::new(mock_state_sync_client),
        Arc::new(mock_class_manager_client),
        block_number,
        tokio::runtime::Handle::current(),
    );

    let result =
        tokio::task::spawn_blocking(move || state_sync_reader.get_nonce_at(contract_address))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(result, expected_result);
}

#[tokio::test]
async fn test_get_class_hash_at() {
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let mock_class_manager_client = MockClassManagerClient::new();
    let block_number = BlockNumber(1);
    let contract_address = contract_address!("0x2");
    let expected_result = class_hash!("0x3");

    mock_state_sync_client
        .expect_get_class_hash_at()
        .times(1)
        .with(predicate::eq(block_number), predicate::eq(contract_address))
        .returning(move |_, _| Ok(expected_result));

    let state_sync_reader = SyncStateReader::from_number(
        Arc::new(mock_state_sync_client),
        Arc::new(mock_class_manager_client),
        block_number,
        tokio::runtime::Handle::current(),
    );

    let result =
        tokio::task::spawn_blocking(move || state_sync_reader.get_class_hash_at(contract_address))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(result, expected_result);
}

#[rstest]
#[case::class_declared(
    Ok(Some(ContractClass::test_casm_contract_class())),
    1,
    Ok(true),
    Ok(RunnableCompiledClass::test_casm_contract_class()),
    *DUMMY_CLASS_HASH,
)]
#[case::class_not_declared_but_in_class_manager(
    Ok(Some(ContractClass::test_casm_contract_class())),
    0,
    Ok(false),
    Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)),
    *DUMMY_CLASS_HASH,
)]
#[case::class_not_declared(
    Ok(None),
    0,
    Ok(false),
    Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)),
    *DUMMY_CLASS_HASH,
)]
#[tokio::test]
async fn test_get_compiled_class(
    #[case] get_executable_result: ClassManagerClientResult<Option<ExecutableClass>>,
    #[case] n_calls_to_get_executable: usize,
    #[case] is_class_declared_at_result: StateSyncClientResult<bool>,
    #[case] expected_result: StateResult<RunnableCompiledClass>,
    #[case] class_hash: ClassHash,
) {
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let mut mock_class_manager_client = MockClassManagerClient::new();

    let block_number = BlockNumber(1);

    mock_class_manager_client
        .expect_get_executable()
        .times(n_calls_to_get_executable)
        .with(predicate::eq(class_hash))
        .return_once(move |_| get_executable_result);

    mock_state_sync_client
        .expect_is_class_declared_at()
        .times(1)
        .with(predicate::eq(block_number), predicate::eq(class_hash))
        .return_once(move |_, _| is_class_declared_at_result);

    let state_sync_reader = SyncStateReader::from_number(
        Arc::new(mock_state_sync_client),
        Arc::new(mock_class_manager_client),
        block_number,
        tokio::runtime::Handle::current(),
    );
    let result =
        tokio::task::spawn_blocking(move || state_sync_reader.get_compiled_class(class_hash))
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
        1,
        Ok(true),
        Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)),
        *DUMMY_CLASS_HASH,
    )
    .await;
}

#[rstest]
#[case::cairo_0_class_declared(
    Ok(true),
    Ok(Some(ContractClass::test_deprecated_casm_contract_class())),
    1,
    Ok(None),
    0,
    Ok(CompiledClasses::from_runnable_for_testing(
        RunnableCompiledClass::test_deprecated_casm_contract_class(),
    ))
)]
#[case::class_declared(
    Ok(true),
    Ok(Some(ContractClass::test_casm_contract_class())),
    1,
    Ok(Some(SierraContractClass::default())),
    1,
    Ok(CompiledClasses::from_runnable_for_testing(
        RunnableCompiledClass::test_casm_contract_class(),
    ))
)]
#[case::class_not_declared_but_in_class_manager(
    Ok(false),
    Ok(Some(ContractClass::test_casm_contract_class())),
    0,
    Ok(Some(SierraContractClass::default())),
    0,
    Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)),
)]
#[case::class_not_declared(
    Ok(false),
    Ok(None),
    0,
    Ok(None),
    0,
    Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)),
)]
#[tokio::test]
async fn test_fetch_compiled_classes_get_compiled_classes(
    #[case] is_class_declared_at_result: StateSyncClientResult<bool>,
    #[case] get_executable_result: ClassManagerClientResult<Option<ExecutableClass>>,
    #[case] n_calls_to_get_executable: usize,
    #[case] get_sierra_result: ClassManagerClientResult<Option<SierraContractClass>>,
    #[case] n_calls_to_get_sierra: usize,
    #[case] expected_result: StateResult<CompiledClasses>,
) {
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let mut mock_class_manager_client = MockClassManagerClient::new();

    let block_number = BlockNumber(0);
    let class_hash = *DUMMY_CLASS_HASH;

    mock_state_sync_client
        .expect_is_class_declared_at()
        .times(1)
        .with(predicate::eq(block_number), predicate::eq(class_hash))
        .return_once(|_, _| is_class_declared_at_result);
    mock_class_manager_client
        .expect_get_executable()
        .times(n_calls_to_get_executable)
        .with(predicate::eq(class_hash))
        .return_once(|_| get_executable_result);
    mock_class_manager_client
        .expect_get_sierra()
        .times(n_calls_to_get_sierra)
        .with(predicate::eq(class_hash))
        .return_once(|_| get_sierra_result);

    let state_sync_reader = SyncStateReader::from_number(
        Arc::new(mock_state_sync_client),
        Arc::new(mock_class_manager_client),
        block_number,
        tokio::runtime::Handle::current(),
    );

    let result =
        tokio::task::spawn_blocking(move || state_sync_reader.get_compiled_classes(class_hash))
            .await
            .unwrap();
    assert_eq_state_result(&result, &expected_result);
}

#[rstest]
#[case::declared(Ok(true), Ok(true))]
#[case::not_declared(Ok(false), Ok(false))]
#[tokio::test]
async fn test_fetch_compiled_classes_is_declared(
    #[case] is_cairo_1_declared_result: StateSyncClientResult<bool>,
    #[case] expected_result: StateResult<bool>,
) {
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let mock_class_manager_client = MockClassManagerClient::new();

    let block_number = BlockNumber(0);
    let class_hash = *DUMMY_CLASS_HASH;

    mock_state_sync_client
        .expect_is_cairo_1_class_declared_at()
        .times(1)
        .with(predicate::eq(block_number), predicate::eq(class_hash))
        .return_once(|_, _| is_cairo_1_declared_result);

    let state_sync_reader = SyncStateReader::from_number(
        Arc::new(mock_state_sync_client),
        Arc::new(mock_class_manager_client),
        block_number,
        tokio::runtime::Handle::current(),
    );

    let result = tokio::task::spawn_blocking(move || state_sync_reader.is_declared(class_hash))
        .await
        .unwrap();
    assert_eq!(result.expect("unexpected error in state reader"), expected_result.unwrap())
}

#[tokio::test]
async fn test_returns_genesis_readers_when_no_blocks_exist() {
    use apollo_class_manager_types::{EmptyClassManagerClient, SharedClassManagerClient};
    use apollo_state_sync_types::communication::SharedStateSyncClient;
    use assert_matches::assert_matches;
    use blockifier::state::errors::StateError;

    let mut state_sync_client = MockStateSyncClient::new();
    state_sync_client.expect_get_latest_block_number().times(1).returning(|| Ok(None));

    let shared_state_sync_client: SharedStateSyncClient = Arc::new(state_sync_client);
    let class_manager_client: SharedClassManagerClient = Arc::new(EmptyClassManagerClient);

    let factory = SyncStateReaderFactory {
        shared_state_sync_client,
        class_manager_client,
        runtime: tokio::runtime::Handle::current(),
    };

    let (state_reader, fixed_block_reader) = factory
        .get_blockifier_state_reader_and_gateway_fixed_block_from_latest_block()
        .await
        .unwrap();

    // All storage, nonce, and class hash operations should return zero.
    assert_eq!(
        state_reader.get_storage_at(contract_address!("0x1"), storage_key!("0x2")).unwrap(),
        Felt::default()
    );
    assert_eq!(state_reader.get_nonce_at(contract_address!("0x1")).unwrap(), Default::default());
    assert_eq!(
        state_reader.get_class_hash_at(contract_address!("0x1")).unwrap(),
        Default::default()
    );

    // Asking for a compiled class should return an error.
    let class_hash = class_hash!(123_u32);
    assert_matches!(
        state_reader.get_compiled_class(class_hash),
        Err(StateError::UndeclaredClassHash(ch)) if ch == class_hash
    );
    assert_matches!(
        state_reader.get_compiled_classes(class_hash),
        Err(StateError::UndeclaredClassHash(ch)) if ch == class_hash
    );
    assert!(!state_reader.is_declared(class_hash).unwrap());

    // The genesis fixed-block reader should return the "minimal genesis block".
    let block_info = fixed_block_reader.get_block_info().await.unwrap();
    assert_eq!(block_info.block_number, BlockNumber(0));
    assert_eq!(block_info.block_timestamp, BlockTimestamp(0));
    assert_eq!(block_info.sequencer_address, ContractAddress::default());
    assert!(!block_info.use_kzg_da);

    // Ensure gas prices are non-zero.
    let one: NonzeroGasPrice = 1_u128.try_into().unwrap();
    assert_eq!(block_info.gas_prices.eth_gas_prices.l1_gas_price, one);
    assert_eq!(block_info.gas_prices.eth_gas_prices.l1_data_gas_price, one);
    assert_eq!(block_info.gas_prices.eth_gas_prices.l2_gas_price, one);
    assert_eq!(block_info.gas_prices.strk_gas_prices.l1_gas_price, one);
    assert_eq!(block_info.gas_prices.strk_gas_prices.l1_data_gas_price, one);
    assert_eq!(block_info.gas_prices.strk_gas_prices.l2_gas_price, one);

    assert_eq!(
        fixed_block_reader.get_nonce(contract_address!("0x1")).await.unwrap(),
        Default::default()
    );
}

#[tokio::test]
async fn test_returns_latest_block_fixed_reader_when_blocks_exist() {
    use apollo_class_manager_types::{EmptyClassManagerClient, SharedClassManagerClient};
    use apollo_state_sync_types::communication::SharedStateSyncClient;

    let latest = BlockNumber(0);

    let mut state_sync_client = MockStateSyncClient::new();
    state_sync_client.expect_get_latest_block_number().times(1).returning(move || Ok(Some(latest)));

    let header = BlockHeaderWithoutHash {
        block_number: latest,
        timestamp: BlockTimestamp(99),
        sequencer: SequencerContractAddress(ContractAddress::default()),
        l1_gas_price: GasPricePerToken { price_in_wei: GasPrice(11), price_in_fri: GasPrice(21) },
        l1_data_gas_price: GasPricePerToken {
            price_in_wei: GasPrice(12),
            price_in_fri: GasPrice(22),
        },
        l2_gas_price: GasPricePerToken { price_in_wei: GasPrice(13), price_in_fri: GasPrice(23) },
        l1_da_mode: L1DataAvailabilityMode::Blob,
        ..Default::default()
    };
    let sync_block = SyncBlock { block_header_without_hash: header.clone(), ..Default::default() };

    // Assert the fixed-block reader asks state sync for exactly the latest block.
    state_sync_client.expect_get_block().times(1).returning(move |bn| {
        assert_eq!(bn, latest);
        Ok(sync_block.clone())
    });

    let shared_state_sync_client: SharedStateSyncClient = Arc::new(state_sync_client);
    let class_manager_client: SharedClassManagerClient = Arc::new(EmptyClassManagerClient);

    let factory = SyncStateReaderFactory {
        shared_state_sync_client,
        class_manager_client,
        runtime: tokio::runtime::Handle::current(),
    };

    let (_state_reader, fixed_block_reader) = factory
        .get_blockifier_state_reader_and_gateway_fixed_block_from_latest_block()
        .await
        .unwrap();

    let block_info = fixed_block_reader.get_block_info().await.unwrap();
    assert_eq!(block_info.block_number, latest);
    assert_eq!(block_info.block_timestamp, BlockTimestamp(99));
    assert_eq!(block_info.sequencer_address, ContractAddress::default());
    assert!(block_info.use_kzg_da);

    let expected_wei_l1: NonzeroGasPrice = GasPrice(11).try_into().unwrap();
    let expected_wei_l1_data: NonzeroGasPrice = GasPrice(12).try_into().unwrap();
    let expected_wei_l2: NonzeroGasPrice = GasPrice(13).try_into().unwrap();
    let expected_fri_l1: NonzeroGasPrice = GasPrice(21).try_into().unwrap();
    let expected_fri_l1_data: NonzeroGasPrice = GasPrice(22).try_into().unwrap();
    let expected_fri_l2: NonzeroGasPrice = GasPrice(23).try_into().unwrap();

    assert_eq!(block_info.gas_prices.eth_gas_prices.l1_gas_price, expected_wei_l1);
    assert_eq!(block_info.gas_prices.eth_gas_prices.l1_data_gas_price, expected_wei_l1_data);
    assert_eq!(block_info.gas_prices.eth_gas_prices.l2_gas_price, expected_wei_l2);
    assert_eq!(block_info.gas_prices.strk_gas_prices.l1_gas_price, expected_fri_l1);
    assert_eq!(block_info.gas_prices.strk_gas_prices.l1_data_gas_price, expected_fri_l1_data);
    assert_eq!(block_info.gas_prices.strk_gas_prices.l2_gas_price, expected_fri_l2);
}
