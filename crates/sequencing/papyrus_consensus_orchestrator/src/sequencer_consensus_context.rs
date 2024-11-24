//! Implementation of the ConsensusContext interface for running the sequencer.
//!
//! It connects to the Batcher who is responsible for building/validating blocks.
#[cfg(test)]
#[path = "sequencer_consensus_context_test.rs"]
mod sequencer_consensus_context_test;

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
use papyrus_network::network_manager::BroadcastTopicClient;
use papyrus_protobuf::consensus::{
    ConsensusMessage,
    ProposalFin,
    ProposalInit,
    ProposalPart,
    TransactionBatch,
    Vote,
};
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::executable_transaction::Transaction;
use starknet_batcher_types::batcher_types::{
    DecisionReachedInput,
    GetProposalContent,
    GetProposalContentInput,
    ProposalId,
    ProposalStatus,
    ProposeBlockInput,
    SendProposalContent,
    SendProposalContentInput,
    StartHeightInput,
    ValidateBlockInput,
};
use starknet_batcher_types::communication::BatcherClient;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tracing::{debug, debug_span, error, info, trace, warn, Instrument};

// {height: {proposal_id: (content, [proposal_ids])}}
// Note that multiple proposals IDs can be associated with the same content, but we only need to
// store one of them.
type HeightToIdToContent =
    BTreeMap<BlockNumber, HashMap<ProposalContentId, (Vec<Transaction>, ProposalId)>>;
type ValidationParams = (BlockNumber, Duration, mpsc::Receiver<Vec<Transaction>>);

const CHANNEL_SIZE: usize = 100;

pub struct SequencerConsensusContext {
    batcher: Arc<dyn BatcherClient>,
    validators: Vec<ValidatorId>,
    // Proposal building/validating returns immediately, leaving the actual processing to a spawned
    // task. The spawned task processes the proposal asynchronously and updates the
    // valid_proposals map upon completion, ensuring consistency across tasks.
    valid_proposals: Arc<Mutex<HeightToIdToContent>>,
    // Used to generate unique proposal IDs across the lifetime of the context.
    // TODO(matan): Consider robustness in case consensus can restart without the Batcher
    // restarting.
    proposal_id: u64,
    current_height: Option<BlockNumber>,
    current_round: Round,
    // Used to broadcast proposals to other consensus nodes.
    // TODO(Guy) switch to the actual streaming struct.
    _proposal_streaming_client: BroadcastTopicClient<ProposalPart>,
    // The active proposal refers to the proposal being validated at the current height/round.
    // Building proposals are not tracked as active, as consensus can't move on to the next
    // height/round until building is done. Context only works on proposals for the
    // current round.
    active_proposal: Option<(Arc<Notify>, JoinHandle<()>)>,
    // Stores proposals for future rounds until the round is reached.
    queued_proposals: BTreeMap<Round, (ValidationParams, oneshot::Sender<ProposalContentId>)>,
    outbound_proposal_sender: mpsc::Sender<(u64, mpsc::Receiver<ProposalPart>)>,
}

impl SequencerConsensusContext {
    pub fn new(
        batcher: Arc<dyn BatcherClient>,
        _proposal_streaming_client: BroadcastTopicClient<ProposalPart>,
        outbound_proposal_sender: mpsc::Sender<(u64, mpsc::Receiver<ProposalPart>)>,
        num_validators: u64,
    ) -> Self {
        Self {
            batcher,
            _proposal_streaming_client,
            outbound_proposal_sender,
            validators: (0..num_validators).map(ValidatorId::from).collect(),
            valid_proposals: Arc::new(Mutex::new(HeightToIdToContent::new())),
            proposal_id: 0,
            current_height: None,
            current_round: 0,
            active_proposal: None,
            queued_proposals: BTreeMap::new(),
        }
    }
}

#[async_trait]
impl ConsensusContext for SequencerConsensusContext {
    // TODO(guyn): Switch to ProposalPart when done with the streaming integration.
    type ProposalChunk = Vec<Transaction>;
    type ProposalPart = ProposalPart;

