//! Implementation of the ConsensusContext interface for Papyrus.
//!
//! It connects to papyrus storage and runs consensus on actual blocks that already exist on the
//! network. Useful for testing the consensus algorithm without the need to actually build new
//! blocks.
#[cfg(test)]
#[path = "papyrus_consensus_context_test.rs"]
mod papyrus_consensus_context_test;

use core::panic;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use futures::{SinkExt, StreamExt};
use papyrus_consensus::types::{
    ConsensusContext,
    ConsensusError,
    ProposalContentId,
    Round,
    ValidatorId,
};
use papyrus_network::network_manager::{BroadcastTopicClient, BroadcastTopicClientTrait};
use papyrus_protobuf::consensus::{
    ConsensusMessage,
    Proposal,
    ProposalFin,
    ProposalInit,
    ProposalPart,
    TransactionBatch,
    Vote,
};
use papyrus_storage::body::BodyStorageReader;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::{StorageError, StorageReader};
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::transaction::Transaction;
use tracing::{debug, debug_span, info, warn, Instrument};

// TODO: add debug messages and span to the tasks.

type HeightToIdToContent = BTreeMap<BlockNumber, HashMap<ProposalContentId, Vec<Transaction>>>;

const CHANNEL_SIZE: usize = 100;

pub struct PapyrusConsensusContext {
    storage_reader: StorageReader,
    network_broadcast_client: BroadcastTopicClient<ConsensusMessage>,
    network_proposal_sender: mpsc::Sender<(u64, mpsc::Receiver<ProposalPart>)>,
    validators: Vec<ValidatorId>,
    sync_broadcast_sender: Option<BroadcastTopicClient<Vote>>,
    // Proposal building/validating returns immediately, leaving the actual processing to a spawned
    // task. The spawned task processes the proposal asynchronously and updates the
    // valid_proposals map upon completion, ensuring consistency across tasks.
    valid_proposals: Arc<Mutex<HeightToIdToContent>>,
}

