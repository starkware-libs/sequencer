use std::sync::Arc;

use apollo_class_manager_types::{MockClassManagerClient, SharedClassManagerClient};
use apollo_state_sync_types::communication::{MockStateSyncClient, SharedStateSyncClient};
use apollo_state_sync_types::state_sync_types::SyncBlock;
use apollo_test_utils::{get_rng, GetTestInstance};
use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::state_api::StateReader;
use blockifier::state::state_reader_and_contract_manager::StateReaderAndContractManager;
use blockifier::test_utils::initial_test_state::state_reader_and_contract_manager_for_testing;
use mockall::predicate;
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
use starknet_api::core::SequencerContractAddress;
use starknet_api::data_availability::L1DataAvailabilityMode;
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
    let result = state_reader_and_contract_manager.get_block_info().await.unwrap();

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
