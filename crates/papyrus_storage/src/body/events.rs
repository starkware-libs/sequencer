//! Interface for iterating over events from the storage.
//!
//! Events are part of the transaction output. Each transaction output holds an array of events.
//! Import [`EventsReader`] to iterate over events using a read-only [`StorageTxn`].
//!
//! # Example
//! ```
//! use papyrus_storage::open_storage;
//! use papyrus_storage::body::TransactionIndex;
//! use papyrus_storage::body::events::{EventIndex, EventsReader};
//! # use papyrus_storage::{db::DbConfig, StorageConfig};
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
//! # Ok::<(), papyrus_storage::StorageError>(())
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
    TransactionOutput,
};
use tracing::debug;

use super::{update_marker, AddressToTxIndexTable, EventsTable, FileOffsetsTable, MarkerKind};
use crate::body::{AddressToTxIndexTableKey, TransactionIndex};
use crate::db::serialization::{NoVersionValueWrapper, VersionZeroWrapper};
use crate::db::table_types::{CommonPrefix, DbCursor, DbCursorTrait, NoValue, SimpleTable, Table};
use crate::db::{DbTransaction, RO, RW};
use crate::{
    FileHandlers,
    OffsetKind,
    StorageResult,
    StorageScope,
    StorageTxn,
    TransactionMetadata,
};

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
}

// TODO: support all read transactions (including RW).
impl<'txn, 'env> EventsReader<'txn, 'env> for StorageTxn<'env, RO> {
    fn iter_events(
        &'env self,
        optional_address: Option<ContractAddress>,
        event_index: EventIndex,
        to_block_number: BlockNumber,
    ) -> StorageResult<EventIter<'txn, 'env>> {
        if let Some(address) = optional_address {
            return Ok(EventIter::ByContractAddress(
                self.iter_events_by_contract_address((address, event_index))?,
            ));
        }

        Ok(EventIter::ByEventIndex(self.iter_events_by_event_index(event_index, to_block_number)?))
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
    file_handles: &'txn FileHandlers<RO>,
    events_table: EventsTable<'env>,
    // This value is the next entry in the events table to search for relevant events. If it is
    // None there are no more events.
    next_entry_in_address_to_tx_index_table: Option<AddressToTxIndexTableKey>,
    // Queue of events to return from the iterator. When this queue is empty, we need to fetch more
    // events.
    events_queue: VecDeque<((ContractAddress, EventIndex), EventContent)>,
    cursor: AddressToTxIndexTableCursor<'txn>,
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
            let Some((contract_address, tx_index)) =
                self.next_entry_in_address_to_tx_index_table.take()
            else {
                return Ok(None);
            };
            let mut events_cursor = self.events_table.cursor(self.txn)?;
            let (_, event_location) = events_cursor
                .lower_bound(&tx_index)?
                .unwrap_or_else(|| panic!("Events not found for transaction index: {tx_index:?}."));
            let transaction_events = self.file_handles.get_events_unchecked(event_location)?;
            // TODO(dvir): don't clone the events here. (alonl: Where are they cloned?)
            self.events_queue =
                get_events_from_tx(transaction_events, tx_index, contract_address, 0);
            self.next_entry_in_address_to_tx_index_table = self.cursor.next()?.map(|(key, _)| key);
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
    tx_current: Option<(TransactionIndex, TransactionOutput)>,
    tx_cursor: TransactionMetadataTableCursor<'txn>,
    event_index_in_tx_current: EventIndexInTransactionOutput,
    to_block_number: BlockNumber,
}

