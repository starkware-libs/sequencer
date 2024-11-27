use futures::StreamExt;
use papyrus_protobuf::sync::{
    BlockHashOrNumber,
    DataOrFin,
    Direction,
    HeaderQuery,
    Query,
    SignedBlockHeader,
};
use papyrus_storage::header::HeaderStorageReader;
use starknet_api::block::{BlockHeader, BlockHeaderWithoutHash, BlockNumber};
use tokio::time::timeout;

use super::test_utils::{
    create_block_hashes_and_signatures,
    setup,
    wait_for_marker,
    MarkerKind,
    TestArgs,
    HEADER_QUERY_LENGTH,
    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
    TIMEOUT_FOR_NEW_QUERY_AFTER_PARTIAL_RESPONSE,
    TIMEOUT_FOR_TEST,
    WAIT_PERIOD_FOR_NEW_DATA,
};

#[tokio::test]
async fn signed_headers_basic_flow() {
    const NUM_QUERIES: u64 = 3;

    let TestArgs {
        p2p_sync,
        storage_reader,
        mut mock_header_response_manager,
        // The test will fail if we drop these
        mock_state_diff_response_manager: _mock_state_diff_response_manager,
        mock_transaction_response_manager: _mock_transaction_response_manager,
        mock_class_response_manager: _mock_class_response_manager,
        ..
    } = setup();
    let block_hashes_and_signatures =
        create_block_hashes_and_signatures((NUM_QUERIES * HEADER_QUERY_LENGTH).try_into().unwrap());

    // Create a future that will receive queries, send responses and validate the results.
    let parse_queries_future = async move {
        for query_index in 0..NUM_QUERIES {
            let start_block_number = query_index * HEADER_QUERY_LENGTH;
            let end_block_number = (query_index + 1) * HEADER_QUERY_LENGTH;

            // Receive query and validate it.
            let mut mock_header_responses_manager =
                mock_header_response_manager.next().await.unwrap();
            assert_eq!(
                *mock_header_responses_manager.query(),
                Ok(HeaderQuery(Query {
                    start_block: BlockHashOrNumber::Number(BlockNumber(start_block_number)),
                    direction: Direction::Forward,
                    limit: HEADER_QUERY_LENGTH,
                    step: 1,
                }))
            );

            for (i, (block_hash, block_signature)) in block_hashes_and_signatures
                .iter()
                .enumerate()
                .take(end_block_number.try_into().expect("Failed converting u64 to usize"))
                .skip(start_block_number.try_into().expect("Failed converting u64 to usize"))
            {
                // Send responses
                mock_header_responses_manager
                    .send_response(DataOrFin(Some(SignedBlockHeader {
                        block_header: BlockHeader {
                            block_hash: *block_hash,
                            block_header_without_hash: BlockHeaderWithoutHash {
                                block_number: BlockNumber(i.try_into().unwrap()),
                                ..Default::default()
                            },
                            state_diff_length: Some(0),
                            ..Default::default()
                        },
                        signatures: vec![*block_signature],
                    })))
                    .await
                    .unwrap();

                // Check responses were written to the storage. This way we make sure that the sync
                // writes to the storage each response it receives before all query responses were
                // sent.
                let block_number = BlockNumber(i.try_into().unwrap());
                wait_for_marker(
                    MarkerKind::Header,
                    &storage_reader,
                    block_number.unchecked_next(),
                    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                    TIMEOUT_FOR_TEST,
                )
                .await;

                let txn = storage_reader.begin_ro_txn().unwrap();
                let block_header = txn.get_block_header(block_number).unwrap().unwrap();
                assert_eq!(block_number, block_header.block_header_without_hash.block_number);
                assert_eq!(*block_hash, block_header.block_hash);
                let actual_block_signature =
                    txn.get_block_signature(block_number).unwrap().unwrap();
                assert_eq!(*block_signature, actual_block_signature);
            }
            mock_header_responses_manager.send_response(DataOrFin(None)).await.unwrap();
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

#[tokio::test]
async fn sync_sends_new_header_query_if_it_got_partial_responses() {
    const NUM_ACTUAL_RESPONSES: u8 = 2;
    assert!(u64::from(NUM_ACTUAL_RESPONSES) < HEADER_QUERY_LENGTH);

    let TestArgs {
        p2p_sync,
        mut mock_header_response_manager,
        // The test will fail if we drop these
        mock_state_diff_response_manager: _state_diff_receiver,
        mock_transaction_response_manager: _transaction_receiver,
        mock_class_response_manager: _class_receiver,
        ..
    } = setup();
    let block_hashes_and_signatures = create_block_hashes_and_signatures(NUM_ACTUAL_RESPONSES);

    // Create a future that will receive a query, send partial responses and receive the next query.
    let parse_queries_future = async move {
        let mut mock_header_responses_manager = mock_header_response_manager.next().await.unwrap();

        for (i, (block_hash, signature)) in block_hashes_and_signatures.into_iter().enumerate() {
            mock_header_responses_manager
                .send_response(DataOrFin(Some(SignedBlockHeader {
                    block_header: BlockHeader {
                        block_hash,
                        block_header_without_hash: BlockHeaderWithoutHash {
                            block_number: BlockNumber(i.try_into().unwrap()),
                            ..Default::default()
                        },
                        state_diff_length: Some(0),
                        ..Default::default()
                    },
                    signatures: vec![signature],
                })))
                .await
                .unwrap();
        }
        mock_header_responses_manager.send_response(DataOrFin(None)).await.unwrap();

        // Wait for the sync to enter sleep due to partial responses. Then, simulate time has
        // passed.
        tokio::time::sleep(SLEEP_DURATION_TO_LET_SYNC_ADVANCE).await;
        tokio::time::pause();
        tokio::time::advance(WAIT_PERIOD_FOR_NEW_DATA).await;
        tokio::time::resume();

        // First unwrap is for the timeout. Second unwrap is for the Option returned from Stream.
        let mock_header_responses_manager = timeout(
            TIMEOUT_FOR_NEW_QUERY_AFTER_PARTIAL_RESPONSE,
            mock_header_response_manager.next(),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(
            *mock_header_responses_manager.query(),
            Ok(HeaderQuery(Query {
                start_block: BlockHashOrNumber::Number(BlockNumber(NUM_ACTUAL_RESPONSES.into())),
                direction: Direction::Forward,
                limit: HEADER_QUERY_LENGTH,
                step: 1,
            }))
        );
    };

    tokio::select! {
        sync_result = p2p_sync.run() => {
            sync_result.unwrap();
            panic!("P2P sync aborted with no failure.");
        }
        _ = parse_queries_future => {}
    }
}

// TODO(shahak): Add negative tests.
