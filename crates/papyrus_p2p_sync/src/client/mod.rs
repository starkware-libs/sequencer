mod class;
#[cfg(test)]
mod class_test;
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
#[cfg(test)]
mod transaction_test;

use std::collections::BTreeMap;
use std::time::Duration;

use class::ClassStreamBuilder;
use futures::channel::mpsc::{Receiver, SendError, Sender};
use futures::stream::BoxStream;
use futures::Stream;
use header::HeaderStreamBuilder;
use papyrus_common::pending_classes::ApiContractClass;
use papyrus_config::converters::deserialize_milliseconds_to_duration;
use papyrus_config::dumping::{ser_param, SerializeConfig};
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
use starknet_api::block::{BlockBody, BlockNumber};
use starknet_api::core::ClassHash;
use starknet_api::state::{DeclaredClasses, DeprecatedDeclaredClasses, ThinStateDiff};
use starknet_api::transaction::FullTransaction;
use starknet_state_sync_types::state_sync_types::SyncBlock;
use state_diff::StateDiffStreamBuilder;
use stream_builder::{DataStreamBuilder, DataStreamResult};
use tokio_stream::StreamExt;
use tracing::{info, instrument};
use transaction::TransactionStreamFactory;

const STEP: u64 = 1;
const ALLOWED_SIGNATURES_LENGTH: usize = 1;

const NETWORK_DATA_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct P2PSyncClientConfig {
    pub num_headers_per_query: u64,
    pub num_block_state_diffs_per_query: u64,
    pub num_block_transactions_per_query: u64,
    pub num_block_classes_per_query: u64,
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub wait_period_for_new_data: Duration,
    pub buffer_size: usize,
}

impl SerializeConfig for P2PSyncClientConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
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
                "num_block_transactions_per_query",
                &self.num_block_transactions_per_query,
                "The maximum amount of blocks to ask their transactions from peers in each \
                 iteration.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "num_block_classes_per_query",
                &self.num_block_classes_per_query,
                "The maximum amount of block's classes to ask from peers in each iteration.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "wait_period_for_new_data",
                &self.wait_period_for_new_data.as_millis(),
                "Time in millisseconds to wait when a query returned with partial data before \
                 sending a new query",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "buffer_size",
                &self.buffer_size,
                "Size of the buffer for read from the storage and for incoming responses.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for P2PSyncClientConfig {
    fn default() -> Self {
        P2PSyncClientConfig {
            num_headers_per_query: 10000,
            // State diffs are split into multiple messages, so big queries can lead to a lot of
            // messages in the network buffers.
            num_block_state_diffs_per_query: 100,
            num_block_transactions_per_query: 100,
            num_block_classes_per_query: 100,
            wait_period_for_new_data: Duration::from_millis(50),
            // TODO(eitan): split this by protocol
            buffer_size: 100000,
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
        _internal_blocks_receivers: InternalBlocksReceivers,
    ) -> impl Stream<Item = DataStreamResult> + Send + 'static {
        let header_stream = HeaderStreamBuilder::create_stream(
            self.header_sender,
            storage_reader.clone(),
            None,
            config.wait_period_for_new_data,
            config.num_headers_per_query,
        );

        let state_diff_stream = StateDiffStreamBuilder::create_stream(
            self.state_diff_sender,
            storage_reader.clone(),
            None,
            config.wait_period_for_new_data,
            config.num_block_state_diffs_per_query,
        );

        let transaction_stream = TransactionStreamFactory::create_stream(
            self.transaction_sender,
            storage_reader.clone(),
            None,
            config.wait_period_for_new_data,
            config.num_block_transactions_per_query,
        );

        let class_stream = ClassStreamBuilder::create_stream(
            self.class_sender,
            storage_reader.clone(),
            None,
            config.wait_period_for_new_data,
            config.num_block_classes_per_query,
        );

        header_stream.merge(state_diff_stream).merge(transaction_stream).merge(class_stream)
    }
}

pub struct P2PSyncClient {
    config: P2PSyncClientConfig,
    storage_reader: StorageReader,
    storage_writer: StorageWriter,
    p2p_sync_channels: P2PSyncClientChannels,
    #[allow(dead_code)]
    internal_blocks_receiver: BoxStream<'static, (BlockNumber, SyncBlock)>,
}

impl P2PSyncClient {
    pub fn new(
        config: P2PSyncClientConfig,
        storage_reader: StorageReader,
        storage_writer: StorageWriter,
        p2p_sync_channels: P2PSyncClientChannels,
        internal_blocks_receiver: BoxStream<'static, (BlockNumber, SyncBlock)>,
    ) -> Self {
        Self { config, storage_reader, storage_writer, p2p_sync_channels, internal_blocks_receiver }
    }

    #[instrument(skip(self), level = "debug", err)]
    pub async fn run(mut self) -> Result<(), P2PSyncClientError> {
        info!("Starting P2P sync client");

        let internal_blocks_channels = InternalBlocksChannels::new();
        self.create_internal_blocks_sender_task(internal_blocks_channels.senders);
        let mut data_stream = self.p2p_sync_channels.create_stream(
            self.storage_reader.clone(),
            self.config,
            internal_blocks_channels.receivers,
        );

        loop {
            let data = data_stream.next().await.expect("Sync data stream should never end")?;
            data.write_to_storage(&mut self.storage_writer)?;
        }
    }

    fn create_internal_blocks_sender_task(
        &self,
        #[allow(unused_variables)] internal_blocks_senders: InternalBlocksSenders,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {})
    }
}

#[allow(dead_code)]
pub(crate) struct InternalBlocksReceivers {
    header_receiver: Receiver<(BlockNumber, SignedBlockHeader)>,
    state_diff_receiver: Receiver<(BlockNumber, (ThinStateDiff, BlockNumber))>,
    transaction_receiver: Receiver<(BlockNumber, (BlockBody, BlockNumber))>,
    #[allow(dead_code)]
    class_receiver:
        Receiver<(BlockNumber, (DeclaredClasses, DeprecatedDeclaredClasses, BlockNumber))>,
}

#[allow(dead_code)]
struct InternalBlocksSenders {
    header_sender: Sender<(BlockNumber, SignedBlockHeader)>,
    state_diff_sender: Sender<(BlockNumber, (ThinStateDiff, BlockNumber))>,
    transaction_sender: Sender<(BlockNumber, (BlockBody, BlockNumber))>,
    #[allow(dead_code)]
    class_sender: Sender<(BlockNumber, (DeclaredClasses, DeprecatedDeclaredClasses, BlockNumber))>,
}

struct InternalBlocksChannels {
    receivers: InternalBlocksReceivers,
    senders: InternalBlocksSenders,
}

impl InternalBlocksChannels {
    pub fn new() -> Self {
        let (header_sender, header_receiver) = futures::channel::mpsc::channel(100);
        let (state_diff_sender, state_diff_receiver) = futures::channel::mpsc::channel(100);
        let (transaction_sender, transaction_receiver) = futures::channel::mpsc::channel(100);
        let (class_sender, class_receiver) = futures::channel::mpsc::channel(100);

        Self {
            receivers: InternalBlocksReceivers {
                header_receiver,
                state_diff_receiver,
                transaction_receiver,
                class_receiver,
            },
            senders: InternalBlocksSenders {
                header_sender,
                state_diff_sender,
                transaction_sender,
                class_sender,
            },
        }
    }
}
