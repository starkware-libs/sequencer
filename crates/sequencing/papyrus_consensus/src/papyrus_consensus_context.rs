#[cfg(test)]
#[path = "papyrus_consensus_context_test.rs"]
mod papyrus_consensus_context_test;

use core::panic;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use futures::sink::SinkExt;
use futures::StreamExt;
use papyrus_network::network_manager::BroadcastTopicSender;
use papyrus_protobuf::consensus::{ConsensusMessage, Proposal, Vote};
use papyrus_storage::body::BodyStorageReader;
use papyrus_storage::header::HeaderStorageReader;
use papyrus_storage::{StorageError, StorageReader};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ContractAddress;
use starknet_api::transaction::Transaction;
use tokio::sync::Mutex;
use tracing::{debug, debug_span, info, warn, Instrument};

use crate::types::{
    ConsensusContext,
    ConsensusError,
    ProposalContentId,
    ProposalInit,
    Round,
    ValidatorId,
};
use crate::ProposalWrapper;

// TODO: add debug messages and span to the tasks.

#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct PapyrusConsensusBlock {
    content: Vec<Transaction>,
    id: BlockHash,
}

type ActiveProposals = BTreeMap<BlockNumber, HashMap<ProposalContentId, Vec<Transaction>>>;

pub struct PapyrusConsensusContext {
    storage_reader: StorageReader,
    network_broadcast_sender: BroadcastTopicSender<ConsensusMessage>,
    validators: Vec<ValidatorId>,
    sync_broadcast_sender: Option<BroadcastTopicSender<Vote>>,
    active_proposals: Arc<Mutex<ActiveProposals>>,
}

impl PapyrusConsensusContext {
    // TODO(dvir): remove the dead code attribute after we will use this function.
    #[allow(dead_code)]
    pub fn new(
        storage_reader: StorageReader,
        network_broadcast_sender: BroadcastTopicSender<ConsensusMessage>,
        num_validators: u64,
        sync_broadcast_sender: Option<BroadcastTopicSender<Vote>>,
    ) -> Self {
        Self {
            storage_reader,
            network_broadcast_sender,
            validators: (0..num_validators).map(ContractAddress::from).collect(),
            sync_broadcast_sender,
            active_proposals: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub async fn get_proposal(
        &self,
        height: BlockNumber,
        id: ProposalContentId,
    ) -> mpsc::Receiver<Vec<Transaction>> {
        let (mut tx, rx) = mpsc::channel(1);
        if let Some(proposals_at_height) = self.active_proposals.lock().await.get(&height) {
            if let Some(transactions) = proposals_at_height.get(&id) {
                let _ = tx.send(transactions.clone()).await;
            }
        }
        rx
    }
}

const CHANNEL_SIZE: usize = 5000;

#[async_trait]
impl ConsensusContext for PapyrusConsensusContext {
    type Block = PapyrusConsensusBlock;
    type ProposalChunk = Transaction;

    async fn build_proposal(
        &mut self,
        height: BlockNumber,
    ) -> (mpsc::Receiver<Transaction>, oneshot::Receiver<ProposalContentId>) {
        let (mut sender, receiver) = mpsc::channel(CHANNEL_SIZE);
        let (fin_sender, fin_receiver) = oneshot::channel();

        let storage_reader = self.storage_reader.clone();
        let active_proposals = Arc::clone(&self.active_proposals);
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

                for tx in transactions.clone() {
                    sender.try_send(tx).expect("Send should succeed");
                }
                sender.close_channel();

                let block_hash = txn
                    .get_block_header(height)
                    .expect("Get header from storage failed")
                    .unwrap_or_else(|| {
                        panic!("Block in {height} was not found in storage despite waiting for it")
                    })
                    .block_hash;

                let mut proposals = active_proposals.lock().await;
                proposals
                    .entry(height)
                    .or_insert_with(HashMap::new)
                    .insert(block_hash, transactions.clone());

                fin_sender.send(block_hash).expect("Send should succeed");
            }
            .instrument(debug_span!("consensus_build_proposal")),
        );

        (receiver, fin_receiver)
    }

