use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use apollo_batcher_types::batcher_types::Round;
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use apollo_starknet_client::reader::StateDiff;
use async_trait::async_trait;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use indexmap::map::Entry;
use indexmap::IndexMap;
#[cfg(test)]
use mockall::automock;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;
use tracing::info;

use crate::cende_client_types::{
    CendeBlockMetadata,
    CendePreConfirmedBlock,
    CendePreConfirmedTransaction,
    StarknetClientTransactionReceipt,
};
use crate::pre_confirmed_cende_client::{
    CendeWritePreConfirmedBlock,
    PreConfirmedCendeClientError,
    PreConfirmedCendeClientTrait,
};

#[derive(Debug, Error)]
pub enum BlockWriterError {
    #[error(transparent)]
    PreConfirmedCendeClientError(#[from] PreConfirmedCendeClientError),
}

pub type BlockWriterResult<T> = Result<T, BlockWriterError>;

pub type CandidateTxReceiver = tokio::sync::mpsc::Receiver<Vec<InternalConsensusTransaction>>;
pub type CandidateTxSender = tokio::sync::mpsc::Sender<Vec<InternalConsensusTransaction>>;

// TODO(noamsp): rename to pre_confirmed_tx_receiver.
pub type ExecutedTxReceiver = tokio::sync::mpsc::Receiver<(
    InternalConsensusTransaction,
    StarknetClientTransactionReceipt,
    StateDiff,
)>;

// TODO(noamsp): rename to pre_confirmed_tx_sender.
pub type ExecutedTxSender = tokio::sync::mpsc::Sender<(
    InternalConsensusTransaction,
    StarknetClientTransactionReceipt,
    StateDiff,
)>;

/// Coordinates the flow of pre-confirmed block data during block proposal.
/// Listens for transaction updates from the block builder via dedicated channels and utilizes a
/// Cende client to communicate the updates to the Cende recorder.
#[async_trait]
#[cfg_attr(test, automock)]
pub trait PreConfirmedBlockWriterTrait: Send {
    async fn run(&mut self) -> BlockWriterResult<()>;
}

pub struct PreConfirmedBlockWriter {
    pre_confirmed_block_writer_input: PreConfirmedBlockWriterInput,
    candidate_tx_receiver: CandidateTxReceiver,
    executed_tx_receiver: ExecutedTxReceiver,
    cende_client: Arc<dyn PreConfirmedCendeClientTrait>,
    write_block_interval_millis: u64,
}

impl PreConfirmedBlockWriter {
    pub fn new(
        pre_confirmed_block_writer_input: PreConfirmedBlockWriterInput,
        candidate_tx_receiver: CandidateTxReceiver,
        executed_tx_receiver: ExecutedTxReceiver,
        cende_client: Arc<dyn PreConfirmedCendeClientTrait>,
        write_block_interval_millis: u64,
    ) -> Self {
        Self {
            pre_confirmed_block_writer_input,
            candidate_tx_receiver,
            executed_tx_receiver,
            cende_client,
            write_block_interval_millis,
        }
    }