    async fn build_proposal(
        &mut self,
        proposal_init: ProposalInit,
        timeout: Duration,
    ) -> oneshot::Receiver<ProposalContentId> {
        // Handles interrupting an active proposal from a previous height/round
        self.set_height_and_round(proposal_init.height, proposal_init.round).await;
        debug!(
            "Building proposal for height: {} with timeout: {:?}",
            proposal_init.height, timeout
        );
        let (fin_sender, fin_receiver) = oneshot::channel();

        let batcher = Arc::clone(&self.batcher);
        let valid_proposals = Arc::clone(&self.valid_proposals);

        let proposal_id = ProposalId(self.proposal_id);
        self.proposal_id += 1;
        let timeout =
            chrono::Duration::from_std(timeout).expect("Can't convert timeout to chrono::Duration");
        let build_proposal_input = ProposeBlockInput {
            proposal_id,
            // TODO: Discuss with batcher team passing std Duration instead.
            deadline: chrono::Utc::now() + timeout,
            // TODO: This is not part of Milestone 1.
            retrospective_block_hash: Some(BlockHashAndNumber {
                number: BlockNumber::default(),
                hash: BlockHash::default(),
            }),
            // TODO(Dan, Matan): Fill block info.
            block_info: Default::default(),
        };
        // TODO: Should we be returning an error?
        // I think this implies defining an error type in this crate and moving the trait definition
        // here also.
        debug!("Initiating proposal build: {build_proposal_input:?}");
        batcher
            .propose_block(build_proposal_input)
            .await
            .expect("Failed to initiate proposal build");
        debug!("Broadcasting proposal init: {proposal_init:?}");
        let (mut proposal_sender, proposal_receiver) = mpsc::channel(CHANNEL_SIZE);
        let stream_id = proposal_init.height.0;
        self.outbound_proposal_sender
            .send((stream_id, proposal_receiver))
            .await
            .expect("Failed to send proposal receiver");
        proposal_sender
            .send(ProposalPart::Init(proposal_init.clone()))
            .await
            .expect("Failed to send proposal init");
        tokio::spawn(
            async move {
                stream_build_proposal(
                    proposal_init.height,
                    proposal_id,
                    batcher,
                    valid_proposals,
                    proposal_sender,
                    fin_sender,
                )
                .await;
            }
            .instrument(debug_span!("consensus_build_proposal")),
        );

        fin_receiver
    }

    async fn validate_proposal(
        &mut self,
        height: BlockNumber,
        round: Round,
        timeout: Duration,
        content: mpsc::Receiver<Self::ProposalChunk>,
    ) -> oneshot::Receiver<ProposalContentId> {
        assert_eq!(Some(height), self.current_height);
        let (fin_sender, fin_receiver) = oneshot::channel();
        match round.cmp(&self.current_round) {
            std::cmp::Ordering::Less => fin_receiver,
            std::cmp::Ordering::Greater => {
                self.queued_proposals.insert(round, ((height, timeout, content), fin_sender));
                fin_receiver
            }
            std::cmp::Ordering::Equal => {
                self.validate_current_round_proposal(height, timeout, content, fin_sender).await;
                fin_receiver
            }
        }
    }

    async fn repropose(&mut self, id: ProposalContentId, init: ProposalInit) {
        let height = init.height;
        debug!("Getting proposal for height: {height} and id: {id}");
        let (_transactions, _) = self
            .valid_proposals
            .lock()
            .expect("Lock on active proposals was poisoned due to a previous panic")
            .get(&height)
            .unwrap_or_else(|| panic!("No proposals found for height {height}"))
            .get(&id)
            .unwrap_or_else(|| panic!("No proposal found for height {height} and id {id}"));
        // TODO: Stream the TXs to the network.
    }

    async fn validators(&self, _height: BlockNumber) -> Vec<ValidatorId> {
        self.validators.clone()
    }

    fn proposer(&self, _height: BlockNumber, _round: Round) -> ValidatorId {
        *self.validators.first().expect("there should be at least one validator")
    }

    async fn broadcast(&mut self, message: ConsensusMessage) -> Result<(), ConsensusError> {
        debug!("No-op broadcasting message: {message:?}");
        Ok(())
    }

    async fn decision_reached(
        &mut self,
        block: ProposalContentId,
        precommits: Vec<Vote>,
    ) -> Result<(), ConsensusError> {
        let height = precommits[0].height;
        info!("Finished consensus for height: {height}. Agreed on block: {:#064x}", block.0);

        // TODO(matan): Broadcast the decision to the network.

        let proposal_id;
        {
            let mut proposals = self
                .valid_proposals
                .lock()
                .expect("Lock on active proposals was poisoned due to a previous panic");
            proposal_id = proposals.get(&BlockNumber(height)).unwrap().get(&block).unwrap().1;
            proposals.retain(|&h, _| h > BlockNumber(height));
        }
        self.batcher.decision_reached(DecisionReachedInput { proposal_id }).await.unwrap();

        Ok(())
    }