impl EventIterByEventIndex<'_> {
    /// Returns the next event. If there are no more events, returns None.
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was an error.
    fn next(&mut self) -> StorageResult<Option<((ContractAddress, EventIndex), EventContent)>> {
        let Some((tx_index, tx_output)) = &self.tx_current else { return Ok(None) };
        let Some(Event { from_address, content }) =
            tx_output.events().get(self.event_index_in_tx_current.0)
        else {
            return Ok(None);
        };
        let key = (*from_address, EventIndex(*tx_index, self.event_index_in_tx_current));
        // TODO(dvir): don't clone here the event content.
        let content = content.clone();
        self.event_index_in_tx_current.0 += 1;
        self.find_next_event_by_event_index()?;
        Ok(Some((key, content.clone())))
    }

    /// Finds the event that corresponds to the first event index greater than or equals to the
    /// current event index. The current event index is composed of the transaction index of the
    /// current transaction (tx_current) and the event index in current transaction output
    /// (event_index_in_tx_current).
    ///
    /// # Errors
    /// Returns [`StorageError`](crate::StorageError) if there was an error.
    fn find_next_event_by_event_index(&mut self) -> StorageResult<()> {
        while let Some((tx_index, tx_output)) = &self.tx_current {
            if tx_index.0 > self.to_block_number {
                self.tx_current = None;
                break;
            }
            // Checks if there's an event in the current event index.
            if tx_output.events().len() > self.event_index_in_tx_current.0 {
                break;
            }

            // There are no more events in the current transaction, so we go over the rest of the
            // transactions until we find an event.
            let Some((tx_index, tx_metadata)) = self.tx_cursor.next()? else {
                self.tx_current = None;
                return Ok(());
            };
            self.tx_current = Some((
                tx_index,
                self.file_handlers
                    .get_transaction_output_unchecked(tx_metadata.tx_output_location)?,
            ));
            self.event_index_in_tx_current = EventIndexInTransactionOutput(0);
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
        // let transaction_metadata_table = self.open_table(&self.tables.transaction_metadata)?;
        let address_to_tx_index_table = self.open_table(&self.tables.address_to_tx_index)?;
        let events_table = self.open_table(&self.tables.events)?;
        let mut cursor = address_to_tx_index_table.cursor(&self.txn)?;
        let events_queue = if let Some((contract_address, tx_index)) =
            cursor.lower_bound(&(key.0, key.1.0))?.map(|(key, _)| key)
        {
            let mut event_cursor = events_table.cursor(&self.txn)?;
            let (_, event_location) = event_cursor
                .lower_bound(&tx_index)?
                .unwrap_or_else(|| panic!("Events not found for transaction index: {tx_index:?}."));
            let events = self.file_handlers.get_events_unchecked(event_location)?;

            // In case of we get tx_index different from the key, it means we need to start a new
            // transaction which means the first event.
            let start_event_index = if tx_index == key.1.0 { key.1.1.0 } else { 0 };
            // TODO(dvir): don't clone the events here.
            get_events_from_tx(events, tx_index, contract_address, start_event_index)
        } else {
            VecDeque::new()
        };
        let next_entry_in_event_table = cursor.next()?.map(|(key, _)| key);

        Ok(EventIterByContractAddress {
            txn: &self.txn,
            file_handles: &self.file_handlers,
            events_table,
            next_entry_in_address_to_tx_index_table: next_entry_in_event_table,
            events_queue,
            cursor,
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
        let transaction_metadata_table = self.open_table(&self.tables.transaction_metadata)?;
        let mut tx_cursor = transaction_metadata_table.cursor(&self.txn)?;
        let first_txn_location = tx_cursor.lower_bound(&event_index.0)?;
        let first_relevant_transaction = match first_txn_location {
            None => None,
            Some((tx_index, tx_metadata)) => Some((
                tx_index,
                self.file_handlers
                    .get_transaction_output_unchecked(tx_metadata.tx_output_location)?,
            )),
        };

        let mut it = EventIterByEventIndex {
            file_handlers: &self.file_handlers,
            tx_current: first_relevant_transaction,
            tx_cursor,
            event_index_in_tx_current: event_index.1,
            to_block_number,
        };
        it.find_next_event_by_event_index()?;
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
type AddressToTxIndexTableCursor<'txn> =
    DbCursor<'txn, RO, AddressToTxIndexTableKey, NoVersionValueWrapper<NoValue>, CommonPrefix>;
/// A cursor of the transaction outputs table.
type TransactionMetadataTableCursor<'txn> =
    DbCursor<'txn, RO, TransactionIndex, VersionZeroWrapper<TransactionMetadata>, SimpleTable>;

/// Interface for reading the events from the storage.
pub trait EventStorageReader {
    /// The first block number that isn't written in the events table.
    fn get_events_marker(&self) -> StorageResult<BlockNumber>;

    /// Returns the events of a specific block.
    fn get_block_events(&self, block_number: BlockNumber)
    -> StorageResult<Option<Vec<Vec<Event>>>>;

    /// Returns the events of a specific transaction.
    fn get_transaction_events(
        &self,
        transaction_index: &TransactionIndex,
    ) -> StorageResult<Option<Vec<Event>>>;
}

impl EventStorageReader for StorageTxn<'_, RW> {
    fn get_events_marker(&self) -> StorageResult<BlockNumber> {
        let markers_table = self.open_table(&self.tables.markers)?;
        Ok(markers_table.get(&self.txn, &MarkerKind::Event)?.unwrap_or_default())
    }

    fn get_block_events(
        &self,
        block_number: BlockNumber,
    ) -> StorageResult<Option<Vec<Vec<Event>>>> {
        let events_table = self.open_table(&self.tables.events)?;
        let mut cursor = events_table.cursor(&self.txn)?;
        let mut current =
            cursor.lower_bound(&TransactionIndex(block_number, TransactionOffsetInBlock(0)))?;
        let mut res = Vec::new();
        while let Some((tx_index, events_location)) = current {
            if tx_index.0 != block_number {
                break;
            }
            res.push(self.file_handlers.get_events_unchecked(events_location)?);
            current = cursor.next()?;
        }
        if res.is_empty() { Ok(None) } else { Ok(Some(res)) }
    }

    fn get_transaction_events(
        &self,
        transaction_index: &TransactionIndex,
    ) -> StorageResult<Option<Vec<Event>>> {
        let events_table = self.open_table(&self.tables.events)?;
        let mut cursor = events_table.cursor(&self.txn)?;
        let current = cursor.lower_bound(transaction_index)?;
        if let Some((_, events_location)) = current {
            Ok(Some(self.file_handlers.get_events_unchecked(events_location)?))
        } else {
            panic!("Transaction events not found for transaction index: {transaction_index:?}");
        }
    }
}

/// Interface for updating the events in the storage.
pub trait EventStorageWriter
where
    Self: Sized,
{
    /// Appends the events of an entire block to the storage.
    fn append_events(
        self,
        block_number: BlockNumber,
        block_events: Vec<Vec<Event>>,
    ) -> StorageResult<Self>;

    /// Removes the events of an entire block from the storage.
    fn revert_events(
        self,
        block_number: BlockNumber,
    ) -> StorageResult<(Self, Option<Vec<Vec<Event>>>)>;
}

impl EventStorageWriter for StorageTxn<'_, RW> {
    fn append_events(
        self,
        block_number: BlockNumber,
        block_events: Vec<Vec<Event>>,
    ) -> StorageResult<Self> {
        let markers_table = self.open_table(&self.tables.markers)?;
        update_marker(&self.txn, &markers_table, block_number)?;
        if self.scope != StorageScope::StateOnly {
            let events_table = self.open_table(&self.tables.events)?;
            let file_offset_table = self.open_table(&self.tables.file_offsets)?;
            let address_to_tx_index_table = self.open_table(&self.tables.address_to_tx_index)?;
            write_events(
                block_events,
                &self.txn,
                &self.file_handlers,
                &file_offset_table,
                &events_table,
                &address_to_tx_index_table,
                block_number,
            )?;
        }
        Ok(self)
    }

    fn revert_events(
        self,
        block_number: BlockNumber,
    ) -> StorageResult<(Self, Option<Vec<Vec<Event>>>)> {
        let markers_table = self.open_table(&self.tables.markers)?;
        // Assert that body marker equals the reverted block number + 1
        let current_header_marker = self.get_events_marker()?;
        if block_number
            .next()
            .filter(|next_block_number| current_header_marker == *next_block_number)
            .is_none()
        {
            debug!(
                "Attempt to revert a non-existing / old block {}. Returning without an action.",
                block_number
            );
            return Ok((self, None));
        }
        let reverted_block_events = 'reverted_block_events: {
            if self.scope == StorageScope::StateOnly {
                break 'reverted_block_events None;
            } else {
                let events_table = self.open_table(&self.tables.events)?;
                let address_to_tx_index_table =
                    self.open_table(&self.tables.address_to_tx_index)?;
                let block_events = self.get_block_events(block_number)?.unwrap_or_else(|| {
                    panic!("Block events not found for block number: {block_number:?}")
                });
                // Assuming theres a vector of events for every transaction (even if empty), so
                // the index of each item of this enumerate is the transaction index in the
                // block.
                for (index, transaction_events) in block_events.iter().enumerate() {
                    let transaction_index =
                        TransactionIndex(block_number, TransactionOffsetInBlock(index));
                    events_table.delete(&self.txn, &transaction_index)?;
                    for event in transaction_events {
                        address_to_tx_index_table
                            .delete(&self.txn, &(event.from_address, transaction_index))?;
                    }
                }
                Some(block_events)
            }
        };
        markers_table.upsert(&self.txn, &MarkerKind::Event, &block_number)?;
        Ok((self, reverted_block_events))
    }
}

#[allow(clippy::too_many_arguments)]
fn write_events<'env>(
    block_events: Vec<Vec<Event>>,
    txn: &DbTransaction<'env, RW>,
    file_handlers: &FileHandlers<RW>,
    file_offset_table: &'env FileOffsetsTable<'env>,
    events_table: &'env EventsTable<'env>,
    address_to_tx_index_table: &'env AddressToTxIndexTable<'env>,
    block_number: BlockNumber,
) -> StorageResult<()> {
    for (index, transaction_events) in block_events.iter().enumerate() {
        let transaction_index = TransactionIndex(block_number, TransactionOffsetInBlock(index));
        let event_offset = file_handlers.append_events(&transaction_events.clone());
        events_table.append(txn, &transaction_index, &event_offset)?;
        for event in transaction_events {
            address_to_tx_index_table.insert(
                txn,
                &(event.from_address, transaction_index),
                &NoValue,
            )?;
        }
        if index == block_events.len() - 1 {
            file_offset_table.upsert(txn, &OffsetKind::Events, &event_offset.next_offset())?;
        }
    }
    StorageResult::Ok(())
}
