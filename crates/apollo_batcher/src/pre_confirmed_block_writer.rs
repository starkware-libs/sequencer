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
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;
use tracing::{error, info};

use crate::cende_client_types::{
    CendeBlockMetadata,
    CendePreconfirmedBlock,
    CendePreconfirmedTransaction,
    StarknetClientTransactionReceipt,
};
use crate::pre_confirmed_cende_client::{
    CendeWritePreconfirmedBlock,
    PreconfirmedCendeClientError,
    PreconfirmedCendeClientTrait,
};

#[derive(Debug, Error)]
pub enum BlockWriterError {
    #[error(transparent)]
    PreconfirmedCendeClientError(#[from] PreconfirmedCendeClientError),
}

pub type BlockWriterResult<T> = Result<T, BlockWriterError>;

pub type CandidateTxReceiver = tokio::sync::mpsc::Receiver<Vec<InternalConsensusTransaction>>;
pub type CandidateTxSender = tokio::sync::mpsc::Sender<Vec<InternalConsensusTransaction>>;

pub type PreconfirmedTxReceiver = tokio::sync::mpsc::Receiver<(
    InternalConsensusTransaction,
    StarknetClientTransactionReceipt,
    StateDiff,
)>;

pub type PreconfirmedTxSender = tokio::sync::mpsc::Sender<(
    InternalConsensusTransaction,
    StarknetClientTransactionReceipt,
    StateDiff,
)>;

/// Coordinates the flow of pre-confirmed block data during block proposal.
/// Listens for transaction updates from the block builder via dedicated channels and utilizes a
/// Cende client to communicate the updates to the Cende recorder.
#[async_trait]
#[cfg_attr(test, automock)]
pub trait PreconfirmedBlockWriterTrait: Send {
    async fn run(&mut self) -> BlockWriterResult<()>;
}

pub struct PreconfirmedBlockWriter {
    pre_confirmed_block_writer_input: PreconfirmedBlockWriterInput,
    candidate_tx_receiver: CandidateTxReceiver,
    pre_confirmed_tx_receiver: PreconfirmedTxReceiver,
    cende_client: Arc<dyn PreconfirmedCendeClientTrait>,
    write_block_interval_millis: u64,
}

impl PreconfirmedBlockWriter {
    pub fn new(
        pre_confirmed_block_writer_input: PreconfirmedBlockWriterInput,
        candidate_tx_receiver: CandidateTxReceiver,
        pre_confirmed_tx_receiver: PreconfirmedTxReceiver,
        cende_client: Arc<dyn PreconfirmedCendeClientTrait>,
        write_block_interval_millis: u64,
    ) -> Self {
        Self {
            pre_confirmed_block_writer_input,
            candidate_tx_receiver,
            pre_confirmed_tx_receiver,
            cende_client,
            write_block_interval_millis,
        }
    }