impl PapyrusConsensusContext {
    pub fn new(
        storage_reader: StorageReader,
        network_broadcast_client: BroadcastTopicClient<ConsensusMessage>,
        network_proposal_sender: mpsc::Sender<(u64, mpsc::Receiver<ProposalPart>)>,
        num_validators: u64,
        sync_broadcast_sender: Option<BroadcastTopicClient<Vote>>,
    ) -> Self {
        Self {
            storage_reader,
            network_broadcast_client,
            network_proposal_sender,
            validators: (0..num_validators).map(ContractAddress::from).collect(),
            sync_broadcast_sender,
            valid_proposals: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
}

#[async_trait]
impl ConsensusContext for PapyrusConsensusContext {
    type ProposalChunk = Transaction;
    type ProposalPart = ProposalPart;

    async fn build_proposal(
        &mut self,
        proposal_init: ProposalInit,
        _timeout: Duration,
    ) -> oneshot::Receiver<ProposalContentId> {
        let mut proposal_sender_sender = self.network_proposal_sender.clone();
        let (fin_sender, fin_receiver) = oneshot::channel();

        let storage_reader = self.storage_reader.clone();
        let valid_proposals = Arc::clone(&self.valid_proposals);
        tokio::spawn(
            async move {
                // TODO(dvir): consider fix this for the case of reverts. If between the check that
                // the block in storage and to getting the transaction was a revert
                // this flow will fail.
                wait_for_block(&storage_reader, proposal_init.height)
                    .await
                    .expect("Failed to wait to block");

                let txn = storage_reader.begin_ro_txn().expect("Failed to begin ro txn");
                let transactions = txn
                    .get_block_transactions(proposal_init.height)
                    .expect("Get transactions from storage failed")
                    .unwrap_or_else(|| {
                        panic!(
                            "Block in {} was not found in storage despite waiting for it",
                            proposal_init.height
                        )
                    });

                let block_hash = txn
                    .get_block_header(proposal_init.height)
                    .expect("Get header from storage failed")
                    .unwrap_or_else(|| {
                        panic!(
                            "Block in {} was not found in storage despite waiting for it",
                            proposal_init.height
                        )
                    })
                    .block_hash;

                let (mut proposal_sender, proposal_receiver) = mpsc::channel(CHANNEL_SIZE);
                let stream_id = proposal_init.height.0;
                proposal_sender_sender
                    .send((stream_id, proposal_receiver))
                    .await
                    .expect("Failed to send proposal receiver");
                proposal_sender
                    .send(Self::ProposalPart::Init(proposal_init.clone()))
                    .await
                    .expect("Failed to send proposal init");
                proposal_sender
                    .send(ProposalPart::Transactions(TransactionBatch {
                        transactions: transactions.clone(),
                        tx_hashes: vec![],
                    }))
                    .await
                    .expect("Failed to send transactions");
                proposal_sender
                    .send(ProposalPart::Fin(ProposalFin { proposal_content_id: block_hash }))
                    .await
                    .expect("Failed to send fin");
                {
                    let mut proposals = valid_proposals
                        .lock()
                        .expect("Lock on active proposals was poisoned due to a previous panic");
                    proposals
                        .entry(proposal_init.height)
                        .or_default()
                        .insert(block_hash, transactions);
                }
                // Done after inserting the proposal into the map to avoid race conditions between
                // insertion and calls to `repropose`.
                fin_sender.send(block_hash).expect("Send should succeed");
            }
            .instrument(debug_span!("consensus_build_proposal")),
        );

        fin_receiver
    }

    async fn validate_proposal(
        &mut self,
        height: BlockNumber,
        _timeout: Duration,
        mut content: mpsc::Receiver<Transaction>,
    ) -> oneshot::Receiver<ProposalContentId> {
        let (fin_sender, fin_receiver) = oneshot::channel();

        let storage_reader = self.storage_reader.clone();
        let valid_proposals = Arc::clone(&self.valid_proposals);
        tokio::spawn(
            async move {
                // TODO(dvir): consider fix this for the case of reverts. If between the check that
                // the block in storage and to getting the transaction was a revert
                // this flow will fail.
                wait_for_block(&storage_reader, height).await.expect("Failed to wait to block");

                let txn = storage_reader.begin_ro_txn().expect("Failed to begin ro txn");
                let transactions = txn
                    .get_block_transactions(height)
                    .expect("Get transactions from storage failed")
                    .unwrap_or_else(|| {
                        panic!("Block in {height} was not found in storage despite waiting for it")
                    });

                for tx in transactions.iter() {
                    let received_tx = content
                        .next()
                        .await
                        .unwrap_or_else(|| panic!("Not received transaction equals to {tx:?}"));
                    if tx != &received_tx {
                        panic!("Transactions are not equal. In storage: {tx:?}, : {received_tx:?}");
                    }
                }

                if content.next().await.is_some() {
                    panic!("Received more transactions than expected");
                }

                let block_hash = txn
                    .get_block_header(height)
                    .expect("Get header from storage failed")
                    .unwrap_or_else(|| {
                        panic!("Block in {height} was not found in storage despite waiting for it")
                    })
                    .block_hash;

                let mut proposals = valid_proposals
                    .lock()
                    .expect("Lock on active proposals was poisoned due to a previous panic");

                proposals.entry(height).or_default().insert(block_hash, transactions);
                // Done after inserting the proposal into the map to avoid race conditions between
                // insertion and calls to `repropose`.
                // This can happen as a result of sync interrupting `run_height`.
                fin_sender.send(block_hash).unwrap_or_else(|_| {
                    warn!("Failed to send block to consensus. height={height}");
                })
            }
            .instrument(debug_span!("consensus_validate_proposal")),
        );

        fin_receiver
    }

    async fn repropose(&mut self, id: ProposalContentId, init: ProposalInit) {
        let transactions = self
            .valid_proposals
            .lock()
            .expect("valid_proposals lock was poisoned")
            .get(&init.height)
            .unwrap_or_else(|| panic!("No proposals found for height {}", init.height))
            .get(&id)
            .unwrap_or_else(|| panic!("No proposal found for height {} and id {}", init.height, id))
            .clone();

        let proposal = Proposal {
            height: init.height.0,
            round: init.round,
            proposer: init.proposer,
            transactions,
            block_hash: id,
            valid_round: init.valid_round,
        };
        self.network_broadcast_client
            .broadcast_message(ConsensusMessage::Proposal(proposal))
            .await
            .expect("Failed to send proposal");
    }

    async fn validators(&self, _height: BlockNumber) -> Vec<ValidatorId> {
        self.validators.clone()
    }

    fn proposer(&self, _height: BlockNumber, _round: Round) -> ValidatorId {
        *self.validators.first().expect("there should be at least one validator")
    }

    async fn broadcast(&mut self, message: ConsensusMessage) -> Result<(), ConsensusError> {
        debug!("Broadcasting message: {message:?}");
        self.network_broadcast_client.broadcast_message(message).await?;
        Ok(())
    }

    async fn decision_reached(
        &mut self,
        block: ProposalContentId,
        precommits: Vec<Vote>,
    ) -> Result<(), ConsensusError> {
        let height = precommits[0].height;
        info!("Finished consensus for height: {height}. Agreed on block: {:#064x}", block.0);
        if let Some(sender) = &mut self.sync_broadcast_sender {
            sender.broadcast_message(precommits[0].clone()).await?;
        }

        let mut proposals = self
            .valid_proposals
            .lock()
            .expect("Lock on active proposals was poisoned due to a previous panic");
        proposals.retain(|&h, _| h > BlockNumber(height));
        Ok(())
    }
}

const SLEEP_BETWEEN_CHECK_FOR_BLOCK: Duration = Duration::from_secs(10);

async fn wait_for_block(
    storage_reader: &StorageReader,
    height: BlockNumber,
) -> Result<(), StorageError> {
    while storage_reader.begin_ro_txn()?.get_body_marker()? <= height {
        debug!("Waiting for block {height:?} to continue consensus");
        tokio::time::sleep(SLEEP_BETWEEN_CHECK_FOR_BLOCK).await;
    }
    Ok(())
}