    async fn validate_proposal(
        &mut self,
        height: BlockNumber,
        mut content: mpsc::Receiver<Transaction>,
    ) -> oneshot::Receiver<ProposalContentId> {
        let (fin_sender, fin_receiver) = oneshot::channel();

        let storage_reader = self.storage_reader.clone();
        let active_proposals = Arc::clone(&self.active_proposals);
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

                let mut proposals = active_proposals.lock().await;
                proposals
                    .entry(height)
                    .or_insert_with(HashMap::new)
                    .insert(block_hash, transactions.clone());

                // This can happen as a result of sync interrupting `run_height`.
                fin_sender.send(block_hash).unwrap_or_else(|_| {
                    warn!("Failed to send block to consensus. height={height}");
                })
            }
            .instrument(debug_span!("consensus_validate_proposal")),
        );

        fin_receiver
    }

    async fn validators(&self, _height: BlockNumber) -> Vec<ValidatorId> {
        self.validators.clone()
    }

    fn proposer(&self, _height: BlockNumber, _round: Round) -> ValidatorId {
        *self.validators.first().expect("validators should have at least 2 validators")
    }

    async fn broadcast(&mut self, message: ConsensusMessage) -> Result<(), ConsensusError> {
        debug!("Broadcasting message: {message:?}");
        self.network_broadcast_sender.send(message).await?;
        Ok(())
    }

    async fn propose(
        &self,
        init: ProposalInit,
        mut content_receiver: mpsc::Receiver<Transaction>,
        fin_receiver: oneshot::Receiver<BlockHash>,
    ) -> Result<(), ConsensusError> {
        let mut network_broadcast_sender = self.network_broadcast_sender.clone();

        tokio::spawn(
            async move {
                let mut transactions = Vec::new();
                while let Some(tx) = content_receiver.next().await {
                    transactions.push(tx);
                }

                let Ok(block_hash) = fin_receiver.await else {
                    // This can occur due to sync interrupting a height.
                    warn!("Failed to get block hash from fin receiver. {init:?}");
                    return;
                };
                let proposal = Proposal {
                    height: init.height.0,
                    round: init.round,
                    proposer: init.proposer,
                    transactions,
                    block_hash,
                    valid_round: init.valid_round,
                };
                debug!(
                    "Sending proposal: height={:?} id={:?} num_txs={} block_hash={:?}",
                    proposal.height,
                    proposal.proposer,
                    proposal.transactions.len(),
                    proposal.block_hash
                );

                network_broadcast_sender
                    .send(ConsensusMessage::Proposal(proposal))
                    .await
                    .expect("Failed to send proposal");
            }
            .instrument(debug_span!("consensus_propose")),
        );
        Ok(())
    }

    async fn decision_reached(
        &mut self,
        block: ProposalContentId,
        precommits: Vec<Vote>,
    ) -> Result<(), ConsensusError> {
        let height = precommits[0].height;
        info!("Finished consensus for height: {height}. Agreed on block: {:}", block);
        if let Some(sender) = &mut self.sync_broadcast_sender {
            sender.send(precommits[0].clone()).await?;
        }

        let mut proposals = self.active_proposals.lock().await;
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

impl From<ProposalWrapper>
    for (ProposalInit, mpsc::Receiver<Transaction>, oneshot::Receiver<BlockHash>)
{
    fn from(val: ProposalWrapper) -> Self {
        let transactions: Vec<Transaction> = val.0.transactions.into_iter().collect();
        let proposal_init = ProposalInit {
            height: BlockNumber(val.0.height),
            round: val.0.round,
            proposer: val.0.proposer,
            valid_round: val.0.valid_round,
        };
        let (mut content_sender, content_receiver) = mpsc::channel(transactions.len());
        for tx in transactions {
            content_sender.try_send(tx).expect("Send should succeed");
        }
        content_sender.close_channel();

        let (fin_sender, fin_receiver) = oneshot::channel();
        fin_sender.send(val.0.block_hash).expect("Send should succeed");

        (proposal_init, content_receiver, fin_receiver)
    }
}