    fn create_pre_confirmed_block(
        &self,
        transactions_map: &IndexMap<
            TransactionHash,
            (
                CendePreConfirmedTransaction,
                Option<StarknetClientTransactionReceipt>,
                Option<StateDiff>,
            ),
        >,
        write_iteration: u64,
    ) -> CendeWritePreConfirmedBlock {
        let mut transactions = Vec::with_capacity(transactions_map.len());
        let mut transaction_receipts = Vec::with_capacity(transactions_map.len());
        let mut transaction_state_diffs = Vec::with_capacity(transactions_map.len());

        for (tx, tx_receipt, tx_state_diff) in transactions_map.values() {
            transactions.push(tx.clone());
            transaction_receipts.push(tx_receipt.clone());
            transaction_state_diffs.push(tx_state_diff.clone());
        }

        let pre_confirmed_block = CendePreConfirmedBlock {
            metadata: self.pre_confirmed_block_writer_input.block_metadata.clone(),
            transactions,
            transaction_receipts,
            transaction_state_diffs,
        };

        CendeWritePreConfirmedBlock {
            block_number: self.pre_confirmed_block_writer_input.block_number,
            round: self.pre_confirmed_block_writer_input.round,
            write_iteration,
            pre_confirmed_block,
        }
    }
}

#[async_trait]
impl PreConfirmedBlockWriterTrait for PreConfirmedBlockWriter {
    async fn run(&mut self) -> BlockWriterResult<()> {
        let mut transactions_map: IndexMap<
            TransactionHash,
            (
                CendePreConfirmedTransaction,
                Option<StarknetClientTransactionReceipt>,
                Option<StateDiff>,
            ),
        > = IndexMap::new();

        let mut pending_tasks = FuturesUnordered::new();
        let mut write_executed_txs_timer =
            tokio::time::interval(Duration::from_millis(self.write_block_interval_millis));

        // We initially mark that we have pending changes so that the client will write to the
        // Cende recorder that a new proposal round has started.
        let mut pending_changes = true;
        let mut write_iteration: u64 = 0;

        loop {
            tokio::select! {
                _ = write_executed_txs_timer.tick() => {
                    // Only send if there are pending changes to avoid unnecessary calls
                    if pending_changes {
                        // TODO(noamsp): Extract to a function.
                        let pre_confirmed_block = self.create_pre_confirmed_block(
                            &transactions_map,
                            write_iteration,
                        );
                        pending_tasks.push(self.cende_client.write_pre_confirmed_block(pre_confirmed_block));
                        write_iteration += 1;
                        pending_changes = false;
                    }
                }
                // TODO(noamsp): Handle height/round mismatch by immediately exiting the loop; All the other writes will be rejected as well.
                Some(_) = pending_tasks.next() => {}
                msg = self.executed_tx_receiver.recv() => {
                    match msg {
                        Some((tx, tx_receipt, tx_state_diff)) => {
                            let tx = CendePreConfirmedTransaction::from(tx);
                            let tx_hash = tx.transaction.transaction_hash();
                            transactions_map.insert(tx_hash, (tx, Some(tx_receipt), Some(tx_state_diff)));
                            pending_changes = true;
                        }
                        None => {
                            info!("Executed tx channel closed");
                            break;
                        }
                    }
                }
                msg = self.candidate_tx_receiver.recv() => {
                    match msg {
                        Some(txs) => {
                            // Skip transactions that were already executed, to avoid an unnecessary write.
                            for tx in txs {
                                let tx = CendePreConfirmedTransaction::from(tx);
                                match transactions_map.entry(tx.transaction.transaction_hash()) {
                                    Entry::Vacant(entry) => {
                                        entry.insert((tx, None, None));
                                        pending_changes = true;
                                    }
                                    Entry::Occupied(_) => {}
                                }
                            }
                        }
                        None => {
                            info!("Candidate tx channel closed");
                            break;
                        }
                    }
                }
            }
        }

        if pending_changes {
            let pre_confirmed_block =
                self.create_pre_confirmed_block(&transactions_map, write_iteration);
            self.cende_client.write_pre_confirmed_block(pre_confirmed_block).await?
        }

        // Wait for all pending tasks to complete gracefully.
        // TODO(noamsp): Add error handling and timeout.
        while pending_tasks.next().await.is_some() {}
        info!("Pre confirmed block writer finished");

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Copy)]
pub struct PreConfirmedBlockWriterConfig {
    pub channel_buffer_capacity: usize,
    pub write_block_interval_millis: u64,
}

impl Default for PreConfirmedBlockWriterConfig {
    fn default() -> Self {
        Self { channel_buffer_capacity: 1000, write_block_interval_millis: 50 }
    }
}

impl SerializeConfig for PreConfirmedBlockWriterConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "channel_buffer_capacity",
                &self.channel_buffer_capacity,
                "The capacity of the channel buffer for receiving pre-confirmed transactions.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "write_block_interval_millis",
                &self.write_block_interval_millis,
                "Time interval (ms) between writing pre-confirmed blocks. Writes occur only when \
                 block data changes.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

#[cfg_attr(test, automock)]
pub trait PreConfirmedBlockWriterFactoryTrait: Send + Sync {
    fn create(
        &self,
        block_number: BlockNumber,
        proposal_round: Round,
        block_metadata: CendeBlockMetadata,
    ) -> (Box<dyn PreConfirmedBlockWriterTrait>, CandidateTxSender, ExecutedTxSender);
}

pub struct PreConfirmedBlockWriterFactory {
    pub config: PreConfirmedBlockWriterConfig,
    pub cende_client: Arc<dyn PreConfirmedCendeClientTrait>,
}

impl PreConfirmedBlockWriterFactoryTrait for PreConfirmedBlockWriterFactory {
    fn create(
        &self,
        block_number: BlockNumber,
        round: Round,
        block_metadata: CendeBlockMetadata,
    ) -> (Box<dyn PreConfirmedBlockWriterTrait>, CandidateTxSender, ExecutedTxSender) {
        // Initialize channels for communication between the pre confirmed block writer and the
        // block builder.
        let (executed_tx_sender, executed_tx_receiver) =
            tokio::sync::mpsc::channel(self.config.channel_buffer_capacity);
        let (candidate_tx_sender, candidate_tx_receiver) =
            tokio::sync::mpsc::channel(self.config.channel_buffer_capacity);

        let cende_client = self.cende_client.clone();

        let pre_confirmed_block_writer_input =
            PreConfirmedBlockWriterInput { block_number, round, block_metadata };

        let pre_confirmed_block_writer = Box::new(PreConfirmedBlockWriter::new(
            pre_confirmed_block_writer_input,
            candidate_tx_receiver,
            executed_tx_receiver,
            cende_client,
            self.config.write_block_interval_millis,
        ));
        (pre_confirmed_block_writer, candidate_tx_sender, executed_tx_sender)
    }
}

// TODO(noamsp): find a better name for this struct.
pub struct PreConfirmedBlockWriterInput {
    pub block_number: BlockNumber,
    pub round: Round,
    pub block_metadata: CendeBlockMetadata,
}
