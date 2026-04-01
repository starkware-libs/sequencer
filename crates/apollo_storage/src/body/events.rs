//! Interface for iterating over events from the storage.
//!
//! Events are stored in the `transaction_events` table, keyed by [`TransactionIndex`].
//!
//! Import [`EventsReader`] to iterate over events using a read-only [`StorageTxn`].
//!
//! # Example
//! ```
//! use apollo_storage::open_storage;
//! use apollo_storage::body::TransactionIndex;
//! use apollo_storage::body::events::{EventIndex, EventsReader};
//! # use apollo_storage::{db::DbConfig, StorageConfig};
//! # use starknet_api::core::ChainId;
//! # use starknet_api::block::BlockNumber;
//! use starknet_api::core::ContractAddress;
//! use starknet_api::transaction::TransactionOffsetInBlock;
//! use starknet_api::transaction::EventIndexInTransactionOutput;
//!
//! # let dir_handle = tempfile::tempdir().unwrap();
//! # let dir = dir_handle.path().to_path_buf();
//! # let db_config = DbConfig {
//! #     path_prefix: dir,
//! #     chain_id: ChainId::Mainnet,
//! #     enforce_file_exists: false,
//! #     min_size: 1 << 20,    // 1MB
//! #     max_size: 1 << 35,    // 32GB
//! #     growth_step: 1 << 26, // 64MB
//! #     max_readers: 1 << 13, // 8K readers
//! # };
//! # let storage_config = StorageConfig{db_config, ..Default::default()};
//! // The API allows read-only interactions with the events. To write events, use the body writer.
//! let (reader, mut writer) = open_storage(storage_config)?;
//! // iterate events from all contracts, starting from the first event in the first transaction.
//! let event_index = EventIndex(
//!     TransactionIndex(BlockNumber(0), TransactionOffsetInBlock(0)),
//!     EventIndexInTransactionOutput(0),
//! );
//! let txn = reader.begin_ro_txn()?; // The transaction must live longer than the iterator.
//! let events_iterator = txn.iter_events(None, event_index, BlockNumber(0))?;
//! for ((contract_address, event_index), event_content) in events_iterator {
//!    // Do something with the event.
//! }
//! // iterate events from a specific contract.
//! let contract_events_iterator = txn.iter_events(Some(ContractAddress::default()), event_index, BlockNumber(0))?;
//! for ((contract_address, event_index), event_content) in contract_events_iterator {
//!    // Do something with the event.
//! }
//! # Ok::<(), apollo_storage::StorageError>(())
#[cfg(test)]
#[path = "events_test.rs"]
mod events_test;

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::transaction::{
    Event,
    EventContent,
    EventIndexInTransactionOutput,
    TransactionOffsetInBlock,
};

use crate::body::{
    BodyStorageReader,
    ContractAddressEventsIndexKey,
    TransactionEventsTable,
    TransactionIndex,
};
use crate::db::serialization::NoVersionValueWrapper;
use crate::db::table_types::{CommonPrefix, DbCursor, DbCursorTrait, NoValue, SimpleTable, Table};
use crate::db::{DbTransaction, RO};
use crate::mmap_file::LocationInFile;
use crate::{FileHandlers, StorageResult, StorageTxn};

/// An identifier of an event.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Deserialize, Serialize, PartialOrd, Ord)]
#[cfg_attr(any(test, feature = "testing"), derive(Hash))]
pub struct EventIndex(pub TransactionIndex, pub EventIndexInTransactionOutput);

/// An interface for reading events.
pub trait EventsReader<'txn, 'env> {
    /// Returns an iterator over events, which is a wrapper of two iterators.
    /// If the address is none it iterates the events by the order of the event index,
    /// else, it iterated the events by the order of the contract addresses.
    ///
    /// # Arguments
    /// * address - contract address to iterate over events was emitted by it.
    /// * event_index - event index to start iterate from it.
    /// * to_block_number - block number to stop iterate at it.
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was an error.
    fn iter_events(
        &'env self,
        address: Option<ContractAddress>,
        event_index: EventIndex,
        to_block_number: BlockNumber,
    ) -> StorageResult<EventIter<'txn, 'env>>;

