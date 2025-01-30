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
use papyrus_network::network_manager::{BroadcastTopicClient, BroadcastTopicClientTrait};
use papyrus_protobuf::consensus::{
    ConsensusMessage,
    ProposalFin,
    ProposalInit,
    ProposalPart,
    TransactionBatch,
    Vote,
    DEFAULT_VALIDATOR_ID,
};
use starknet_api::block::{
    BlockHash,
    BlockHashAndNumber,
    BlockInfo,
    BlockNumber,
    BlockTimestamp,
    GasPriceVector,
    GasPrices,
    NonzeroGasPrice,
};
use starknet_api::core::ChainId;
use starknet_api::executable_transaction::Transaction as ExecutableTransaction;
use starknet_api::transaction::{Transaction, TransactionHash};
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
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, debug_span, info, instrument, trace, warn, Instrument};

use crate::cende::{BlobParameters, CendeContext};

// TODO(Dan, Matan): Remove this once and replace with real gas prices.
const TEMPORARY_GAS_PRICES: GasPrices = GasPrices {
    eth_gas_prices: GasPriceVector {
        l1_gas_price: NonzeroGasPrice::MIN,
        l1_data_gas_price: NonzeroGasPrice::MIN,
        l2_gas_price: NonzeroGasPrice::MIN,
    },
    strk_gas_prices: GasPriceVector {
        l1_gas_price: NonzeroGasPrice::MIN,
        l1_data_gas_price: NonzeroGasPrice::MIN,
        l2_gas_price: NonzeroGasPrice::MIN,
    },
};

// {height: {proposal_id: (content, [proposal_ids])}}
// Note that multiple proposals IDs can be associated with the same content, but we only need to
// store one of them.
type HeightToIdToContent =
    BTreeMap<BlockNumber, HashMap<ProposalContentId, (Vec<ExecutableTransaction>, ProposalId)>>;
type ValidationParams = (BlockNumber, ValidatorId, Duration, mpsc::Receiver<ProposalPart>);

const CHANNEL_SIZE: usize = 100;

enum HandledProposalPart {
    Continue,
    Finished(ProposalContentId, ProposalFin),
    Failed(String),
}

// Safety margin to make sure that the batcher completes building the proposal with enough time for
// the Fin to be checked by validators.
//
// TODO(Guy): Move this to the context config.
const BUILD_PROPOSAL_MARGIN: Duration = Duration::from_millis(1000);

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
    // The active proposal refers to the proposal being validated at the current height/round.
    // Building proposals are not tracked as active, as consensus can't move on to the next
    // height/round until building is done. Context only works on proposals for the
    // current round.
    active_proposal: Option<(CancellationToken, JoinHandle<()>)>,
    // Stores proposals for future rounds until the round is reached.
    queued_proposals:
        BTreeMap<Round, (ValidationParams, oneshot::Sender<(ProposalContentId, ProposalFin)>)>,
    outbound_proposal_sender: mpsc::Sender<(u64, mpsc::Receiver<ProposalPart>)>,
    // Used to broadcast votes to other consensus nodes.
    vote_broadcast_client: BroadcastTopicClient<ConsensusMessage>,
    // Used to convert Transaction to ExecutableTransaction.
    chain_id: ChainId,
    cende_ambassador: Arc<dyn CendeContext>,
}

impl SequencerConsensusContext {
    pub fn new(
        batcher: Arc<dyn BatcherClient>,
        outbound_proposal_sender: mpsc::Sender<(u64, mpsc::Receiver<ProposalPart>)>,
        vote_broadcast_client: BroadcastTopicClient<ConsensusMessage>,
        num_validators: u64,
        chain_id: ChainId,
        cende_ambassador: Arc<dyn CendeContext>,
    ) -> Self {
        Self {
            batcher,
            outbound_proposal_sender,
            vote_broadcast_client,
            // TODO(Matan): Set the actual validator IDs (contract addresses).
            validators: (0..num_validators)
                .map(|i| ValidatorId::from(DEFAULT_VALIDATOR_ID + i))
                .collect(),
            valid_proposals: Arc::new(Mutex::new(HeightToIdToContent::new())),
            proposal_id: 0,
            current_height: None,
            current_round: 0,
            active_proposal: None,
            queued_proposals: BTreeMap::new(),
            chain_id,
            cende_ambassador,
        }
    }
}

