use std::collections::HashMap;
use std::vec;

use apollo_protobuf::sync::DataOrFin;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use apollo_storage::body::BodyStorageReader;
use apollo_storage::header::HeaderStorageReader;
use apollo_storage::state::StateStorageReader;
use apollo_test_utils::{get_rng, GetTestInstance};
use futures::FutureExt;
use indexmap::IndexMap;
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use starknet_api::block::{BlockHeaderWithoutHash, BlockNumber};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::state::ThinStateDiff;
use starknet_api::transaction::TransactionHash;

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
    let account_transaction_hashes_len = 4;
    let l1_transaction_hashes_len = 2;
    let sync_block = create_random_sync_block(
        BlockNumber(0),
        account_transaction_hashes_len,
        l1_transaction_hashes_len,
        get_rng(),
    );
    let block_header_without_hash = sync_block.block_header_without_hash.clone();
    let transaction_hashes = sync_block.get_all_transaction_hashes();
    let state_diff = sync_block.state_diff.clone();

    run_test(
        HashMap::new(),
        None,
        vec![
            Action::SendInternalBlock(sync_block),
            Action::RunP2pSync,
            Action::CheckStorage(Box::new(move |reader| {
                async move {
                    wait_for_marker(
                        DataType::StateDiff,
                        &reader,
                        BlockNumber(1),
                        SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                        TIMEOUT_FOR_TEST,
                    )
                    .await;
                    wait_for_marker(
                        DataType::Transaction,
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
                            == Into::<usize>::into(transaction_hashes.len())
                    );
                    assert!(block_header.unwrap().state_diff_length.unwrap() == 1);
                    assert_eq!(
                        txn.get_block_header(BlockNumber(0))
                            .unwrap()
                            .unwrap()
                            .block_header_without_hash,
                        block_header_without_hash
                    );
                    assert_eq!(txn.get_state_diff(BlockNumber(0)).unwrap().unwrap(), state_diff);
                    assert_eq!(
                        txn.get_block_transaction_hashes(BlockNumber(0))
                            .unwrap()
                            .unwrap()
                            .as_slice(),
                        transaction_hashes.as_slice()
                    );
                }
                .boxed()
            })),
        ],
    )
    .await;
}

#[tokio::test]
async fn receive_blocks_out_of_order() {
    let mut rng = get_rng();
    let account_transaction_hashes_len_0 = 4;
    let l1_transaction_hashes_len_0 = 2;
    let sync_block_0 = create_random_sync_block(
        BlockNumber(0),
        account_transaction_hashes_len_0,
        l1_transaction_hashes_len_0,
        rng.clone(),
    );
    let block_header_without_hash_0 = sync_block_0.block_header_without_hash.clone();
    let transaction_hashes_0 = sync_block_0.get_all_transaction_hashes();
    let state_diff_0 = sync_block_0.state_diff.clone();

    // We need to forward the rng to the next generated num to make sure the blocks are different.
    rng.gen::<u8>();
    let account_transaction_hashes_len_1 = 3;
    let l1_transaction_hashes_len_1 = 1;
    let sync_block_1 = create_random_sync_block(
        BlockNumber(1),
        account_transaction_hashes_len_1,
        l1_transaction_hashes_len_1,
        rng,
    );
    let block_header_without_hash_1 = sync_block_1.block_header_without_hash.clone();
    let transaction_hashes_1 = sync_block_1.get_all_transaction_hashes();
    let state_diff_1 = sync_block_1.state_diff.clone();

    run_test(
        HashMap::new(),
        None,
        vec![
            Action::SendInternalBlock(sync_block_1),
            Action::SendInternalBlock(sync_block_0),
            Action::RunP2pSync,
            Action::CheckStorage(Box::new(move |reader| {
                async move {
                    wait_for_marker(
                        DataType::StateDiff,
                        &reader,
                        BlockNumber(2),
                        SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                        TIMEOUT_FOR_TEST,
                    )
                    .await;
                    wait_for_marker(
                        DataType::Transaction,
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
                    assert_eq!(txn.get_state_diff(BlockNumber(0)).unwrap().unwrap(), state_diff_0);
                    assert_eq!(
                        txn.get_block_transaction_hashes(BlockNumber(0))
                            .unwrap()
                            .unwrap()
                            .as_slice(),
                        transaction_hashes_0.as_slice()
                    );
                    assert_eq!(
                        txn.get_block_header(BlockNumber(1))
                            .unwrap()
                            .unwrap()
                            .block_header_without_hash,
                        block_header_without_hash_1
                    );
                    assert_eq!(txn.get_state_diff(BlockNumber(1)).unwrap().unwrap(), state_diff_1);
                    assert_eq!(
                        txn.get_block_transaction_hashes(BlockNumber(1))
                            .unwrap()
                            .unwrap()
                            .as_slice(),
                        transaction_hashes_1.as_slice()
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
    let rng = get_rng();
    let sync_block_0 = create_random_sync_block(BlockNumber(0), 1, 0, rng.clone());
    let sync_block_1 = create_random_sync_block(BlockNumber(1), 1, 0, rng);
    run_test(
        HashMap::from([(DataType::Header, 2)]),
        None,
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
            Action::SendInternalBlock(sync_block_0),
            Action::SendInternalBlock(sync_block_1),
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

fn create_random_sync_block(
    block_number: BlockNumber,
    account_transaction_hashes_len: u8,
    l1_transaction_hashes_len: u8,
    mut rng: ChaCha8Rng,
) -> SyncBlock {
    let contract_address = ContractAddress::from(1_u128);
    let state_diff = ThinStateDiff {
        deployed_contracts: vec![(contract_address, ClassHash::get_test_instance(&mut rng))]
            .into_iter()
            .collect(),
        storage_diffs: IndexMap::new(),
        declared_classes: IndexMap::new(),
        deprecated_declared_classes: vec![],
        nonces: IndexMap::new(),
    };

    let mut account_transaction_hashes = vec![];
    for _ in 0..account_transaction_hashes_len {
        account_transaction_hashes.push(TransactionHash::random(&mut rng));
    }
    let mut l1_transaction_hashes = vec![];
    for _ in 0..l1_transaction_hashes_len {
        l1_transaction_hashes.push(TransactionHash::random(&mut rng));
    }
    let BlockHeaderWithoutHash {
        block_number: _,
        parent_hash,
        timestamp,
        state_root,
        l1_gas_price,
        l1_data_gas_price,
        l2_gas_price,
        l2_gas_consumed,
        next_l2_gas_price,
        sequencer,
        l1_da_mode,
        starknet_version,
    }
    // TODO(Arni): Make sure the function get_test_instance correctly
    // creates a Some starknet version.
        = BlockHeaderWithoutHash::get_test_instance(&mut rng);
    let block_header_without_hash = BlockHeaderWithoutHash {
        block_number,
        parent_hash,
        timestamp,
        state_root,
        l1_gas_price,
        l1_data_gas_price,
        l2_gas_price,
        l2_gas_consumed,
        next_l2_gas_price,
        sequencer,
        l1_da_mode,
        starknet_version,
    };
    SyncBlock {
        state_diff,
        account_transaction_hashes,
        l1_transaction_hashes,
        block_header_without_hash,
    }
}