    /// Checks if a contract address emitted any event in a specific transaction.
    ///
    /// # Arguments
    /// * `address` - The contract address to check.
    /// * `tx_index` - The transaction index to check.
    ///
    /// # Returns
    /// * `Ok(Some(()))` if the contract emitted at least one event in this transaction.
    /// * `Ok(None)` if no events were emitted by this contract in this transaction.
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was a database error.
    fn has_event(
        &self,
        address: ContractAddress,
        tx_index: TransactionIndex,
    ) -> StorageResult<Option<()>>;

    /// Returns the events emitted by the transaction at the given index.
    fn get_transaction_events(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<Vec<Event>>>;

    /// Returns the events for each transaction in the block with the given number.
    fn get_block_events_per_transaction(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<Vec<Event>>>>;
}

// TODO(DanB): support all read transactions (including RW).
impl<'txn, 'env> EventsReader<'txn, 'env> for StorageTxn<'env, RO> {
    fn iter_events(
        &'env self,
        optional_address: Option<ContractAddress>,
        event_index: EventIndex,
        to_block_number: BlockNumber,
    ) -> StorageResult<EventIter<'txn, 'env>> {
        self.verify_not_sequencer_mode("iter_events")?;
        if let Some(address) = optional_address {
            return Ok(EventIter::ByContractAddress(
                self.iter_events_by_contract_address((address, event_index))?,
            ));
        }

        Ok(EventIter::ByEventIndex(self.iter_events_by_event_index(event_index, to_block_number)?))
    }

    fn has_event(
        &self,
        address: ContractAddress,
        tx_index: TransactionIndex,
    ) -> StorageResult<Option<()>> {
        self.verify_not_sequencer_mode("has_event")?;
        let contract_address_events_index =
            self.open_table(&self.tables.contract_address_events_index)?;
        Ok(contract_address_events_index.get(&self.txn, &(address, tx_index))?.map(|_| ()))
    }

    fn get_transaction_events(
        &self,
        transaction_index: TransactionIndex,
    ) -> StorageResult<Option<Vec<Event>>> {
        self.verify_not_sequencer_mode("get_transaction_events")?;
        let transaction_events_table = self.open_table(&self.tables.transaction_events)?;
        let Some(location) = transaction_events_table.get(&self.txn, &transaction_index)? else {
            return Ok(None);
        };
        Ok(Some(self.file_handlers.get_events_unchecked(location)?))
    }

    fn get_block_events_per_transaction(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<Vec<Event>>>> {
        self.verify_not_sequencer_mode("get_block_events_per_transaction")?;
        if self.get_event_marker()? <= block_number {
            return Ok(None);
        }
        let transaction_events_table = self.open_table(&self.tables.transaction_events)?;
        let mut cursor = transaction_events_table.cursor(&self.txn)?;
        let mut current_entry =
            cursor.lower_bound(&TransactionIndex(block_number, TransactionOffsetInBlock(0)))?;
        let mut result = Vec::new();
        while let Some((TransactionIndex(current_block_number, _), location)) = current_entry {
            if current_block_number != block_number {
                break;
            }
            result.push(self.file_handlers.get_events_unchecked(location)?);
            current_entry = cursor.next()?;
        }
        Ok(Some(result))
    }
}

// TODO(dvir): add transaction hash to the return value. In the RPC when returning events this is
// with the transaction hash. We can do it efficiently here because we anyway read the relevant
// entry in the transaction_metadata table..
#[allow(missing_docs)]
/// A wrapper of two iterators [`EventIterByContractAddress`] and [`EventIterByEventIndex`].
pub enum EventIter<'txn, 'env> {
    ByContractAddress(EventIterByContractAddress<'env, 'txn>),
    ByEventIndex(EventIterByEventIndex<'txn>),
}

/// This iterator is a wrapper of two iterators [`EventIterByContractAddress`]
/// and [`EventIterByEventIndex`].
/// With this wrapper we can execute the same code, regardless the
/// type of iteration used.
impl Iterator for EventIter<'_, '_> {
    type Item = ((ContractAddress, EventIndex), EventContent);

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            EventIter::ByContractAddress(it) => it.next(),
            EventIter::ByEventIndex(it) => it.next(),
        }
        .unwrap_or(None)
    }
}

/// This iterator goes over the events in the order of the events table key.
/// That is, the events iterated first by the contract address and then by the event index.
pub struct EventIterByContractAddress<'env, 'txn> {
    txn: &'txn DbTransaction<'env, RO>,
    file_handlers: &'txn FileHandlers<RO>,
    // This value is the next entry in the events table to search for relevant events. If it is
    // None there are no more events.
    next_index_entry: Option<ContractAddressEventsIndexKey>,
    // Queue of events to return from the iterator. When this queue is empty, we need to fetch more
    // events.
    events_queue: VecDeque<((ContractAddress, EventIndex), EventContent)>,
    cursor: ContractAddressEventsIndexCursor<'txn>,
    transaction_events_table: TransactionEventsTable<'env>,
}

impl EventIterByContractAddress<'_, '_> {
    /// Returns the next event. If there are no more events, returns None.
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was an error.
    fn next(&mut self) -> StorageResult<Option<((ContractAddress, EventIndex), EventContent)>> {
        // Here we make sure that the events_queue is not empty. If it does we fill it with new
        // relevant events.
        if self.events_queue.is_empty() {
            let Some((contract_address, tx_index)) = self.next_index_entry.take() else {
                return Ok(None);
            };
            let location =
                self.transaction_events_table.get(self.txn, &tx_index)?.unwrap_or_else(|| {
                    panic!(
                        "Transaction events for {tx_index:?} not found, but entry exists in \
                         events table"
                    )
                });
            let events = self.file_handlers.get_events_unchecked(location)?;
            self.events_queue = get_events_from_tx(events, tx_index, contract_address, 0);
            self.next_index_entry = self.cursor.next()?.map(|(key, _)| key);
        }

        Ok(Some(self.events_queue.pop_front().expect("events_queue should not be empty.")))
    }
}

/// This iterator goes over the events in the order of the event index.
/// That is, the events are iterated by the order they are emitted.
/// First by the block number, then by the transaction offset in the block,
/// and finally, by the event index in the transaction output.
pub struct EventIterByEventIndex<'txn> {
    file_handlers: &'txn FileHandlers<RO>,
    current_transaction_events: Option<(TransactionIndex, Vec<Event>)>,
    events_cursor: TransactionEventsCursor<'txn>,
    current_event_offset: EventIndexInTransactionOutput,
    to_block_number: BlockNumber,
}

