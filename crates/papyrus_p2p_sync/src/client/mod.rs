mod header;
#[cfg(test)]
mod header_test;
mod state_diff;
#[cfg(test)]
mod state_diff_test;
mod stream_builder;
#[cfg(test)]
mod test_utils;
mod transaction;

use std::collections::BTreeMap;
use std::time::Duration;

use futures::channel::mpsc::SendError;
use futures::Stream;
use header::HeaderStreamBuilder;
use papyrus_common::pending_classes::ApiContractClass;
use papyrus_config::converters::deserialize_seconds_to_duration;
use papyrus_config::dumping::{ser_optional_param, ser_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use papyrus_network::network_manager::SqmrClientSender;
use papyrus_protobuf::sync::{
    ClassQuery,
    DataOrFin,
    HeaderQuery,
    SignedBlockHeader,
    StateDiffChunk,
    StateDiffQuery,
    TransactionQuery,
};
use papyrus_storage::{StorageError, StorageReader, StorageWriter};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::core::ClassHash;
use starknet_api::transaction::FullTransaction;
use state_diff::StateDiffStreamBuilder;
use stream_builder::{DataStreamBuilder, DataStreamResult};
use tokio_stream::StreamExt;
use tracing::instrument;
use transaction::TransactionStreamFactory;
const STEP: u64 = 1;
const ALLOWED_SIGNATURES_LENGTH: usize = 1;

const NETWORK_DATA_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct P2PSyncClientConfig {
    pub num_headers_per_query: u64,
    pub num_block_state_diffs_per_query: u64,
    pub num_transactions_per_query: u64,
    #[serde(deserialize_with = "deserialize_seconds_to_duration")]
    pub wait_period_for_new_data: Duration,
    pub buffer_size: usize,
    pub stop_sync_at_block_number: Option<BlockNumber>,
}

impl SerializeConfig for P2PSyncClientConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        let mut config = BTreeMap::from_iter([
            ser_param(
                "num_headers_per_query",
                &self.num_headers_per_query,
                "The maximum amount of headers to ask from peers in each iteration.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "num_block_state_diffs_per_query",
                &self.num_block_state_diffs_per_query,
                "The maximum amount of block's state diffs to ask from peers in each iteration.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "num_transactions_per_query",
                &self.num_transactions_per_query,
                "The maximum amount of blocks to ask their transactions from peers in each \
                 iteration.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "wait_period_for_new_data",
                &self.wait_period_for_new_data.as_secs(),
                "Time in seconds to wait when a query returned with partial data before sending a \
                 new query",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "buffer_size",
                &self.buffer_size,
                "Size of the buffer for read from the storage and for incoming responses.",
                ParamPrivacyInput::Public,
            ),
        ]);
        config.extend(ser_optional_param(
            &self.stop_sync_at_block_number,
            BlockNumber(1000),
            "stop_sync_at_block_number",
            "Stops the sync at given block number and closes the node cleanly. Used to run \
             profiling on the node.",
            ParamPrivacyInput::Public,
        ));
        config
    }
}

impl Default for P2PSyncClientConfig {
    fn default() -> Self {
        P2PSyncClientConfig {
            num_headers_per_query: 10000,
            // State diffs are split into multiple messages, so big queries can lead to a lot of
            // messages in the network buffers.
            num_block_state_diffs_per_query: 100,
            num_transactions_per_query: 100,
            wait_period_for_new_data: Duration::from_secs(5),
            // TODO(eitan): split this by protocol
            buffer_size: 100000,
            stop_sync_at_block_number: None,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum P2PSyncClientError {
    // TODO(shahak): Remove this and report to network on invalid data once that's possible.
    #[error("Network returned more responses than expected for a query.")]
    TooManyResponses,
    #[error(
        "Encountered an old header in the storage at {block_number:?} that's missing the field \
         {missing_field}. Re-sync the node from {block_number:?} from a node that provides this \
         field."
    )]
    OldHeaderInStorage { block_number: BlockNumber, missing_field: &'static str },
    #[error("The sender end of the response receivers for {type_description:?} was closed.")]
    ReceiverChannelTerminated { type_description: &'static str },
    #[error(transparent)]
    NetworkTimeout(#[from] tokio::time::error::Elapsed),
    #[error(transparent)]
    StorageError(#[from] StorageError),
    #[error(transparent)]
    SendError(#[from] SendError),
}

type HeaderSqmrSender = SqmrClientSender<HeaderQuery, DataOrFin<SignedBlockHeader>>;
type StateSqmrDiffSender = SqmrClientSender<StateDiffQuery, DataOrFin<StateDiffChunk>>;
type TransactionSqmrSender = SqmrClientSender<TransactionQuery, DataOrFin<FullTransaction>>;
type ClassSqmrSender = SqmrClientSender<ClassQuery, DataOrFin<(ApiContractClass, ClassHash)>>;

pub struct P2PSyncClientChannels {
    header_sender: HeaderSqmrSender,
    state_diff_sender: StateSqmrDiffSender,
    transaction_sender: TransactionSqmrSender,
    #[allow(dead_code)]
    class_sender: ClassSqmrSender,
}

impl P2PSyncClientChannels {
    pub fn new(
        header_sender: HeaderSqmrSender,
        state_diff_sender: StateSqmrDiffSender,
        transaction_sender: TransactionSqmrSender,
        class_sender: ClassSqmrSender,
    ) -> Self {
        Self { header_sender, state_diff_sender, transaction_sender, class_sender }
    }
    pub(crate) fn create_stream(
        self,
        storage_reader: StorageReader,
        config: P2PSyncClientConfig,
    ) -> impl Stream<Item = DataStreamResult> + Send + 'static {
        let header_stream = HeaderStreamBuilder::create_stream(
            self.header_sender,
            storage_reader.clone(),
            config.wait_period_for_new_data,
            config.num_headers_per_query,
            config.stop_sync_at_block_number,
        );

        let state_diff_stream = StateDiffStreamBuilder::create_stream(
            self.state_diff_sender,
            storage_reader.clone(),
            config.wait_period_for_new_data,
            config.num_block_state_diffs_per_query,
            config.stop_sync_at_block_number,
        );

        let transaction_stream = TransactionStreamFactory::create_stream(
            self.transaction_sender,
            storage_reader.clone(),
            config.wait_period_for_new_data,
            config.num_transactions_per_query,
            config.stop_sync_at_block_number,
        );

        header_stream.merge(state_diff_stream).merge(transaction_stream)
    }
}

pub struct P2PSyncClient {
    config: P2PSyncClientConfig,
    storage_reader: StorageReader,
    storage_writer: StorageWriter,
    p2p_sync_channels: P2PSyncClientChannels,
}

impl P2PSyncClient {
    pub fn new(
        config: P2PSyncClientConfig,
        storage_reader: StorageReader,
        storage_writer: StorageWriter,
        p2p_sync_channels: P2PSyncClientChannels,
    ) -> Self {
        Self { config, storage_reader, storage_writer, p2p_sync_channels }
    }

    #[instrument(skip(self), level = "debug", err)]
    pub async fn run(mut self) -> Result<(), P2PSyncClientError> {
        let mut data_stream =
            self.p2p_sync_channels.create_stream(self.storage_reader.clone(), self.config);

        loop {
            let data = data_stream.next().await.expect("Sync data stream should never end")?;
            data.write_to_storage(&mut self.storage_writer)?;
        }
    }
}
