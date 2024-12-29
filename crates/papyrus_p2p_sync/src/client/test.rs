use std::collections::HashMap;

use futures::FutureExt;
use indexmap::IndexMap;
use papyrus_protobuf::sync::DataOrFin;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_test_utils::{get_rng, GetTestInstance};
use starknet_api::block::{BlockHeaderWithoutHash, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;
use starknet_state_sync_types::state_sync_types::SyncBlock;

use crate::client::test_utils::{
    random_header,
    run_test,
    wait_for_marker,
    Action,
    DataType,
    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
    TIMEOUT_FOR_TEST,
};

#[tokio::test]
async fn receive_block_internally() {
    let transaction_hashes_len = 2;
    let sync_block = create_random_sync_block(BlockNumber(0), transaction_hashes_len);

    run_test(
        HashMap::new(),
        vec![
            Action::SendInternalBlock(BlockNumber(0), sync_block),
            Action::RunP2pSync,
            // Check storage using StateSync fn get_block once introduced into flow
            // TODO(Eitan): test rest of data types
            Action::CheckStorage(Box::new(move |reader| {
                async move {
                    wait_for_marker(
                        DataType::Header,
                        &reader,
                        BlockNumber(1),
                        SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                        TIMEOUT_FOR_TEST,
                    )
                    .await;

                    assert_eq!(
                        reader.begin_ro_txn().unwrap().get_header_marker().unwrap(),
                        BlockNumber(1)
                    );
                    let txn = reader.begin_ro_txn().unwrap();
                    let block_header = txn.get_block_header(BlockNumber(0)).unwrap();
                    assert!(block_header.clone().is_some());
                    assert!(
                        block_header.clone().unwrap().n_transactions
                            == Into::<usize>::into(transaction_hashes_len)
                    );
                    assert!(block_header.unwrap().state_diff_length.unwrap() == 1);
                }
                .boxed()
            })),
        ],
    )
    .await;
}

#[tokio::test]
async fn receive_blocks_out_of_order() {
    let sync_block_0 = create_random_sync_block(BlockNumber(0), 1);
    let block_header_without_hash_0 = sync_block_0.block_header_without_hash.clone();
    let sync_block_1 = create_random_sync_block(BlockNumber(1), 1);
    let block_header_without_hash_1 = sync_block_1.block_header_without_hash.clone();

    run_test(
        HashMap::new(),
        vec![
            Action::SendInternalBlock(BlockNumber(1), sync_block_1),
            Action::SendInternalBlock(BlockNumber(0), sync_block_0),
            Action::RunP2pSync,
            // Check storage using StateSync fn get_block once introduced into flow
            Action::CheckStorage(Box::new(move |reader| {
                async move {
                    wait_for_marker(
                        DataType::Header,
                        &reader,
                        BlockNumber(2),
                        SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                        TIMEOUT_FOR_TEST,
                    )
                    .await;

                    assert_eq!(
                        reader.begin_ro_txn().unwrap().get_header_marker().unwrap(),
                        BlockNumber(2)
                    );
                    let txn = reader.begin_ro_txn().unwrap();
                    // TODO(Eitan): test rest of data types
                    assert_eq!(
                        txn.get_block_header(BlockNumber(0))
                            .unwrap()
                            .unwrap()
                            .block_header_without_hash,
                        block_header_without_hash_0
                    );
                    assert_eq!(
                        txn.get_block_header(BlockNumber(1))
                            .unwrap()
                            .unwrap()
                            .block_header_without_hash,
                        block_header_without_hash_1
                    );
                }
                .boxed()
            })),
        ],
    )
    .await;
}

#[tokio::test]
async fn receive_blocks_first_externally_and_then_internally() {
    let sync_block_0 = create_random_sync_block(BlockNumber(0), 1);
    let sync_block_1 = create_random_sync_block(BlockNumber(1), 1);
    run_test(
        HashMap::from([(DataType::Header, 2)]),
        vec![
            Action::RunP2pSync,
            // We already validate the query content in other tests.
            Action::ReceiveQuery(Box::new(|_query| ()), DataType::Header),
            Action::SendHeader(DataOrFin(Some(random_header(
                &mut get_rng(),
                BlockNumber(0),
                None,
                None,
            )))),
            Action::CheckStorage(Box::new(|reader| {
                async move {
                    wait_for_marker(
                        DataType::Header,
                        &reader,
                        BlockNumber(1),
                        SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                        TIMEOUT_FOR_TEST,
                    )
                    .await;
                    assert_eq!(
                        BlockNumber(1),
                        reader.begin_ro_txn().unwrap().get_header_marker().unwrap()
                    );
                }
                .boxed()
            })),
            Action::SendInternalBlock(BlockNumber(0), sync_block_0),
            Action::SendInternalBlock(BlockNumber(1), sync_block_1),
            Action::CheckStorage(Box::new(|reader| {
                async move {
                    wait_for_marker(
                        DataType::Header,
                        &reader,
                        BlockNumber(2),
                        SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                        TIMEOUT_FOR_TEST,
                    )
                    .await;
                    assert_eq!(
                        BlockNumber(2),
                        reader.begin_ro_txn().unwrap().get_header_marker().unwrap()
                    );
                }
                .boxed()
            })),
        ],
    )
    .await;
}

fn create_random_sync_block(block_number: BlockNumber, transaction_hashes_len: u8) -> SyncBlock {
    let mut rng = get_rng();
    let contract_address = ContractAddress::from(1_u128);
    let state_diff = ThinStateDiff {
        deployed_contracts: vec![(contract_address, ClassHash::get_test_instance(&mut rng))]
            .into_iter()
            .collect(),
        storage_diffs: IndexMap::new(),
        declared_classes: IndexMap::new(),
        deprecated_declared_classes: vec![],
        nonces: IndexMap::new(),
        replaced_classes: IndexMap::new(),
    };

    let mut transaction_hashes = vec![];
    for i in 0..transaction_hashes_len {
        transaction_hashes.push(TransactionHash::from(Vec::from([i; 32])));
    }
    let BlockHeaderWithoutHash {
        block_number: _,
        parent_hash,
        timestamp,
        state_root,
        l1_gas_price,
        l1_data_gas_price,
        l2_gas_price,
        sequencer,
        l1_da_mode,
        starknet_version,
    } = BlockHeaderWithoutHash::get_test_instance(&mut rng);
    let block_header_without_hash = BlockHeaderWithoutHash {
        block_number,
        parent_hash,
        timestamp,
        state_root,
        l1_gas_price,
        l1_data_gas_price,
        l2_gas_price,
        sequencer,
        l1_da_mode,
        starknet_version,
    };
    SyncBlock { state_diff, transaction_hashes, block_header_without_hash }
}
