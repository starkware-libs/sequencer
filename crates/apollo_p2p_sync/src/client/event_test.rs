use std::cmp::min;
use std::collections::HashMap;

use apollo_protobuf::sync::{BlockHashOrNumber, DataOrFin, Direction, Query};
use apollo_storage::body::events::EventsReader;
use apollo_test_utils::{get_rng, get_test_body};
use futures::FutureExt;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::{Event, FullTransaction, TransactionHash};

use super::test_utils::{
    random_header,
    run_test,
    wait_for_marker,
    Action,
    DataType,
    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
    TIMEOUT_FOR_TEST,
};

#[tokio::test]
async fn event_basic_flow() {
    const NUM_BLOCKS: u64 = 3;
    const EVENTS_PER_TX: usize = 2;
    const NUM_TRANSACTIONS_PER_BLOCK: usize = 2;
    // All transactions fit in a single query.
    const TRANSACTION_QUERY_LENGTH: u64 = NUM_BLOCKS;
    const EVENT_QUERY_LENGTH: u64 = 2;

    let mut rng = get_rng();

    // Collect (body, events_per_tx) for each block.
    let block_data: Vec<_> = (0..NUM_BLOCKS)
        .map(|block_index| {
            let (mut body, events_per_tx) =
                get_test_body(NUM_TRANSACTIONS_PER_BLOCK, Some(EVENTS_PER_TX), None, None);
            // Offset transaction hashes to avoid collisions across blocks.
            for tx_hash in &mut body.transaction_hashes {
                *tx_hash = TransactionHash(tx_hash.0 + NUM_BLOCKS * block_index);
            }
            (body, events_per_tx)
        })
        .collect();

    let mut actions = vec![
        Action::RunP2pSync,
        // Receive but don't validate the header query — covered in header_test.
        Action::ReceiveQuery(Box::new(|_query| ()), DataType::Header),
    ];

    // Let other sync protocols advance so they wait for header data.
    actions.push(Action::SleepToLetSyncAdvance);

    // Send headers. Each header carries n_events matching the events we'll send.
    for (block_index, (body, events_per_tx)) in block_data.iter().enumerate() {
        let num_events: usize = events_per_tx.iter().map(|events| events.len()).sum();
        let mut header = random_header(
            &mut rng,
            BlockNumber(block_index as u64),
            None,
            Some(body.transactions.len()),
        );
        header.block_header.n_events = num_events;
        actions.push(Action::SendHeader(DataOrFin(Some(header))));
    }
    actions.push(Action::SendHeader(DataOrFin(None)));

    // Wait for all headers to be stored.
    actions.push(Action::CheckStorage(Box::new(move |reader| {
        async move {
            wait_for_marker(
                DataType::Header,
                &reader,
                BlockNumber(NUM_BLOCKS),
                SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                TIMEOUT_FOR_TEST,
            )
            .await;
        }
        .boxed()
    })));
    actions.push(Action::SimulateWaitPeriodForOtherProtocol);

    // Send all transactions in a single query to advance the body marker.
    actions.push(Action::ReceiveQuery(
        Box::new(|query| {
            assert_eq!(
                query,
                Query {
                    start_block: BlockHashOrNumber::Number(BlockNumber(0)),
                    direction: Direction::Forward,
                    limit: NUM_BLOCKS,
                    step: 1,
                }
            );
        }),
        DataType::Transaction,
    ));
    for (body, _events_per_tx) in &block_data {
        for (transaction, (transaction_output, transaction_hash)) in
            body.transactions.iter().cloned().zip(
                body.transaction_outputs
                    .iter()
                    .cloned()
                    .zip(body.transaction_hashes.iter().cloned()),
            )
        {
            actions.push(Action::SendTransaction(DataOrFin(Some(FullTransaction {
                transaction,
                transaction_output,
                transaction_hash,
            }))));
        }
    }
    actions.push(Action::SendTransaction(DataOrFin(None)));

    // Wait for all bodies to be stored.
    actions.push(Action::CheckStorage(Box::new(move |reader| {
        async move {
            wait_for_marker(
                DataType::Transaction,
                &reader,
                BlockNumber(NUM_BLOCKS),
                SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                TIMEOUT_FOR_TEST,
            )
            .await;
        }
        .boxed()
    })));
    actions.push(Action::SimulateWaitPeriodForOtherProtocol);

    // Send events block by block, checking storage after each block is complete.
    for (block_index, (body, events_per_tx)) in block_data.into_iter().enumerate() {
        let block_index = block_index as u64;

        // Receive a new event query at each query boundary.
        if block_index.is_multiple_of(EVENT_QUERY_LENGTH) {
            let limit = min(EVENT_QUERY_LENGTH, NUM_BLOCKS - block_index);
            actions.push(Action::ReceiveQuery(
                Box::new(move |query| {
                    assert_eq!(
                        query,
                        Query {
                            start_block: BlockHashOrNumber::Number(BlockNumber(block_index)),
                            direction: Direction::Forward,
                            limit,
                            step: 1,
                        }
                    );
                }),
                DataType::Event,
            ));
        }

        // Send each event paired with its transaction hash.
        for (tx_hash, events) in body.transaction_hashes.iter().zip(events_per_tx.iter()) {
            for event in events {
                actions.push(Action::SendEvent(DataOrFin(Some((event.clone(), *tx_hash)))));
            }
        }

        // After the last block in a query (or the last block overall), send Fin.
        if (block_index + 1).is_multiple_of(EVENT_QUERY_LENGTH) || block_index + 1 == NUM_BLOCKS {
            actions.push(Action::SendEvent(DataOrFin(None)));
        }

        // Verify events are stored correctly after the event marker advances.
        let expected_events_per_tx: Vec<Vec<Event>> = events_per_tx;
        actions.push(Action::CheckStorage(Box::new(move |reader| {
            async move {
                wait_for_marker(
                    DataType::Event,
                    &reader,
                    BlockNumber(block_index + 1),
                    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                    TIMEOUT_FOR_TEST,
                )
                .await;
                let actual_events_per_tx = reader
                    .begin_ro_txn()
                    .unwrap()
                    .get_block_events_per_transaction(BlockNumber(block_index))
                    .unwrap()
                    .unwrap();
                assert_eq!(actual_events_per_tx, expected_events_per_tx);
            }
            .boxed()
        })));
    }

    run_test(
        HashMap::from([
            (DataType::Header, NUM_BLOCKS),
            (DataType::Transaction, TRANSACTION_QUERY_LENGTH),
            (DataType::Event, EVENT_QUERY_LENGTH),
        ]),
        None,
        actions,
    )
    .await;
}
