use std::vec;

use apollo_test_utils::get_test_block;
use assert_matches::assert_matches;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::transaction::{
    Event,
    EventContent,
    EventData,
    EventIndexInTransactionOutput,
    TransactionOffsetInBlock,
};

use crate::body::events::{get_events_from_tx, EventIndex, EventsReader};
use crate::body::{BodyStorageWriter, TransactionIndex};
use crate::db::table_types::Table;
use crate::header::HeaderStorageWriter;
use crate::test_utils::get_test_storage;

#[test]
fn iter_events_by_key() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let ca1: ContractAddress = 1u32.into();
    let ca2: ContractAddress = 2u32.into();
    let from_addresses = vec![ca1, ca2];
    let (block, _block_events) = get_test_block(4, None, None, None);
    let block_number = block.header.block_header_without_hash.block_number;
    let events_per_tx = generate_test_events(4, 3, &from_addresses);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block_number, &block.header)
        .unwrap()
        .append_body(block_number, block.body.clone())
        .unwrap()
        .append_events(block_number, &events_per_tx)
        .unwrap()
        .commit()
        .unwrap();

    let txn = storage_reader.begin_ro_txn().unwrap();

    // Verify event index entries were written to the events table.
    for (tx_i, events) in events_per_tx.iter().enumerate() {
        let transaction_index = TransactionIndex(block_number, TransactionOffsetInBlock(tx_i));
        for event in events {
            assert_matches!(txn.has_event(event.from_address, transaction_index), Ok(Some(())));
        }
    }
}

#[test]
fn iter_events_by_index() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let (block, _block_events) = get_test_block(2, None, None, None);
    let block_number = block.header.block_header_without_hash.block_number;
    let ca1: ContractAddress = 1u32.into();
    let events_per_tx = generate_test_events(2, 5, &[ca1]);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block_number, &block.header)
        .unwrap()
        .append_body(block_number, block.body.clone())
        .unwrap()
        .append_events(block_number, &events_per_tx)
        .unwrap()
        .commit()
        .unwrap();

    let txn = storage_reader.begin_ro_txn().unwrap();

    // Verify event index entries were written.
    for (tx_i, events) in events_per_tx.iter().enumerate() {
        let transaction_index = TransactionIndex(block_number, TransactionOffsetInBlock(tx_i));
        for event in events {
            assert_matches!(txn.has_event(event.from_address, transaction_index), Ok(Some(())));
        }
    }
}

#[test]
fn revert_events() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let (block, _block_events) = get_test_block(2, None, None, None);
    let block_number = block.header.block_header_without_hash.block_number;
    let ca1: ContractAddress = 1u32.into();
    let ca2: ContractAddress = 2u32.into();
    let events_per_tx = generate_test_events(2, 5, &[ca1, ca2]);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block_number, &block.header)
        .unwrap()
        .append_body(block_number, block.body.clone())
        .unwrap()
        .append_events(block_number, &events_per_tx)
        .unwrap()
        .commit()
        .unwrap();

    // Verify events were written to the events table.
    let txn = storage_reader.begin_ro_txn().unwrap();
    let contract_address_events_index =
        txn.txn.open_table(&txn.tables.contract_address_events_index).unwrap();
    for (tx_idx, events) in events_per_tx.iter().enumerate() {
        let transaction_index = TransactionIndex(block_number, TransactionOffsetInBlock(tx_idx));
        for event in events {
            assert_matches!(
                contract_address_events_index
                    .get(&txn.txn, &(event.from_address, transaction_index)),
                Ok(Some(_))
            );
        }
    }
    drop(txn);

    storage_writer
        .begin_rw_txn()
        .unwrap()
        .revert_header(block_number)
        .unwrap()
        .0
        .revert_events(block_number)
        .unwrap()
        .revert_body(block_number)
        .unwrap()
        .0
        .commit()
        .unwrap();

    // Verify events were deleted from the events table.
    let txn = storage_reader.begin_ro_txn().unwrap();
    let contract_address_events_index =
        txn.txn.open_table(&txn.tables.contract_address_events_index).unwrap();
    for (tx_idx, events) in events_per_tx.iter().enumerate() {
        let transaction_index = TransactionIndex(block_number, TransactionOffsetInBlock(tx_idx));
        for event in events {
            assert_matches!(
                contract_address_events_index
                    .get(&txn.txn, &(event.from_address, transaction_index)),
                Ok(None)
            );
        }
    }
}

