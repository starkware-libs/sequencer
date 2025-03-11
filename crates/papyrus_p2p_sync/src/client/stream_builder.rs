use std::cmp::min;
use std::collections::HashMap;
use std::time::Duration;

use async_stream::stream;
use futures::channel::mpsc::Receiver;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt};
use papyrus_network::network_manager::{ClientResponsesManager, SqmrClientSender};
use papyrus_protobuf::converters::ProtobufConversionError;
use papyrus_protobuf::sync::{BlockHashOrNumber, DataOrFin, Direction, Query};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use starknet_api::block::{BlockNumber, BlockSignature};
use starknet_api::core::ClassHash;
use starknet_state_sync_types::state_sync_types::SyncBlock;
use tracing::{debug, info, warn};

use super::{P2PSyncClientError, STEP};

pub type DataStreamResult = Result<Box<dyn BlockData>, P2PSyncClientError>;

pub(crate) trait BlockData: Send {
    fn write_to_storage(
        // This is Box<Self> in order to allow using it with `Box<dyn BlockData>`.
        self: Box<Self>,
        storage_writer: &mut StorageWriter,
    ) -> Result<(), StorageError>;
}

pub(crate) enum BlockNumberLimit {
    Unlimited,
    HeaderMarker,
    StateDiffMarker,
}

