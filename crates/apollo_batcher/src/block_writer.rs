use std::collections::BTreeMap;

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use async_trait::async_trait;
use blockifier::fee::receipt::TransactionReceipt;
use serde::{Deserialize, Serialize};
use starknet_api::transaction::TransactionHash;
use thiserror::Error;

use crate::batcher::AbortSignalSender;

#[derive(Debug, Error)]
pub enum BlockWriterError {}

pub type BlockWriterResult<T> = Result<T, BlockWriterError>;
pub type ProposalPreConfirmedTxReceiver =
    tokio::sync::mpsc::UnboundedReceiver<(TransactionHash, Option<TransactionReceipt>)>;
pub type ProposalPreConfirmedTxSender =
    tokio::sync::mpsc::UnboundedSender<(TransactionHash, Option<TransactionReceipt>)>;

// TODO(noamsp): Refactor this to use an actual client instead of a placeholder.
#[derive(Clone)]
pub struct CendeClient {}

#[async_trait]
pub trait BlockWriterTrait: Send {
    async fn write_block(&mut self) -> BlockWriterResult<()>;
}

pub struct BlockWriter {
    _proposal_pre_confirmed_tx_receiver: ProposalPreConfirmedTxReceiver,
    _abort_signal_receiver: tokio::sync::oneshot::Receiver<()>,
    _cende_client: CendeClient,
    _time_to_live: usize,
}

impl BlockWriter {
    pub fn new(
        proposal_pre_confirmed_tx_receiver: ProposalPreConfirmedTxReceiver,
        abort_signal_receiver: tokio::sync::oneshot::Receiver<()>,
        cende_client: CendeClient,
        time_to_live: usize,
    ) -> Self {
        Self {
            _proposal_pre_confirmed_tx_receiver: proposal_pre_confirmed_tx_receiver,
            _abort_signal_receiver: abort_signal_receiver,
            _cende_client: cende_client,
            _time_to_live: time_to_live,
        }
    }
}

#[async_trait]
impl BlockWriterTrait for BlockWriter {
    async fn write_block(&mut self) -> BlockWriterResult<()> {
        todo!("Implement block writing logic")
    }
}

pub trait BlockWriterFactoryTrait: Send + Sync {
    fn create_block_writer(
        &self,
    ) -> BlockWriterResult<(
        Box<dyn BlockWriterTrait>,
        ProposalPreConfirmedTxSender,
        AbortSignalSender,
    )>;
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BlockWriterConfig {
    pub time_to_live_seconds: usize,
}

impl Default for BlockWriterConfig {
    fn default() -> Self {
        Self { time_to_live_seconds: 60 }
    }
}

impl SerializeConfig for BlockWriterConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param(
            "time_to_live_seconds",
            &self.time_to_live_seconds,
            "Duration in seconds for pre-confirmed transaction storage in the cende server.",
            ParamPrivacyInput::Public,
        )])
    }
}

pub struct BlockWriterFactory {
    pub block_writer_config: BlockWriterConfig,
    pub cende_client: CendeClient,
}

impl BlockWriterFactoryTrait for BlockWriterFactory {
    fn create_block_writer(
        &self,
    ) -> BlockWriterResult<(
        Box<dyn BlockWriterTrait>,
        ProposalPreConfirmedTxSender,
        AbortSignalSender,
    )> {
        let (abort_signal_sender, abort_signal_receiver) = tokio::sync::oneshot::channel();
        // Initialize channel for communication between the block writer and the block builder.
        let (proposal_pre_confirmed_tx_sender, proposal_pre_confirmed_tx_receiver) =
            tokio::sync::mpsc::unbounded_channel();

        let block_writer = Box::new(BlockWriter::new(
            proposal_pre_confirmed_tx_receiver,
            abort_signal_receiver,
            self.cende_client.clone(),
            self.block_writer_config.time_to_live_seconds,
        ));
        Ok((block_writer, proposal_pre_confirmed_tx_sender, abort_signal_sender))
    }
}
