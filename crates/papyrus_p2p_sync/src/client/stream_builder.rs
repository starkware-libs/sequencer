use std::cmp::min;
use std::time::Duration;

use async_stream::stream;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use futures::StreamExt;
use papyrus_network::network_manager::{ClientResponsesManager, SqmrClientSender};
use papyrus_protobuf::converters::ProtobufConversionError;
use papyrus_protobuf::sync::{BlockHashOrNumber, DataOrFin, Direction, Query};
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use starknet_api::block::BlockNumber;
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
    // TODO(shahak): Add variant for state diff marker once we support classes sync.
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

    fn create_stream<TQuery>(
        mut sqmr_sender: SqmrClientSender<TQuery, DataOrFin<InputFromNetwork>>,
        storage_reader: StorageReader,
        wait_period_for_new_data: Duration,
        num_blocks_per_query: u64,
        stop_sync_at_block_number: Option<BlockNumber>,
    ) -> BoxStream<'static, DataStreamResult>
    where
        TQuery: From<Query> + Send + 'static,
        Vec<u8>: From<TQuery>,
    {
        stream! {
            let mut current_block_number = Self::get_start_block_number(&storage_reader)?;
            'send_query_and_parse_responses: loop {
                let limit = match Self::BLOCK_NUMBER_LIMIT {
                    BlockNumberLimit::Unlimited => num_blocks_per_query,
                    BlockNumberLimit::HeaderMarker => {
                        let last_block_number = storage_reader.begin_ro_txn()?.get_header_marker()?;
                        let limit = min(
                            last_block_number.0 - current_block_number.0,
                            num_blocks_per_query,
                        );
                        if limit == 0 {
                            debug!("{:?} sync is waiting for a new header", Self::TYPE_DESCRIPTION);
                            tokio::time::sleep(wait_period_for_new_data).await;
                            continue;
                        }
                        limit
                    }
                };
                let end_block_number = current_block_number.0 + limit;
                debug!(
                    "Downloading {:?} for blocks [{}, {})",
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
                    )
                    .await?;

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
                    if stop_sync_at_block_number.is_some_and(|stop_sync_at_block_number| {
                        current_block_number >= stop_sync_at_block_number
                    }) {
                        info!("{:?} hit the stop sync block number.", Self::TYPE_DESCRIPTION);
                        return;
                    }
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
pub(crate) enum BadPeerError {}

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
        ParseDataError::Fatal(P2PSyncClientError::ProtobufConversionError(err))
    }
}