#[test]
fn get_events_from_tx_test() {
    let tx_index = TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(0));
    let ca1 = 1u32.into();
    let ca2 = 2u32.into();

    let e1 = Event {
        from_address: ca1,
        content: EventContent { data: EventData(vec![1u32.into()]), ..Default::default() },
    };
    let e2 = Event {
        from_address: ca2,
        content: EventContent { data: EventData(vec![1u32.into()]), ..Default::default() },
    };
    let e3 = Event {
        from_address: ca1,
        content: EventContent { data: EventData(vec![2u32.into()]), ..Default::default() },
    };

    let events = vec![e1.clone(), e2.clone(), e3.clone()];
    let e1_output =
        ((ca1, EventIndex(tx_index, EventIndexInTransactionOutput(0))), e1.content.clone());
    let e2_output =
        ((ca2, EventIndex(tx_index, EventIndexInTransactionOutput(1))), e2.content.clone());
    let e3_output =
        ((ca1, EventIndex(tx_index, EventIndexInTransactionOutput(2))), e3.content.clone());

    // All events.
    assert_eq!(
        get_events_from_tx(events.clone(), tx_index, ca1, 0),
        vec![e1_output.clone(), e3_output.clone()]
    );
    assert_eq!(get_events_from_tx(events.clone(), tx_index, ca2, 0), vec![e2_output.clone()]);

    // All events of starting from the second event.
    assert_eq!(get_events_from_tx(events.clone(), tx_index, ca1, 1), vec![e3_output.clone()]);
    assert_eq!(get_events_from_tx(events.clone(), tx_index, ca2, 1), vec![e2_output.clone()]);

    // All events of starting from the third event.
    assert_eq!(get_events_from_tx(events.clone(), tx_index, ca1, 2), vec![e3_output.clone()]);
    assert_eq!(get_events_from_tx(events.clone(), tx_index, ca2, 2), vec![]);

    // All events of starting from the not existing index.
    assert_eq!(get_events_from_tx(events.clone(), tx_index, ca1, 3), vec![]);
    assert_eq!(get_events_from_tx(events.clone(), tx_index, ca2, 3), vec![]);
}

#[test]
fn get_transaction_events_test() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let (block, _block_events) = get_test_block(3, None, None, None);
    let block_number = block.header.block_header_without_hash.block_number;
    let ca1: ContractAddress = 1u32.into();
    let events_per_tx = generate_test_events(3, 2, &[ca1]);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block_number, &block.header)
        .unwrap()
        .append_body(block_number, block.body.clone())
        .unwrap()
        .append_events(block_number, &events_per_tx)
        .unwrap()
        .commit()
        .unwrap();

    let txn = storage_reader.begin_ro_txn().unwrap();

    // Each transaction returns exactly the events that were stored.
    for (tx_i, expected_events) in events_per_tx.iter().enumerate() {
        let transaction_index = TransactionIndex(block_number, TransactionOffsetInBlock(tx_i));
        let actual_events = txn.get_transaction_events(transaction_index).unwrap().unwrap();
        assert_eq!(&actual_events, expected_events);
    }

    // Non-existent block returns None.
    let missing_block_index = TransactionIndex(BlockNumber(999), TransactionOffsetInBlock(0));
    assert_eq!(txn.get_transaction_events(missing_block_index).unwrap(), None);

    // Existing block but out-of-range transaction offset returns None.
    let missing_tx_index = TransactionIndex(block_number, TransactionOffsetInBlock(999));
    assert_eq!(txn.get_transaction_events(missing_tx_index).unwrap(), None);
}

#[test]
fn get_block_events_per_transaction_test() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let (block, _block_events) = get_test_block(3, None, None, None);
    let block_number = block.header.block_header_without_hash.block_number;
    let ca1: ContractAddress = 1u32.into();
    let ca2: ContractAddress = 2u32.into();
    let events_per_tx = generate_test_events(3, 4, &[ca1, ca2]);
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block_number, &block.header)
        .unwrap()
        .append_body(block_number, block.body.clone())
        .unwrap()
        .append_events(block_number, &events_per_tx)
        .unwrap()
        .commit()
        .unwrap();

    let txn = storage_reader.begin_ro_txn().unwrap();

    // Returns all events grouped by transaction.
    let block_events = txn.get_block_events_per_transaction(block_number).unwrap().unwrap();
    assert_eq!(block_events, events_per_tx);

    // Non-existent block returns None.
    assert_eq!(txn.get_block_events_per_transaction(BlockNumber(999)).unwrap(), None);
}

#[test]
fn get_block_events_per_transaction_empty_block() {
    let ((storage_reader, mut storage_writer), _temp_dir) = get_test_storage();
    let (block, _block_events) = get_test_block(2, None, None, None);
    let block_number = block.header.block_header_without_hash.block_number;
    let empty_events: Vec<Vec<Event>> = vec![vec![], vec![]];
    storage_writer
        .begin_rw_txn()
        .unwrap()
        .append_header(block_number, &block.header)
        .unwrap()
        .append_body(block_number, block.body.clone())
        .unwrap()
        .append_events(block_number, &empty_events)
        .unwrap()
        .commit()
        .unwrap();

    let txn = storage_reader.begin_ro_txn().unwrap();

    // Transactions with no events return empty vecs.
    let block_events = txn.get_block_events_per_transaction(block_number).unwrap().unwrap();
    assert_eq!(block_events, empty_events);

    // Individual transaction getters also return empty.
    for tx_i in 0..2 {
        let transaction_index = TransactionIndex(block_number, TransactionOffsetInBlock(tx_i));
        let events = txn.get_transaction_events(transaction_index).unwrap().unwrap();
        assert!(events.is_empty());
    }
}

/// Helper to generate a flat list of events per transaction for testing.
fn generate_test_events(
    num_transactions: usize,
    events_per_tx: usize,
    from_addresses: &[ContractAddress],
) -> Vec<Vec<Event>> {
    (0..num_transactions)
        .map(|tx_i| {
            (0..events_per_tx)
                .map(|event_i| Event {
                    from_address: from_addresses[event_i % from_addresses.len()],
                    content: EventContent {
                        data: EventData(vec![(tx_i * events_per_tx + event_i).into()]),
                        ..Default::default()
                    },
                })
                .collect()
        })
        .collect()
}