impl EventIterByEventIndex<'_> {
    /// Returns the next event. If there are no more events, returns None.
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was an error.
    fn next(&mut self) -> StorageResult<Option<((ContractAddress, EventIndex), EventContent)>> {
        let Some((tx_index, events)) = &self.current_transaction_events else {
            return Ok(None);
        };
        let Some(Event { from_address, content }) = events.get(self.current_event_offset.0) else {
            return Ok(None);
        };
        let key = (*from_address, EventIndex(*tx_index, self.current_event_offset));
        // TODO(dvir): don't clone here the event content.
        let content = content.clone();
        self.current_event_offset.0 += 1;
        self.advance_to_next_event()?;
        Ok(Some((key, content)))
    }

    /// Advances to the next event across transactions. If the current transaction has more events
    /// at `current_event_offset`, stays on it. Otherwise, advances the cursor until a transaction
    /// with events is found or the block boundary is reached.
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was an error.
    fn advance_to_next_event(&mut self) -> StorageResult<()> {
        while let Some((tx_index, events)) = &self.current_transaction_events {
            if tx_index.0 > self.to_block_number {
                self.current_transaction_events = None;
                break;
            }
            if events.len() > self.current_event_offset.0 {
                break;
            }

            // No more events in this transaction, advance to the next one.
            let Some((tx_index, location)) = self.events_cursor.next()? else {
                self.current_transaction_events = None;
                return Ok(());
            };
            let events = self.file_handlers.get_events_unchecked(location)?;
            self.current_transaction_events = Some((tx_index, events));
            self.current_event_offset = EventIndexInTransactionOutput(0);
        }

        Ok(())
    }
}