    async fn set_height_and_round(&mut self, height: BlockNumber, round: Round) {
        if self.current_height.map(|h| height > h).unwrap_or(true) {
            self.current_height = Some(height);
            assert_eq!(round, 0);
            self.current_round = round;
            self.interrupt_active_proposal();
            self.queued_proposals.clear();
            self.active_proposal = None;
            // The Batcher must be told when we begin to work on a new height. The implicit model is
            // that consensus works on a given height until it is done (either a decision is reached
            // or sync causes us to move on) and then moves on to a different height, never to
            // return to the old height.
            self.batcher
                .start_height(StartHeightInput { height })
                .await
                .expect("Batcher should be ready to start the next height");
            return;
        }
        assert_eq!(Some(height), self.current_height);
        if round == self.current_round {
            return;
        }
        assert!(round > self.current_round);
        self.interrupt_active_proposal();
        self.current_round = round;
        let mut to_process = None;
        while let Some(entry) = self.queued_proposals.first_entry() {
            match self.current_round.cmp(entry.key()) {
                std::cmp::Ordering::Less => {
                    entry.remove();
                }
                std::cmp::Ordering::Equal => {
                    to_process = Some(entry.remove());
                    break;
                }
                std::cmp::Ordering::Greater => return,
            }
        }
        // Validate the proposal for the current round if exists.
        let Some(((height, timeout, content), fin_sender)) = to_process else {
            return;
        };
        self.validate_current_round_proposal(height, timeout, content, fin_sender).await;
    }
}

impl SequencerConsensusContext {
    async fn validate_current_round_proposal(
        &mut self,
        height: BlockNumber,
        timeout: Duration,
        content: mpsc::Receiver<Vec<Transaction>>,
        fin_sender: oneshot::Sender<ProposalContentId>,
    ) {
        debug!("Validating proposal for height: {height} with timeout: {timeout:?}");
        let batcher = Arc::clone(&self.batcher);
        let valid_proposals = Arc::clone(&self.valid_proposals);
        let proposal_id = ProposalId(self.proposal_id);
        self.proposal_id += 1;

        let chrono_timeout =
            chrono::Duration::from_std(timeout).expect("Can't convert timeout to chrono::Duration");
        let input = ValidateBlockInput {
            proposal_id,
            deadline: chrono::Utc::now() + chrono_timeout,
            // TODO(Matan 3/11/2024): Add the real value of the retrospective block hash.
            retrospective_block_hash: Some(BlockHashAndNumber {
                number: BlockNumber::default(),
                hash: BlockHash::default(),
            }),
            // TODO(Dan, Matan): Fill block info.
            block_info: Default::default(),
        };
        batcher.validate_block(input).await.expect("Failed to initiate proposal validation");

        let notify = Arc::new(Notify::new());
        let notify_clone = Arc::clone(&notify);

        let handle = tokio::spawn(
            async move {
                let validate_fut = stream_validate_proposal(
                    height,
                    proposal_id,
                    batcher,
                    valid_proposals,
                    content,
                    fin_sender,
                );
                tokio::select! {
                    _ = notify_clone.notified() => {}
                    result = tokio::time::timeout(timeout, validate_fut) =>{
                        if let Err(e) = result {
                            error!("Validation timed out. {e:?}");
                        }
                    }
                }
            }
            .instrument(debug_span!("consensus_validate_proposal")),
        );
        self.active_proposal = Some((notify, handle));
    }

    fn interrupt_active_proposal(&self) {
        if let Some((notify, _)) = &self.active_proposal {
            notify.notify_one();
        }
    }
}

