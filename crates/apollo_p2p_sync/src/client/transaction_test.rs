use std::cmp::min;
use std::collections::HashMap;

use apollo_protobuf::sync::{BlockHashOrNumber, DataOrFin, Direction, Query};
use apollo_storage::body::BodyStorageReader;
use apollo_test_utils::{get_rng, get_test_body};
use futures::FutureExt;
use starknet_api::block::{BlockBody, BlockNumber};
use starknet_api::transaction::{FullTransaction, TransactionHash};

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
async fn transaction_basic_flow() {
    const NUM_BLOCKS: u64 = 5;
    const TRANSACTION_QUERY_LENGTH: u64 = 2;

    let mut rng = get_rng();

    let block_bodies = (0..NUM_BLOCKS)
        // TODO(shahak): remove Some(0) once we separate events from transactions correctly.
        .map(|i| {
            let mut body = get_test_body(i.try_into().unwrap(), Some(0), None, None);
            // get_test_body returns transaction hash in the range 0..num_transactions. We want to
            // avoid collisions in transaction hash.
            for transaction_hash in &mut body.transaction_hashes {
                *transaction_hash = TransactionHash(transaction_hash.0 + NUM_BLOCKS * i);
            }
            body
        })
        .collect::<Vec<_>>();

    let mut actions = vec![
        Action::RunP2pSync,
        // We already validate the header query content in other tests.
        Action::ReceiveQuery(Box::new(|_query| ()), DataType::Header),
    ];

    // Sleep so transaction sync will reach the sleep waiting for header protocol to receive new
    // data.
    actions.push(Action::SleepToLetSyncAdvance);
    // Send headers with corresponding transaction length
    for (i, block_body) in block_bodies.iter().enumerate() {
        actions.push(Action::SendHeader(DataOrFin(Some(random_header(
            &mut rng,
            BlockNumber(i.try_into().unwrap()),
            None,
            Some(block_body.transactions.len()),
        )))));
    }
    actions.push(Action::SendHeader(DataOrFin(None)));

    let len = block_bodies.len();
    // Wait for header sync to finish before continuing transaction sync.
    actions.push(Action::CheckStorage(Box::new(move |reader| {
        async move {
            let block_number = BlockNumber(len.try_into().unwrap());
            wait_for_marker(
                DataType::Header,
                &reader,
                block_number,
                SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                TIMEOUT_FOR_TEST,
            )
            .await;
        }
        .boxed()
    })));
    actions.push(Action::SimulateWaitPeriodForOtherProtocol);

    // Send transactions for each block and then validate they were written
    for (i, BlockBody { transactions, transaction_outputs, transaction_hashes }) in
        block_bodies.into_iter().enumerate()
    {
        let i = u64::try_from(i).unwrap();
        // If this block starts a new transaction query, receive the new query.
        if i.is_multiple_of(TRANSACTION_QUERY_LENGTH) {
            let limit = min(TRANSACTION_QUERY_LENGTH, NUM_BLOCKS - i);
            actions.push(Action::ReceiveQuery(
                Box::new(move |query| {
                    assert_eq!(
                        query,
                        Query {
                            start_block: BlockHashOrNumber::Number(BlockNumber(i)),
                            direction: Direction::Forward,
                            limit,
                            step: 1,
                        }
                    )
                }),
                DataType::Transaction,
            ));
        }

        for (transaction, (transaction_output, transaction_hash)) in transactions
            .iter()
            .cloned()
            .zip(transaction_outputs.iter().cloned().zip(transaction_hashes.iter().cloned()))
        {
            // Check that before the last transaction was sent, the transactions aren't written.
            actions.push(Action::CheckStorage(Box::new(move |reader| {
                async move {
                    assert_eq!(i, reader.begin_ro_txn().unwrap().get_body_marker().unwrap().0);
                }
                .boxed()
            })));

            actions.push(Action::SendTransaction(DataOrFin(Some(FullTransaction {
                transaction,
                transaction_output,
                transaction_hash,
            }))));
        }

        // Check that a block's transactions are written before the entire query finished.
        actions.push(Action::CheckStorage(Box::new(move |reader| {
            async move {
                let block_number = BlockNumber(i);
                wait_for_marker(
                    DataType::Transaction,
                    &reader,
                    block_number.unchecked_next(),
                    SLEEP_DURATION_TO_LET_SYNC_ADVANCE,
                    TIMEOUT_FOR_TEST,
                )
                .await;

                let txn = reader.begin_ro_txn().unwrap();
                let actual_transactions =
                    txn.get_block_transactions(block_number).unwrap().unwrap();
                // TODO(alonl): Uncomment this once we fix protobuf conversion for receipt
                // builtins.
                // let actual_transaction_outputs =
                //     txn.get_block_transaction_outputs(block_number).unwrap().unwrap();
                let actual_transaction_hashes =
                    txn.get_block_transaction_hashes(block_number).unwrap().unwrap();
                assert_eq!(actual_transactions, transactions);
                // TODO(alonl): Uncomment this once we fix protobuf conversion for receipt
                // builtins.
                // assert_eq!(actual_transaction_outputs, transaction_outputs);
                assert_eq!(actual_transaction_hashes, transaction_hashes);
            }
            .boxed()
        })));

        if (i + 1).is_multiple_of(TRANSACTION_QUERY_LENGTH) || i + 1 == NUM_BLOCKS {
            actions.push(Action::SendTransaction(DataOrFin(None)));
        }
    }

    run_test(
        HashMap::from([
            (DataType::Header, NUM_BLOCKS),
            (DataType::Transaction, TRANSACTION_QUERY_LENGTH),
        ]),
        None,
        actions,
    )
    .await;
}
