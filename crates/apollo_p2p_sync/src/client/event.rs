use std::collections::HashMap;

use apollo_class_manager_types::SharedClassManagerClient;
use apollo_network::network_manager::ClientResponsesManager;
use apollo_protobuf::sync::DataOrFin;
use apollo_state_sync_types::state_sync_types::SyncBlock;
use apollo_storage::body::{BodyStorageReader, BodyStorageWriter};
use apollo_storage::header::HeaderStorageReader;
use apollo_storage::{StorageError, StorageReader, StorageWriter};
use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use starknet_api::block::BlockNumber;
use starknet_api::transaction::{Event, TransactionHash};

use super::block_data_stream_builder::{
    BadPeerError,
    BlockData,
    BlockDataStreamBuilder,
    BlockNumberLimit,
    ParseDataError,
};
use super::P2pSyncClientError;

pub(crate) struct EventsForBlock {
    pub events_per_tx: Vec<Vec<Event>>,
    pub block_number: BlockNumber,
}

impl BlockData for EventsForBlock {
    fn write_to_storage<'a>(
        self: Box<Self>,
        storage_writer: &'a mut StorageWriter,
        _class_manager_client: &'a mut SharedClassManagerClient,
    ) -> BoxFuture<'a, Result<(), P2pSyncClientError>> {
        async move {
            storage_writer
                .begin_rw_txn()?
                .append_events(self.block_number, &self.events_per_tx)?
                .commit()?;
            Ok(())
        }
        .boxed()
    }
}

pub(crate) struct EventStreamFactory;

impl BlockDataStreamBuilder<(Event, TransactionHash)> for EventStreamFactory {
    type Output = EventsForBlock;

    const TYPE_DESCRIPTION: &'static str = "events";
    // Events depend on the body being synced (we need transaction hashes to group events).
    const BLOCK_NUMBER_LIMIT: BlockNumberLimit = BlockNumberLimit::BodyMarker;

    fn parse_data_for_block<'a>(
        events_response_manager: &'a mut ClientResponsesManager<
            DataOrFin<(Event, TransactionHash)>,
        >,
        block_number: BlockNumber,
        storage_reader: &'a StorageReader,
    ) -> BoxFuture<'a, Result<Option<Self::Output>, ParseDataError>> {
        async move {
            let txn = storage_reader.begin_ro_txn()?;
            let header = txn
                .get_block_header(block_number)?
                .expect("A header with number lower than the body marker is missing");
            let num_events = header.n_events;

            let transaction_hashes = txn
                .get_block_transaction_hashes(block_number)?
                .expect("Block transaction hashes missing for block below body marker");
            let num_transactions = transaction_hashes.len();

            // Map each transaction hash to its index in the block.
            let tx_hash_to_index: HashMap<TransactionHash, usize> = transaction_hashes
                .into_iter()
                .enumerate()
                .map(|(index, tx_hash)| (tx_hash, index))
                .collect();

            let mut events_per_tx: Vec<Vec<Event>> = vec![vec![]; num_transactions];
            let mut received_events = 0;

            while received_events < num_events {
                let maybe_event = events_response_manager.next().await.ok_or(
                    ParseDataError::BadPeer(BadPeerError::SessionEndedWithoutFin {
                        type_description: Self::TYPE_DESCRIPTION,
                    }),
                )?;
                let Some((event, tx_hash)) = maybe_event?.0 else {
                    if received_events == 0 {
                        return Ok(None);
                    } else {
                        return Err(ParseDataError::BadPeer(BadPeerError::NotEnoughEvents {
                            expected: num_events,
                            actual: received_events,
                            block_number: block_number.0,
                        }));
                    }
                };
                let tx_index = tx_hash_to_index.get(&tx_hash).ok_or(ParseDataError::BadPeer(
                    BadPeerError::EventWithUnknownTransactionHash {
                        tx_hash,
                        block_number: block_number.0,
                    },
                ))?;
                events_per_tx[*tx_index].push(event);
                received_events += 1;
            }

            Ok(Some(EventsForBlock { events_per_tx, block_number }))
        }
        .boxed()
    }

    fn get_start_block_number(storage_reader: &StorageReader) -> Result<BlockNumber, StorageError> {
        storage_reader.begin_ro_txn()?.get_event_marker()
    }

    fn convert_sync_block_to_block_data(
        block_number: BlockNumber,
        sync_block: SyncBlock,
    ) -> EventsForBlock {
        let num_transactions =
            sync_block.account_transaction_hashes.len() + sync_block.l1_transaction_hashes.len();
        EventsForBlock { events_per_tx: vec![vec![]; num_transactions], block_number }
    }
}