impl<'txn, 'env> StorageTxn<'env, RO>
where
    'env: 'txn,
{
    /// Returns an events iterator that iterates events by the events table key from the given key.
    ///
    /// # Arguments
    /// * key - key to start from the first event with a key greater or equals to the given key.
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was an error.
    fn iter_events_by_contract_address(
        &'env self,
        key: (ContractAddress, EventIndex),
    ) -> StorageResult<EventIterByContractAddress<'env, 'txn>> {
        let transaction_events_table = self.open_table(&self.tables.transaction_events)?;
        let contract_address_events_index =
            self.open_table(&self.tables.contract_address_events_index)?;
        let mut cursor = contract_address_events_index.cursor(&self.txn)?;
        let events_queue = if let Some((contract_address, tx_index)) =
            cursor.lower_bound(&(key.0, key.1.0))?.map(|(key, _)| key)
        {
            let location =
                transaction_events_table.get(&self.txn, &tx_index)?.unwrap_or_else(|| {
                    panic!(
                        "Transaction events for {tx_index:?} not found, but entry exists in \
                         events table"
                    )
                });
            let events = self.file_handlers.get_events_unchecked(location)?;

            // In case of we get tx_index different from the key, it means we need to start a new
            // transaction which means the first event.
            let start_event_index = if tx_index == key.1.0 { key.1.1.0 } else { 0 };
            get_events_from_tx(events, tx_index, contract_address, start_event_index)
        } else {
            VecDeque::new()
        };
        let next_index_entry = cursor.next()?.map(|(key, _)| key);

        Ok(EventIterByContractAddress {
            txn: &self.txn,
            file_handlers: &self.file_handlers,
            next_index_entry,
            events_queue,
            cursor,
            transaction_events_table,
        })
    }

    /// Returns an events iterator that iterates events by event index from the given event index.
    ///
    /// # Arguments
    /// * event_index - event index to start from the first event with an index greater or equals
    ///   to.
    /// * to_block_number - block number to stop iterate at it.
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was an error.
    fn iter_events_by_event_index(
        &'env self,
        event_index: EventIndex,
        to_block_number: BlockNumber,
    ) -> StorageResult<EventIterByEventIndex<'txn>> {
        let transaction_events_table = self.open_table(&self.tables.transaction_events)?;
        let mut events_cursor = transaction_events_table.cursor(&self.txn)?;
        let first_transaction_events = match events_cursor.lower_bound(&event_index.0)? {
            Some((tx_index, location)) => {
                Some((tx_index, self.file_handlers.get_events_unchecked(location)?))
            }
            None => None,
        };

        let mut it = EventIterByEventIndex {
            file_handlers: &self.file_handlers,
            current_transaction_events: first_transaction_events,
            events_cursor,
            current_event_offset: event_index.1,
            to_block_number,
        };
        it.advance_to_next_event()?;
        Ok(it)
    }
}

fn get_events_from_tx(
    events_list: Vec<Event>,
    tx_index: TransactionIndex,
    contract_address: ContractAddress,
    start_index: usize,
) -> VecDeque<((ContractAddress, EventIndex), EventContent)> {
    let mut events = VecDeque::new();
    for (i, event) in events_list.into_iter().enumerate().skip(start_index) {
        if event.from_address == contract_address {
            let key = (contract_address, EventIndex(tx_index, EventIndexInTransactionOutput(i)));
            events.push_back((key, event.content));
        }
    }
    events
}

/// A cursor of the events table.
type ContractAddressEventsIndexCursor<'txn> =
    DbCursor<'txn, RO, ContractAddressEventsIndexKey, NoVersionValueWrapper<NoValue>, CommonPrefix>;
/// A cursor of the transaction_events table.
type TransactionEventsCursor<'txn> = DbCursor<
    'txn,
    RO,
    TransactionIndex,
    crate::db::serialization::VersionZeroWrapper<LocationInFile>,
    SimpleTable,
>;