    fn create_pre_confirmed_block(
        &self,
        transactions_map: &IndexMap<
            TransactionHash,
            (
                CendePreconfirmedTransaction,
                Option<StarknetClientTransactionReceipt>,
                Option<StateDiff>,
            ),
        >,
        write_iteration: u64,
    ) -> CendeWritePreconfirmedBlock {
        let mut transactions = Vec::with_capacity(transactions_map.len());
        let mut transaction_receipts = Vec::with_capacity(transactions_map.len());
        let mut transaction_state_diffs = Vec::with_capacity(transactions_map.len());

        for (tx, tx_receipt, tx_state_diff) in transactions_map.values() {
            transactions.push(tx.clone());
            transaction_receipts.push(tx_receipt.clone());
            transaction_state_diffs.push(tx_state_diff.clone());
        }

        let pre_confirmed_block = CendePreconfirmedBlock {
            metadata: self.pre_confirmed_block_writer_input.block_metadata.clone(),
            transactions,
            transaction_receipts,
            transaction_state_diffs,
        };

        CendeWritePreconfirmedBlock {
            block_number: self.pre_confirmed_block_writer_input.block_number,
            round: self.pre_confirmed_block_writer_input.round,
            write_iteration,
            pre_confirmed_block,
        }
    }
}

#[async_trait]
impl PreconfirmedBlockWriterTrait for PreconfirmedBlockWriter {
    async fn run(&mut self) -> BlockWriterResult<()> {
        let mut transactions_map: IndexMap<
            TransactionHash,
            (
                CendePreconfirmedTransaction,
                Option<StarknetClientTransactionReceipt>,
                Option<StateDiff>,
            ),
        > = IndexMap::new();

        let mut pending_tasks = FuturesUnordered::new();
        let mut write_pre_confirmed_txs_timer =
            tokio::time::interval(Duration::from_millis(self.write_block_interval_millis));

        // We initially mark that we have pending changes so that the client will write to the
        // Cende recorder that a new proposal round has started.
        let mut pending_changes = true;
        let mut next_write_iteration = 0;

        loop {
            tokio::select! {
                _ = write_pre_confirmed_txs_timer.tick() => {
                    // Only send if there are pending changes to avoid unnecessary calls
                    if pending_changes {
                        // TODO(noamsp): Extract to a function.
                        let pre_confirmed_block = self.create_pre_confirmed_block(
                            &transactions_map,
                            next_write_iteration,
                        );
                        pending_tasks.push(self.cende_client.write_pre_confirmed_block(pre_confirmed_block));
                        next_write_iteration += 1;
                        pending_changes = false;
                    }
                }

                Some(result) = pending_tasks.next() => {
                    if let Err(error) = result {
                        if is_round_mismatch_error(&error, next_write_iteration) {
                            pending_tasks.clear();
                            return Err(error.into());
                        }
                    }
                }
                msg = self.pre_confirmed_tx_receiver.recv() => {
                    match msg {
                        Some((tx, tx_receipt, tx_state_diff)) => {
                            let tx = CendePreconfirmedTransaction::from(tx);
                            let tx_hash = tx.transaction_hash();
                            transactions_map.insert(tx_hash, (tx, Some(tx_receipt), Some(tx_state_diff)));
                            pending_changes = true;
                        }
                        None => {
                            info!("Pre confirmed tx channel closed");
                            break;
                        }
                    }
                }
                msg = self.candidate_tx_receiver.recv() => {
                    match msg {
                        Some(txs) => {
                            // Skip transactions that were already executed, to avoid an unnecessary write.
                            for tx in txs {
                                let tx = CendePreconfirmedTransaction::from(tx);
                                match transactions_map.entry(tx.transaction_hash()) {
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
                self.create_pre_confirmed_block(&transactions_map, next_write_iteration);
            self.cende_client.write_pre_confirmed_block(pre_confirmed_block).await?
        }

        // Wait for all pending tasks to complete gracefully.
        // TODO(noamsp): Add timeout.
        while let Some(result) = pending_tasks.next().await {
            if let Err(error) = result {
                if is_round_mismatch_error(&error, next_write_iteration) {
                    pending_tasks.clear();
                    return Err(error.into());
                }
            }
        }
        info!("Pre confirmed block writer finished");

        Ok(())
    }
}

fn is_round_mismatch_error(
    error: &PreconfirmedCendeClientError,
    next_write_iteration: u64,
) -> bool {
    let PreconfirmedCendeClientError::CendeRecorderError {
        block_number,
        round,
        write_iteration,
        status_code,
    } = error
    else {
        return false;
    };

    // A bad request status indicates a round or write iteration mismatch. The latest request can
    // receive a bad request status only if it is due to a round mismatch.
    if *status_code == StatusCode::BAD_REQUEST && *write_iteration == next_write_iteration - 1 {
        error!(
            "A higher round was detected for block_number: {}. rejected round: {}. Stopping \
             pre-confirmed block writer.",
            block_number, round,
        );
        return true;
    }
    false
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Copy)]
pub struct PreconfirmedBlockWriterConfig {
    pub channel_buffer_capacity: usize,
    pub write_block_interval_millis: u64,
}

impl Default for PreconfirmedBlockWriterConfig {
    fn default() -> Self {
        Self { channel_buffer_capacity: 1000, write_block_interval_millis: 50 }
    }
}

impl SerializeConfig for PreconfirmedBlockWriterConfig {
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
pub trait PreconfirmedBlockWriterFactoryTrait: Send + Sync {
    fn create(
        &self,
        block_number: BlockNumber,
        proposal_round: Round,
        block_metadata: CendeBlockMetadata,
    ) -> (Box<dyn PreconfirmedBlockWriterTrait>, CandidateTxSender, PreconfirmedTxSender);
}

pub struct PreconfirmedBlockWriterFactory {
    pub config: PreconfirmedBlockWriterConfig,
    pub cende_client: Arc<dyn PreconfirmedCendeClientTrait>,
}

impl PreconfirmedBlockWriterFactoryTrait for PreconfirmedBlockWriterFactory {
    fn create(
        &self,
        block_number: BlockNumber,
        round: Round,
        block_metadata: CendeBlockMetadata,
    ) -> (Box<dyn PreconfirmedBlockWriterTrait>, CandidateTxSender, PreconfirmedTxSender) {
        info!("Create pre confirmed block writer for block {}", block_number);
        // Initialize channels for communication between the pre confirmed block writer and the
        // block builder.
        let (pre_confirmed_tx_sender, pre_confirmed_tx_receiver) =
            tokio::sync::mpsc::channel(self.config.channel_buffer_capacity);
        let (candidate_tx_sender, candidate_tx_receiver) =
            tokio::sync::mpsc::channel(self.config.channel_buffer_capacity);

        let cende_client = self.cende_client.clone();

        let pre_confirmed_block_writer_input =
            PreconfirmedBlockWriterInput { block_number, round, block_metadata };

        let pre_confirmed_block_writer = Box::new(PreconfirmedBlockWriter::new(
            pre_confirmed_block_writer_input,
            candidate_tx_receiver,
            pre_confirmed_tx_receiver,
            cende_client,
            self.config.write_block_interval_millis,
        ));
        (pre_confirmed_block_writer, candidate_tx_sender, pre_confirmed_tx_sender)
    }
}

// TODO(noamsp): find a better name for this struct.
pub struct PreconfirmedBlockWriterInput {
    pub block_number: BlockNumber,
    pub round: Round,
    pub block_metadata: CendeBlockMetadata,
}