// Handles building a new proposal without blocking consensus:
// 1. Receive chunks of content from the batcher.
// 2. Forward these to the stream handler to be streamed out to the network.
// 3. Once finished, receive the commitment from the batcher.
// 4. Store the proposal for re-proposal.
// 5. Send the commitment to the stream handler (to send fin).
async fn stream_build_proposal(
    height: BlockNumber,
    proposal_id: ProposalId,
    batcher: Arc<dyn BatcherClient>,
    valid_proposals: Arc<Mutex<HeightToIdToContent>>,
    mut proposal_sender: mpsc::Sender<ProposalPart>,
    fin_sender: oneshot::Sender<ProposalContentId>,
) {
    let mut content = Vec::new();
    loop {
        let response =
            match batcher.get_proposal_content(GetProposalContentInput { proposal_id }).await {
                Ok(response) => response,
                Err(e) => {
                    warn!("Failed to get proposal content: {e:?}");
                    return;
                }
            };
        match response.content {
            GetProposalContent::Txs(txs) => {
                content.extend_from_slice(&txs[..]);
                // TODO: Broadcast the transactions to the network.
                // TODO(matan): Convert to protobuf and make sure this isn't too large for a single
                // proto message (could this be a With adapter added to the channel in `new`?).
                let mut transaction_hashes = Vec::with_capacity(txs.len());
                let mut transactions = Vec::with_capacity(txs.len());
                for tx in txs.into_iter() {
                    transaction_hashes.push(tx.tx_hash());
                    transactions.push(tx.into());
                }
                debug!("Broadcasting proposal content: {transaction_hashes:?}");
                trace!("Broadcasting proposal content: {transactions:?}");
                proposal_sender
                    .send(ProposalPart::Transactions(TransactionBatch {
                        transactions,
                        tx_hashes: transaction_hashes,
                    }))
                    .await
                    .expect("Failed to broadcast proposal content");
            }
            GetProposalContent::Finished(id) => {
                let proposal_content_id = BlockHash(id.state_diff_commitment.0.0);
                info!(
                    "Finished building proposal {:?}: content_id = {:?}, num_txs = {:?}, height = \
                     {:?}",
                    proposal_id,
                    proposal_content_id,
                    content.len(),
                    height
                );
                debug!("Broadcasting proposal fin: {proposal_content_id:?}");
                proposal_sender
                    .send(ProposalPart::Fin(ProposalFin { proposal_content_id }))
                    .await
                    .expect("Failed to broadcast proposal fin");
                // Update valid_proposals before sending fin to avoid a race condition
                // with `repropose` being called before `valid_proposals` is updated.
                let mut valid_proposals = valid_proposals.lock().expect("Lock was poisoned");
                valid_proposals
                    .entry(height)
                    .or_default()
                    .insert(proposal_content_id, (content, proposal_id));
                if fin_sender.send(proposal_content_id).is_err() {
                    // Consensus may exit early (e.g. sync).
                    warn!("Failed to send proposal content id");
                }
                return;
            }
        }
    }
}

// Handles receiving a proposal from another node without blocking consensus:
// 1. Receives the proposal content from the network.
// 2. Pass this to the batcher.
// 3. Once finished, receive the commitment from the batcher.
// 4. Store the proposal for re-proposal.
// 5. Send the commitment to consensus.
async fn stream_validate_proposal(
    height: BlockNumber,
    proposal_id: ProposalId,
    batcher: Arc<dyn BatcherClient>,
    valid_proposals: Arc<Mutex<HeightToIdToContent>>,
    mut content_receiver: mpsc::Receiver<Vec<Transaction>>,
    fin_sender: oneshot::Sender<ProposalContentId>,
) {
    let mut content = Vec::new();
    while let Some(txs) = content_receiver.next().await {
        content.extend_from_slice(&txs[..]);
        let input =
            SendProposalContentInput { proposal_id, content: SendProposalContent::Txs(txs) };
        let response = batcher.send_proposal_content(input).await.unwrap_or_else(|e| {
            panic!("Failed to send proposal content to batcher: {proposal_id:?}. {e:?}")
        });
        match response.response {
            ProposalStatus::Processing => {}
            ProposalStatus::Finished(fin) => {
                panic!("Batcher returned Fin before all content was sent: {proposal_id:?} {fin:?}");
            }
            ProposalStatus::Aborted => {
                panic!("Unexpected abort response for proposal: {:?}", proposal_id);
            }
            ProposalStatus::InvalidProposal => {
                warn!("Proposal was invalid: {:?}", proposal_id);
                return;
            }
        }
    }
    // TODO: In the future we will receive a Fin from the network instead of the channel closing.
    // We will just send the network Fin out along with what the batcher calculates.
    let input = SendProposalContentInput { proposal_id, content: SendProposalContent::Finish };
    let response = batcher
        .send_proposal_content(input)
        .await
        .unwrap_or_else(|e| panic!("Failed to send Fin to batcher: {proposal_id:?}. {e:?}"));
    let id = match response.response {
        ProposalStatus::Finished(id) => id,
        ProposalStatus::Processing => {
            panic!("Batcher failed to return Fin after all content was sent: {:?}", proposal_id);
        }
        ProposalStatus::Aborted => {
            panic!("Unexpected abort response for proposal: {:?}", proposal_id);
        }
        ProposalStatus::InvalidProposal => {
            warn!("Proposal was invalid: {:?}", proposal_id);
            return;
        }
    };
    let proposal_content_id = BlockHash(id.state_diff_commitment.0.0);
    info!(
        "Finished validating proposal {:?}: content_id = {:?}, num_txs = {:?}, height = {:?}",
        proposal_id,
        proposal_content_id,
        content.len(),
        height
    );
    // Update valid_proposals before sending fin to avoid a race condition
    // with `get_proposal` being called before `valid_proposals` is updated.
    let mut valid_proposals = valid_proposals.lock().unwrap();
    valid_proposals.entry(height).or_default().insert(proposal_content_id, (content, proposal_id));
    if fin_sender.send(proposal_content_id).is_err() {
        // Consensus may exit early (e.g. sync).
        warn!("Failed to send proposal content id");
    }
}
