use std::sync::Arc;

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
use blockifier::state::state_api::{StateReader, StateResult};
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
use starknet_api::{class_hash, contract_address, felt, nonce, storage_key};

use crate::state_reader::MempoolStateReader;
use crate::sync_state_reader::SyncStateReader;
#[tokio::test]
async fn test_get_block_info() {
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let mock_class_manager_client = MockClassManagerClient::new();
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

    let state_sync_reader = SyncStateReader::from_number(
        Arc::new(mock_state_sync_client),
        Arc::new(mock_class_manager_client),
        block_number,
        tokio::runtime::Handle::current(),
    );
    let result = state_sync_reader.get_block_info().unwrap();

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
#[case::class_declared(
    Ok(Some(ContractClass::V1((dummy_casm_contract_class(), SierraVersion::default())))),
    Ok(true),
    Ok(RunnableCompiledClass::V1((dummy_casm_contract_class(), SierraVersion::default()).try_into().unwrap()))
)]
#[case::class_not_declared_but_in_class_manager(
    Ok(Some(ContractClass::V1((dummy_casm_contract_class(), SierraVersion::default())))),
    Ok(false),
    Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH))
)]
#[case::class_not_declared(
    Ok(None),
    Ok(false),
    Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH))
)]
#[tokio::test]
async fn test_get_compiled_class(
    #[case] class_manager_client_result: ClassManagerClientResult<Option<ExecutableClass>>,
    #[case] sync_client_result: StateSyncClientResult<bool>,
    #[case] expected_result: StateResult<RunnableCompiledClass>,
) {
    let mut mock_state_sync_client = MockStateSyncClient::new();
    let mut mock_class_manager_client = MockClassManagerClient::new();

    let class_hash = *DUMMY_CLASS_HASH;
    let block_number = BlockNumber(1);

    mock_class_manager_client
        .expect_get_executable()
        .times(0..=1)
        .with(predicate::eq(class_hash))
        .returning(move |_| class_manager_client_result.clone());

    mock_state_sync_client
        .expect_is_class_declared_at()
        .times(1)
        .with(predicate::eq(block_number), predicate::eq(class_hash))
        .return_once(move |_, _| sync_client_result);

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
#[should_panic]
async fn test_get_compiled_class_panics_when_class_exists_in_sync_but_not_in_class_manager() {
    test_get_compiled_class(
        Ok(None),
        Ok(true),
        Err(StateError::UndeclaredClassHash(*DUMMY_CLASS_HASH)),
    )
    .await;
}
