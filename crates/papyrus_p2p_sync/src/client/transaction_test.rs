use std::cmp::min;

use futures::{FutureExt, StreamExt};
use papyrus_protobuf::sync::{
    BlockHashOrNumber,
    DataOrFin,
    Direction,
    Query,
    SignedBlockHeader,
    TransactionQuery,
};
use papyrus_storage::body::BodyStorageReader;
use papyrus_test_utils::get_test_body;
use starknet_api::block::{BlockBody, BlockHeader, BlockHeaderWithoutHash, BlockNumber};
use starknet_api::transaction::FullTransaction;

use super::test_utils::{
    create_block_hashes_and_signatures,
    setup,
    TestArgs,
    HEADER_QUERY_LENGTH,
    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
    TRANSACTION_QUERY_LENGTH,
    WAIT_PERIOD_FOR_NEW_DATA,
};
use crate::client::test_utils::{wait_for_marker, MarkerKind, TIMEOUT_FOR_TEST};

#[tokio::test]
async fn transaction_basic_flow() {
    let TestArgs {
        p2p_sync,
        storage_reader,
        mut mock_header_response_manager,
        mut mock_transaction_response_manager,
        // The test will fail if we drop these
        mock_state_diff_response_manager: _mock_state_diff_response_manager,
        mock_class_response_manager: _mock_class_responses_manager,
        ..
    } = setup();

    const NUM_TRANSACTIONS_PER_BLOCK: u64 = 6;
    let block_hashes_and_signatures =
        create_block_hashes_and_signatures(HEADER_QUERY_LENGTH.try_into().unwrap());
    let BlockBody { transactions, transaction_outputs, transaction_hashes } = get_test_body(
        (NUM_TRANSACTIONS_PER_BLOCK * HEADER_QUERY_LENGTH).try_into().unwrap(),
        None,
        None,
        None,
    );

    // Create a future that will receive queries, send responses and validate the results.
    let parse_queries_future = async move {
        // We wait for the state diff sync to see that there are no headers and start sleeping
        tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;

        // Check that before we send headers there is no state diff query.
        assert!(mock_transaction_response_manager.next().now_or_never().is_none());
        let mut mock_header_responses_manager = mock_header_response_manager.next().await.unwrap();

        // Send headers for entire query.
        for (i, (block_hash, block_signature)) in block_hashes_and_signatures.iter().enumerate() {
            // Send responses
            mock_header_responses_manager
                .send_response(DataOrFin(Some(SignedBlockHeader {
                    block_header: BlockHeader {
                        block_hash: *block_hash,
                        block_header_without_hash: BlockHeaderWithoutHash {
                            block_number: BlockNumber(i.try_into().unwrap()),
                            ..Default::default()
                        },
                        n_transactions: NUM_TRANSACTIONS_PER_BLOCK.try_into().unwrap(),
                        state_diff_length: Some(0),
                        ..Default::default()
                    },
                    signatures: vec![*block_signature],
                })))
                .await
                .unwrap();
        }

        wait_for_marker(
            MarkerKind::Header,
            &storage_reader,
            BlockNumber(HEADER_QUERY_LENGTH),
            SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
            TIMEOUT_FOR_TEST,
        )
        .await;

        // Simulate time has passed so that state diff sync will resend query after it waited for
        // new header
        tokio::time::pause();
        tokio::time::advance(WAIT_PERIOD_FOR_NEW_DATA).await;
        tokio::time::resume();

        let num_transaction_queries = HEADER_QUERY_LENGTH.div_ceil(TRANSACTION_QUERY_LENGTH);
        for transaction_querie in 0..num_transaction_queries {
            let start_block_number = transaction_querie * TRANSACTION_QUERY_LENGTH;
            let num_blocks_in_querie =
                min(TRANSACTION_QUERY_LENGTH, HEADER_QUERY_LENGTH - start_block_number);

            // Receive query and validate it.
            let mut mock_transaction_responses_manager =
                mock_transaction_response_manager.next().await.unwrap();
            assert_eq!(
                *mock_transaction_responses_manager.query(),
                Ok(TransactionQuery(Query {
                    start_block: BlockHashOrNumber::Number(BlockNumber(start_block_number)),
                    direction: Direction::Forward,
                    limit: num_blocks_in_querie,
                    step: 1,
                })),
                "If the limit of the query is too low, try to increase \
                 SLEEP_DURATION_TO_LET_SYNC_ADVANCE",
            );

            for block_number in start_block_number..(start_block_number + num_blocks_in_querie) {
                let start_transaction_number = block_number * NUM_TRANSACTIONS_PER_BLOCK;
                for transaction_number in start_transaction_number
                    ..(start_transaction_number + NUM_TRANSACTIONS_PER_BLOCK)
                {
                    let transaction_idx = usize::try_from(transaction_number).unwrap();
                    let transaction = transactions[transaction_idx].clone();
                    let transaction_output = transaction_outputs[transaction_idx].clone();
                    let transaction_hash = transaction_hashes[transaction_idx];

                    mock_transaction_responses_manager
                        .send_response(DataOrFin(Some(FullTransaction {
                            transaction,
                            transaction_output,
                            transaction_hash,
                        })))
                        .await
                        .unwrap();
                }

                // Check responses were written to the storage. This way we make sure that the sync
                // writes to the storage each response it receives before all query responses were
                // sent.
                let block_number = BlockNumber(block_number);
                wait_for_marker(
                    MarkerKind::Body,
                    &storage_reader,
                    block_number.unchecked_next(),
                    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                    TIMEOUT_FOR_TEST,
                )
                .await;

                let txn = storage_reader.begin_ro_txn().unwrap();

                // TODO: Verify that the transaction outputs are equal aswell. currently is buggy.
                let storage_transactions =
                    txn.get_block_transactions(block_number).unwrap().unwrap();
                let storage_transaction_hashes =
                    txn.get_block_transaction_hashes(block_number).unwrap().unwrap();
                for i in 0..NUM_TRANSACTIONS_PER_BLOCK {
                    let idx: usize = usize::try_from(i + start_transaction_number).unwrap();
                    assert_eq!(
                        storage_transactions[usize::try_from(i).unwrap()],
                        transactions[idx]
                    );
                    assert_eq!(
                        storage_transaction_hashes[usize::try_from(i).unwrap()],
                        transaction_hashes[idx]
                    );
                }
            }

            mock_transaction_responses_manager.send_response(DataOrFin(None)).await.unwrap();
        }
    };

    tokio::select! {
        sync_result = p2p_sync.run() => {
            sync_result.unwrap();
            panic!("P2P sync aborted with no failure.");
        }
        _ = parse_queries_future => {}
    }
}