pub(crate) trait DataStreamBuilder<InputFromNetwork>
where
    InputFromNetwork: Send + 'static,
    DataOrFin<InputFromNetwork>: TryFrom<Vec<u8>, Error = ProtobufConversionError>,
{
    type Output: BlockData + 'static;

    const TYPE_DESCRIPTION: &'static str;
    const BLOCK_NUMBER_LIMIT: BlockNumberLimit;

    // Async functions in trait don't work well with argument references
    fn parse_data_for_block<'a>(
        client_response_manager: &'a mut ClientResponsesManager<DataOrFin<InputFromNetwork>>,
        block_number: BlockNumber,
        storage_reader: &'a StorageReader,
    ) -> BoxFuture<'a, Result<Option<Self::Output>, ParseDataError>>;

    fn get_start_block_number(storage_reader: &StorageReader) -> Result<BlockNumber, StorageError>;

    // TODO(Eitan): Remove option on return once we have a class manager component.
    /// Returning None happens when internal blocks are disabled for this stream.
    fn convert_sync_block_to_block_data(
        block_number: BlockNumber,
        sync_block: SyncBlock,
    ) -> Option<Self::Output>;

    fn get_internal_block_at(
        internal_blocks_received: &mut HashMap<BlockNumber, Self::Output>,
        internal_block_receiver: &mut Option<Receiver<(BlockNumber, SyncBlock)>>,
        current_block_number: BlockNumber,
    ) -> Option<Self::Output> {
        if let Some(block) = internal_blocks_received.remove(&current_block_number) {
            return Some(block);
        }
        let Some(internal_block_receiver) = internal_block_receiver else { return None };
        while let Some((block_number, sync_block)) = internal_block_receiver
            .next()
            .now_or_never()
            .map(|now_or_never_res| now_or_never_res.expect("Internal block receiver closed"))
        {
            if block_number >= current_block_number {
                // If None is received then we don't use internal blocks for this stream
                // TODO(Eitan): Remove this once we have a class manager component.
                let block_data = Self::convert_sync_block_to_block_data(block_number, sync_block)?;
                if block_number == current_block_number {
                    return Some(block_data);
                }
                internal_blocks_received.insert(block_number, block_data);
            }
        }
        None
    }

    fn create_stream<TQuery>(
        mut sqmr_sender: SqmrClientSender<TQuery, DataOrFin<InputFromNetwork>>,
        storage_reader: StorageReader,
        mut internal_block_receiver: Option<Receiver<(BlockNumber, SyncBlock)>>,
        wait_period_for_new_data: Duration,
        num_blocks_per_query: u64,
    ) -> BoxStream<'static, DataStreamResult>
    where
        TQuery: From<Query> + Send + 'static,
        Vec<u8>: From<TQuery>,
    {
        stream! {
            let mut current_block_number = Self::get_start_block_number(&storage_reader)?;
            let mut internal_blocks_received = HashMap::new();
            'send_query_and_parse_responses: loop {
                let limit = match Self::BLOCK_NUMBER_LIMIT {
                    BlockNumberLimit::Unlimited => num_blocks_per_query,
                    BlockNumberLimit::HeaderMarker | BlockNumberLimit::StateDiffMarker => {
                        let (last_block_number, description) = match Self::BLOCK_NUMBER_LIMIT {
                            BlockNumberLimit::HeaderMarker => (storage_reader.begin_ro_txn()?.get_header_marker()?, "header"),
                            BlockNumberLimit::StateDiffMarker => (storage_reader.begin_ro_txn()?.get_state_marker()?, "state diff"),
                            _ => unreachable!(),
                        };
                        let limit = min(last_block_number.0 - current_block_number.0, num_blocks_per_query);
                        if limit == 0 {
                            debug!("{:?} sync is waiting for a new {}", Self::TYPE_DESCRIPTION, description);
                            tokio::time::sleep(wait_period_for_new_data).await;
                            continue;
                        }
                        limit
                    },
                };

                if let Some(block) = Self::get_internal_block_at(&mut internal_blocks_received, &mut internal_block_receiver, current_block_number)
                {
                    debug!("Sync received internally {:?} for block {}.", Self::TYPE_DESCRIPTION, current_block_number);
                    yield Ok(Box::<dyn BlockData>::from(Box::new(block)));
                    current_block_number = current_block_number.unchecked_next();
                    continue 'send_query_and_parse_responses;
                }

                let end_block_number = current_block_number.0 + limit;
                debug!(
                    "Sync downloading {:?} for blocks [{}, {}) from network.",
                    Self::TYPE_DESCRIPTION,
                    current_block_number.0,
                    end_block_number,
                );

                // TODO(shahak): Use the report callback.
                let mut client_response_manager = sqmr_sender
                    .send_new_query(
                        TQuery::from(Query {
                            start_block: BlockHashOrNumber::Number(current_block_number),
                            direction: Direction::Forward,
                            limit,
                            step: STEP,
                        })
                    ).await?;
                while current_block_number.0 < end_block_number {
                    match Self::parse_data_for_block(
                        &mut client_response_manager, current_block_number, &storage_reader
                    ).await {
                        Ok(Some(output)) => yield Ok(Box::<dyn BlockData>::from(Box::new(output))),
                        Ok(None) => {
                            debug!(
                                "Query for {:?} on {:?} returned with partial data. Waiting {:?} before \
                                 sending another query.",
                                Self::TYPE_DESCRIPTION, current_block_number, wait_period_for_new_data
                            );
                            tokio::time::sleep(wait_period_for_new_data).await;
                            continue 'send_query_and_parse_responses;
                        },
                        Err(ParseDataError::BadPeer(err)) => {
                            warn!(
                                "Query for {:?} on {:?} returned with bad peer error: {:?}. reporting \
                                 peer and retrying query.",
                                Self::TYPE_DESCRIPTION, current_block_number, err
                            );
                            client_response_manager.report_peer();
                            continue 'send_query_and_parse_responses;
                        },
                        Err(ParseDataError::Fatal(err)) => {
                            yield Err(err);
                            return;
                        },
                    }
                    info!("Added {:?} for block {}.", Self::TYPE_DESCRIPTION, current_block_number);
                    current_block_number = current_block_number.unchecked_next();
                }

                // Consume the None message signaling the end of the query.
                match client_response_manager.next().await {
                    Some(Ok(DataOrFin(None))) => {
                        debug!("Query sent to network for {:?} finished", Self::TYPE_DESCRIPTION);
                    },
                    Some(_) => Err(P2PSyncClientError::TooManyResponses)?,
                    None => Err(P2PSyncClientError::ReceiverChannelTerminated {
                        type_description: Self::TYPE_DESCRIPTION
                    })?,
                }
            }
        }
        .boxed()
    }
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum BadPeerError {
    #[error(
        "Blocks returned unordered from the network. Expected header with \
         {expected_block_number}, got {actual_block_number}."
    )]
    HeadersUnordered { expected_block_number: BlockNumber, actual_block_number: BlockNumber },
    #[error(
        "Expected to receive {expected} transactions for {block_number} from the network. Got \
         {actual} instead."
    )]
    NotEnoughTransactions { expected: usize, actual: usize, block_number: u64 },
    #[error("Expected to receive one signature from the network. got {signatures:?} instead.")]
    WrongSignaturesLength { signatures: Vec<BlockSignature> },
    #[error(
        "The header says that the block's state diff should be of length {expected_length}. Can \
         only divide the state diff parts into the following lengths: {possible_lengths:?}."
    )]
    WrongStateDiffLength { expected_length: usize, possible_lengths: Vec<usize> },
    #[error("Two state diff parts for the same state diff are conflicting.")]
    ConflictingStateDiffParts,
    #[error(
        "Received an empty state diff part from the network (this is a potential DDoS vector)."
    )]
    EmptyStateDiffPart,
    #[error(transparent)]
    ProtobufConversionError(#[from] ProtobufConversionError),
    #[error(
        "Expected to receive {expected} classes for {block_number} from the network. Got {actual} \
         classes instead"
    )]
    NotEnoughClasses { expected: usize, actual: usize, block_number: u64 },
    #[error("The class with hash {class_hash} was not found in the state diff.")]
    ClassNotInStateDiff { class_hash: ClassHash },
    #[error("Received two classes with the same hash: {class_hash}.")]
    DuplicateClass { class_hash: ClassHash },
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum ParseDataError {
    #[error(transparent)]
    Fatal(#[from] P2PSyncClientError),
    #[error(transparent)]
    BadPeer(#[from] BadPeerError),
}

impl From<StorageError> for ParseDataError {
    fn from(err: StorageError) -> Self {
        ParseDataError::Fatal(P2PSyncClientError::StorageError(err))
    }
}

impl From<tokio::time::error::Elapsed> for ParseDataError {
    fn from(err: tokio::time::error::Elapsed) -> Self {
        ParseDataError::Fatal(P2PSyncClientError::NetworkTimeout(err))
    }
}

impl From<ProtobufConversionError> for ParseDataError {
    fn from(err: ProtobufConversionError) -> Self {
        ParseDataError::BadPeer(BadPeerError::ProtobufConversionError(err))
    }
}