#[async_trait]
impl ConsensusContext for SequencerConsensusContext {
    type ProposalPart = ProposalPart;

    #[instrument(level = "info", skip_all, fields(proposal_init))]
    async fn build_proposal(
        &mut self,
        proposal_init: ProposalInit,
        timeout: Duration,
    ) -> oneshot::Receiver<ProposalContentId> {
        info!("Building proposal: timeout={timeout:?}");
        let cende_write_success =
            self.cende_ambassador.write_prev_height_blob(proposal_init.height);
        // Handles interrupting an active proposal from a previous height/round
        self.set_height_and_round(proposal_init.height, proposal_init.round).await;
        let (fin_sender, fin_receiver) = oneshot::channel();

        let batcher = Arc::clone(&self.batcher);
        let valid_proposals = Arc::clone(&self.valid_proposals);

        let proposal_id = ProposalId(self.proposal_id);
        self.proposal_id += 1;
        assert!(timeout > BUILD_PROPOSAL_MARGIN);
        let batcher_timeout = chrono::Duration::from_std(timeout - BUILD_PROPOSAL_MARGIN)
            .expect("Can't convert timeout to chrono::Duration");
        let now = chrono::Utc::now();
        let build_proposal_input = ProposeBlockInput {
            proposal_id,
            // TODO: Discuss with batcher team passing std Duration instead.
            deadline: now + batcher_timeout,
            // TODO: This is not part of Milestone 1.
            retrospective_block_hash: Some(BlockHashAndNumber {
                number: BlockNumber::default(),
                hash: BlockHash::default(),
            }),
            // TODO(Dan, Matan): Fill block info.
            block_info: BlockInfo {
                block_number: proposal_init.height,
                gas_prices: TEMPORARY_GAS_PRICES,
                block_timestamp: BlockTimestamp(
                    now.timestamp().try_into().expect("Failed to convert timestamp"),
                ),
                use_kzg_da: true,
                sequencer_address: proposal_init.proposer,
            },
        };
        // TODO: Should we be returning an error?
        // I think this implies defining an error type in this crate and moving the trait definition
        // here also.
        debug!("Initiating build proposal: {build_proposal_input:?}");
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
            .send(ProposalPart::Init(proposal_init))
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
                    cende_write_success,
                    fin_sender,
                )
                .await;
            }
            .instrument(debug_span!("consensus_build_proposal")),
        );

        fin_receiver
    }

    // Note: this function does not receive ProposalInit.
    // That part is consumed by the caller, so it can know the height/round.
    #[instrument(level = "info", skip(self, timeout, content_receiver))]
    async fn validate_proposal(
        &mut self,
        proposal_init: ProposalInit,
        timeout: Duration,
        content_receiver: mpsc::Receiver<Self::ProposalPart>,
    ) -> oneshot::Receiver<(ProposalContentId, ProposalFin)> {
        info!("Validating proposal: timeout={timeout:?}");
        assert_eq!(Some(proposal_init.height), self.current_height);
        let (fin_sender, fin_receiver) = oneshot::channel();
        match proposal_init.round.cmp(&self.current_round) {
            std::cmp::Ordering::Less => fin_receiver,
            std::cmp::Ordering::Greater => {
                debug!("Queuing proposal for future round: current_round={}", self.current_round);
                self.queued_proposals.insert(
                    proposal_init.round,
                    (
                        (proposal_init.height, proposal_init.proposer, timeout, content_receiver),
                        fin_sender,
                    ),
                );
                fin_receiver
            }
            std::cmp::Ordering::Equal => {
                self.validate_current_round_proposal(
                    proposal_init.height,
                    proposal_init.proposer,
                    timeout,
                    content_receiver,
                    fin_sender,
                )
                .await;
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
        // TODO(guyn): Stream the TXs to the network.
    }

    async fn validators(&self, _height: BlockNumber) -> Vec<ValidatorId> {
        self.validators.clone()
    }

    fn proposer(&self, height: BlockNumber, round: Round) -> ValidatorId {
        let height: usize = height.0.try_into().expect("Cannot convert to usize");
        let round: usize = round.try_into().expect("Cannot convert to usize");
        *self
            .validators
            .get((height + round) % self.validators.len())
            .expect("There should be at least one validator")
    }

    async fn broadcast(&mut self, message: ConsensusMessage) -> Result<(), ConsensusError> {
        debug!("Broadcasting message: {message:?}");
        self.vote_broadcast_client.broadcast_message(message).await?;
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
        // TODO(dvir): return from the batcher's 'decision_reached' function the relevant data to
        // build a blob.
        self.batcher.decision_reached(DecisionReachedInput { proposal_id }).await.unwrap();
        // TODO(dvir): pass here real `BlobParameters` info.
        // TODO(dvir): when passing here the correct `BlobParameters`, also test that
        // `prepare_blob_for_next_height` is called with the correct parameters.
        self.cende_ambassador.prepare_blob_for_next_height(BlobParameters::default()).await;

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
        let Some(((height, validator, timeout, content), fin_sender)) = to_process else {
            return;
        };
        self.validate_current_round_proposal(height, validator, timeout, content, fin_sender).await;
    }
}

impl SequencerConsensusContext {
    #[instrument(level = "info", skip(self, timeout, content_receiver, fin_sender))]
    async fn validate_current_round_proposal(
        &mut self,
        height: BlockNumber,
        proposer: ValidatorId,
        timeout: Duration,
        mut content_receiver: mpsc::Receiver<ProposalPart>,
        fin_sender: oneshot::Sender<(ProposalContentId, ProposalFin)>,
    ) {
        info!("Validating proposal with timeout: {timeout:?}");
        let batcher = Arc::clone(&self.batcher);
        let valid_proposals = Arc::clone(&self.valid_proposals);
        let proposal_id = ProposalId(self.proposal_id);
        self.proposal_id += 1;

        let chrono_timeout =
            chrono::Duration::from_std(timeout).expect("Can't convert timeout to chrono::Duration");
        let now = chrono::Utc::now();
        let input = ValidateBlockInput {
            proposal_id,
            deadline: now + chrono_timeout,
            // TODO(Matan 3/11/2024): Add the real value of the retrospective block hash.
            retrospective_block_hash: Some(BlockHashAndNumber {
                number: BlockNumber::default(),
                hash: BlockHash::default(),
            }),
            // TODO(Dan, Matan): Fill block info.
            block_info: BlockInfo {
                block_number: height,
                gas_prices: TEMPORARY_GAS_PRICES,
                block_timestamp: BlockTimestamp(
                    now.timestamp().try_into().expect("Failed to convert timestamp"),
                ),
                use_kzg_da: true,
                sequencer_address: proposer,
            },
        };
        debug!("Initiating validate proposal: input={input:?}");
        batcher.validate_block(input).await.expect("Failed to initiate proposal validation");

        let token = CancellationToken::new();
        let token_clone = token.clone();
        let chain_id = self.chain_id.clone();
        let mut content = Vec::new();

        let handle = tokio::spawn(async move {
            let (built_block, received_fin) = loop {
                tokio::select! {
                    _ = token_clone.cancelled() => {
                        warn!("Proposal interrupted: {:?}", proposal_id);
                        batcher_abort_proposal(batcher.as_ref(), proposal_id).await;
                        return;
                    }
                    _ = tokio::time::sleep(timeout) => {
                        warn!("Validation timed out");
                        batcher_abort_proposal(batcher.as_ref(), proposal_id).await;
                        return;
                    }
                    proposal_part = content_receiver.next() => {
                        match handle_proposal_part(
                            proposal_id,
                            batcher.as_ref(),
                            proposal_part,
                            &mut content,
                            chain_id.clone()
                        ).await {
                            HandledProposalPart::Finished(built_block, received_fin) => {
                                break (built_block, received_fin);
                            }
                            HandledProposalPart::Continue => {continue;}
                            HandledProposalPart::Failed(fail_reason) => {
                                warn!("Failed to handle proposal part: {proposal_id:?}, {fail_reason}");
                                batcher_abort_proposal(batcher.as_ref(), proposal_id).await;
                                return;
                            }
                        }
                    }
                }
            };
            // Update valid_proposals before sending fin to avoid a race condition
            // with `get_proposal` being called before `valid_proposals` is updated.
            // TODO(Matan): Consider validating the ProposalFin signature here.
            let mut valid_proposals = valid_proposals.lock().unwrap();
            valid_proposals.entry(height).or_default().insert(built_block, (content, proposal_id));
            if fin_sender.send((built_block, received_fin)).is_err() {
                // Consensus may exit early (e.g. sync).
                warn!("Failed to send proposal content ids");
            }
        });
        self.active_proposal = Some((token, handle));
    }

    fn interrupt_active_proposal(&self) {
        if let Some((token, _)) = &self.active_proposal {
            token.cancel();
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
    mut cende_write_success: oneshot::Receiver<bool>,
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
                let transaction_hashes =
                    txs.iter().map(|tx| tx.tx_hash()).collect::<Vec<TransactionHash>>();
                debug!("Broadcasting proposal content: {transaction_hashes:?}");

                let transactions =
                    txs.into_iter().map(|tx| tx.into()).collect::<Vec<Transaction>>();
                trace!("Broadcasting proposal content: {transactions:?}");

                proposal_sender
                    .send(ProposalPart::Transactions(TransactionBatch { transactions }))
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

                // If the blob writing operation to Aerospike doesn't return a success status, we
                // can't finish the proposal.
                match cende_write_success.try_recv() {
                    Ok(Some(true)) => {
                        debug!("Writing blob to Aerospike completed.");
                    }
                    Ok(Some(false)) => {
                        debug!("Writing blob to Aerospike failed.");
                        return;
                    }
                    _ => {
                        debug!("Writing blob to Aerospike didn't return in time.");
                        return;
                    }
                }

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
// 1. Receives the proposal part from the network.
// 2. Pass this to the batcher.
// 3. Once finished, receive the commitment from the batcher.
async fn handle_proposal_part(
    proposal_id: ProposalId,
    batcher: &dyn BatcherClient,
    proposal_part: Option<ProposalPart>,
    content: &mut Vec<ExecutableTransaction>,
    chain_id: ChainId,
) -> HandledProposalPart {
    match proposal_part {
        None => HandledProposalPart::Failed("Failed to receive proposal content".to_string()),
        Some(ProposalPart::Transactions(TransactionBatch { transactions: txs })) => {
            let exe_txs: Vec<ExecutableTransaction> = txs
                .into_iter()
                .map(|tx| {
                    // An error means we have an invalid chain_id.
                    (tx, &chain_id)
                        .try_into()
                        .expect("Failed to convert transaction to executable_transation.")
                })
                .collect();
            content.extend_from_slice(&exe_txs[..]);
            let input = SendProposalContentInput {
                proposal_id,
                content: SendProposalContent::Txs(exe_txs),
            };
            let response = batcher.send_proposal_content(input).await.unwrap_or_else(|e| {
                panic!("Failed to send proposal content to batcher: {proposal_id:?}. {e:?}")
            });
            match response.response {
                ProposalStatus::Processing => HandledProposalPart::Continue,
                ProposalStatus::InvalidProposal => {
                    HandledProposalPart::Failed("Invalid proposal".to_string())
                }
                status => panic!("Unexpected status: for {proposal_id:?}, {status:?}"),
            }
        }
        Some(ProposalPart::Fin(ProposalFin { proposal_content_id: id })) => {
            // Output this along with the ID from batcher, to compare them.
            let input =
                SendProposalContentInput { proposal_id, content: SendProposalContent::Finish };
            let response = batcher.send_proposal_content(input).await.unwrap_or_else(|e| {
                panic!("Failed to send Fin to batcher: {proposal_id:?}. {e:?}")
            });
            let response_id = match response.response {
                ProposalStatus::Finished(id) => id,
                ProposalStatus::InvalidProposal => {
                    return HandledProposalPart::Failed("Invalid proposal".to_string());
                }
                status => panic!("Unexpected status: for {proposal_id:?}, {status:?}"),
            };
            let batcher_block_id = BlockHash(response_id.state_diff_commitment.0.0);
            info!(
                "Finished validating proposal {:?}: network_block_id: {:?}, batcher_block_id = \
                 {:?}, num_txs = {:?}",
                proposal_id,
                id,
                batcher_block_id,
                content.len(),
            );
            HandledProposalPart::Finished(batcher_block_id, ProposalFin { proposal_content_id: id })
        }
        _ => panic!("Invalid proposal part: {:?}", proposal_part),
    }
}

async fn batcher_abort_proposal(batcher: &dyn BatcherClient, proposal_id: ProposalId) {
    let input = SendProposalContentInput { proposal_id, content: SendProposalContent::Abort };
    batcher
        .send_proposal_content(input)
        .await
        .unwrap_or_else(|e| panic!("Failed to send Abort to batcher: {proposal_id:?}. {e:?}"));
}
