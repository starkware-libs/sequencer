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
use futures::{FutureExt, SinkExt, StreamExt};
use papyrus_consensus::types::{
    ConsensusContext,
    ConsensusError,
    ProposalContentId,
    Round,
    ValidatorId,
};
use papyrus_network::network_manager::{BroadcastTopicClient, BroadcastTopicClientTrait};
use papyrus_protobuf::consensus::{
    HeightAndRound,
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
    BlockHeaderWithoutHash,
    BlockInfo,
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPricePerToken,
    GasPriceVector,
    GasPrices,
    NonzeroGasPrice,
};
use starknet_api::core::{ChainId, ContractAddress, SequencerContractAddress};
use starknet_api::executable_transaction::Transaction as ExecutableTransaction;
use starknet_api::transaction::{Transaction, TransactionHash};
use starknet_batcher_types::batcher_types::{
    DecisionReachedInput,
    DecisionReachedResponse,
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
use starknet_state_sync_types::communication::SharedStateSyncClient;
use starknet_state_sync_types::state_sync_types::SyncBlock;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;
use tracing::{debug, error_span, info, instrument, trace, warn, Instrument};

use crate::cende::{BlobParameters, CendeContext};
use crate::fee_market::calculate_next_base_gas_price;
use crate::versioned_constants::VersionedConstants;

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
    Invalid,
    Finished(ProposalContentId, ProposalFin),
    Failed(String),
}

// Safety margin to make sure that the batcher completes building the proposal with enough time for
// the Fin to be checked by validators.
//
// TODO(Guy): Move this to the context config.
const BUILD_PROPOSAL_MARGIN: Duration = Duration::from_millis(1000);
// When validating a proposal the Context is responsible for timeout handling. The Batcher though
// has a timeout as a defensive measure to make sure the proposal doesn't live forever if the
// Context crashes or has a bug.
const VALIDATE_PROPOSAL_MARGIN: Duration = Duration::from_secs(10);

pub struct SequencerConsensusContext {
    state_sync_client: SharedStateSyncClient,
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
    outbound_proposal_sender: mpsc::Sender<(HeightAndRound, mpsc::Receiver<ProposalPart>)>,
    // Used to broadcast votes to other consensus nodes.
    vote_broadcast_client: BroadcastTopicClient<Vote>,
    // Used to convert Transaction to ExecutableTransaction.
    chain_id: ChainId,
    cende_ambassador: Arc<dyn CendeContext>,
    // The next block's l2 gas price, calculated based on EIP-1559, used for building and
    // validating proposals.
    l2_gas_price: u64,
}

impl SequencerConsensusContext {
    pub fn new(
        state_sync_client: SharedStateSyncClient,
        batcher: Arc<dyn BatcherClient>,
        outbound_proposal_sender: mpsc::Sender<(HeightAndRound, mpsc::Receiver<ProposalPart>)>,
        vote_broadcast_client: BroadcastTopicClient<Vote>,
        num_validators: u64,
        chain_id: ChainId,
        cende_ambassador: Arc<dyn CendeContext>,
    ) -> Self {
        Self {
            state_sync_client,
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
            l2_gas_price: VersionedConstants::latest_constants().min_gas_price,
        }
    }

    fn gas_prices(&self) -> GasPrices {
        GasPrices {
            strk_gas_prices: GasPriceVector {
                l2_gas_price: NonzeroGasPrice::new(self.l2_gas_price.into())
                    .expect("Failed to convert l2_gas_price to NonzeroGasPrice, should not be 0."),
                ..TEMPORARY_GAS_PRICES.strk_gas_prices
            },
            ..TEMPORARY_GAS_PRICES
        }
    }
}

#[async_trait]
impl ConsensusContext for SequencerConsensusContext {
    type ProposalPart = ProposalPart;

    #[instrument(skip_all)]
    async fn build_proposal(
        &mut self,
        proposal_init: ProposalInit,
        timeout: Duration,
    ) -> oneshot::Receiver<ProposalContentId> {
        let cende_write_success = AbortOnDropHandle::new(
            self.cende_ambassador.write_prev_height_blob(proposal_init.height),
        );
        // Handles interrupting an active proposal from a previous height/round
        self.set_height_and_round(proposal_init.height, proposal_init.round).await;

        let (fin_sender, fin_receiver) = oneshot::channel();
        let batcher = Arc::clone(&self.batcher);
        let valid_proposals = Arc::clone(&self.valid_proposals);
        let proposal_id = ProposalId(self.proposal_id);
        self.proposal_id += 1;
        assert!(timeout > BUILD_PROPOSAL_MARGIN);
        let (proposal_sender, proposal_receiver) = mpsc::channel(CHANNEL_SIZE);
        let stream_id = HeightAndRound(proposal_init.height.0, proposal_init.round);
        self.outbound_proposal_sender
            .send((stream_id, proposal_receiver))
            .await
            .expect("Failed to send proposal receiver");
        let gas_prices = self.gas_prices();

        info!(?proposal_init, ?timeout, %proposal_id, "Building proposal");
        let handle = tokio::spawn(
            async move {
                build_proposal(
                    timeout,
                    proposal_init,
                    proposal_sender,
                    fin_sender,
                    batcher,
                    valid_proposals,
                    proposal_id,
                    cende_write_success,
                    gas_prices,
                )
                .await;
            }
            .instrument(
                error_span!("consensus_build_proposal", %proposal_id, round=proposal_init.round),
            ),
        );
        assert!(self.active_proposal.is_none());
        // The cancellation token is unused by the spawned build.
        self.active_proposal = Some((CancellationToken::new(), handle));

        fin_receiver
    }

    // Note: this function does not receive ProposalInit.
    // That part is consumed by the caller, so it can know the height/round.
    #[instrument(skip_all)]
    async fn validate_proposal(
        &mut self,
        proposal_init: ProposalInit,
        timeout: Duration,
        content_receiver: mpsc::Receiver<Self::ProposalPart>,
    ) -> oneshot::Receiver<(ProposalContentId, ProposalFin)> {
        assert_eq!(Some(proposal_init.height), self.current_height);
        let (fin_sender, fin_receiver) = oneshot::channel();
        match proposal_init.round.cmp(&self.current_round) {
            std::cmp::Ordering::Less => {
                trace!("Dropping proposal from past round");
                fin_receiver
            }
            std::cmp::Ordering::Greater => {
                trace!("Queueing proposal for future round.");
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
        info!(?id, ?init, "Reproposing.");
        let height = init.height;
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

    async fn broadcast(&mut self, message: Vote) -> Result<(), ConsensusError> {
        trace!("Broadcasting message: {message:?}");
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

        self.interrupt_active_proposal().await;
        let proposal_id;
        let transactions;
        {
            let mut proposals = self
                .valid_proposals
                .lock()
                .expect("Lock on active proposals was poisoned due to a previous panic");
            (transactions, proposal_id) =
                proposals.get(&BlockNumber(height)).unwrap().get(&block).unwrap().clone();

            proposals.retain(|&h, _| h > BlockNumber(height));
        }
        // TODO(dvir): return from the batcher's 'decision_reached' function the relevant data to
        // build a blob.
        let DecisionReachedResponse { state_diff, l2_gas_used } = self
            .batcher
            .decision_reached(DecisionReachedInput { proposal_id })
            .await
            .expect("Failed to get state diff.");

        let transaction_hashes =
            transactions.iter().map(|tx| tx.tx_hash()).collect::<Vec<TransactionHash>>();
        // TODO(Asmaa/Eitan): update with the correct values.
        let l1_gas_price =
            GasPricePerToken { price_in_fri: GasPrice(1), price_in_wei: GasPrice(1) };
        let l1_data_gas_price =
            GasPricePerToken { price_in_fri: GasPrice(1), price_in_wei: GasPrice(1) };
        let l2_gas_price =
            GasPricePerToken { price_in_fri: GasPrice(1), price_in_wei: GasPrice(1) };
        let sequencer = SequencerContractAddress(ContractAddress::from(123_u128));
        let block_header_without_hash = BlockHeaderWithoutHash {
            block_number: BlockNumber(height),
            l1_gas_price,
            l1_data_gas_price,
            l2_gas_price,
            sequencer,
            ..Default::default()
        };
        let sync_block = SyncBlock {
            state_diff: state_diff.clone(),
            transaction_hashes,
            block_header_without_hash,
        };
        let state_sync_client = self.state_sync_client.clone();
        // `add_new_block` returns immediately, it doesn't wait for sync to fully process the block.
        state_sync_client.add_new_block(sync_block).await.expect("Failed to add new block.");

        // TODO(dvir): pass here real `BlobParameters` info.
        // TODO(dvir): when passing here the correct `BlobParameters`, also test that
        // `prepare_blob_for_next_height` is called with the correct parameters.
        self.cende_ambassador
            .prepare_blob_for_next_height(BlobParameters {
                // TODO(dvir): use the real `BlockInfo` when consensus will save it.
                block_info: BlockInfo { block_number: BlockNumber(height), ..Default::default() },
                state_diff,
                transactions,
                // TODO(Yael): add the execution_infos to DecisionReachedResponse.
                execution_infos: Default::default(),
            })
            .await;

        self.l2_gas_price = calculate_next_base_gas_price(
            self.l2_gas_price,
            l2_gas_used.0,
            VersionedConstants::latest_constants().max_block_size / 2,
        );

        Ok(())
    }

    async fn try_sync(&mut self, height: BlockNumber) -> bool {
        let sync_block = self.state_sync_client.get_block(height).await;
        if let Ok(Some(sync_block)) = sync_block {
            self.interrupt_active_proposal().await;
            self.batcher.add_sync_block(sync_block).await.unwrap();
            return true;
        }
        false
    }

    async fn set_height_and_round(&mut self, height: BlockNumber, round: Round) {
        if self.current_height.map(|h| height > h).unwrap_or(true) {
            self.current_height = Some(height);
            assert_eq!(round, 0);
            self.current_round = round;
            self.queued_proposals.clear();
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
        self.interrupt_active_proposal().await;
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
    async fn validate_current_round_proposal(
        &mut self,
        height: BlockNumber,
        proposer: ValidatorId,
        timeout: Duration,
        content_receiver: mpsc::Receiver<ProposalPart>,
        fin_sender: oneshot::Sender<(ProposalContentId, ProposalFin)>,
    ) {
        let cancel_token = CancellationToken::new();
        let cancel_token_clone = cancel_token.clone();
        let batcher = Arc::clone(&self.batcher);
        let valid_proposals = Arc::clone(&self.valid_proposals);
        let chain_id = self.chain_id.clone();
        let proposal_id = ProposalId(self.proposal_id);
        self.proposal_id += 1;
        let gas_prices = self.gas_prices();

        info!(?timeout, %proposal_id, %proposer, round=self.current_round, "Validating proposal.");

        let handle = tokio::spawn(
            async move {
                validate_proposal(
                    chain_id,
                    proposal_id,
                    batcher.as_ref(),
                    height,
                    proposer,
                    timeout,
                    valid_proposals,
                    content_receiver,
                    fin_sender,
                    cancel_token_clone,
                    gas_prices,
                )
                .await
            }
            .instrument(
                error_span!("consensus_validate_proposal", %proposal_id, round=self.current_round),
            ),
        );
        self.active_proposal = Some((cancel_token, handle));
    }

    async fn interrupt_active_proposal(&mut self) {
        if let Some((token, handle)) = self.active_proposal.take() {
            token.cancel();
            handle.await.expect("Proposal task failed");
        }
    }
}

// Handles building a new proposal without blocking consensus:
#[allow(clippy::too_many_arguments)]
async fn build_proposal(
    timeout: Duration,
    proposal_init: ProposalInit,
    mut proposal_sender: mpsc::Sender<ProposalPart>,
    fin_sender: oneshot::Sender<ProposalContentId>,
    batcher: Arc<dyn BatcherClient>,
    valid_proposals: Arc<Mutex<HeightToIdToContent>>,
    proposal_id: ProposalId,
    cende_write_success: AbortOnDropHandle<bool>,
    gas_prices: GasPrices,
) {
    initialize_build(proposal_id, &proposal_init, timeout, batcher.as_ref(), gas_prices).await;
    proposal_sender
        .send(ProposalPart::Init(proposal_init))
        .await
        .expect("Failed to send proposal init");

    let Some((proposal_content_id, content)) =
        get_proposal_content(proposal_id, batcher.as_ref(), proposal_sender, cende_write_success)
            .await
    else {
        return;
    };

    // Update valid_proposals before sending fin to avoid a race condition
    // with `repropose` being called before `valid_proposals` is updated.
    let mut valid_proposals = valid_proposals.lock().expect("Lock was poisoned");
    valid_proposals
        .entry(proposal_init.height)
        .or_default()
        .insert(proposal_content_id, (content, proposal_id));
    if fin_sender.send(proposal_content_id).is_err() {
        // Consensus may exit early (e.g. sync).
        warn!("Failed to send proposal content id");
    }
}

async fn initialize_build(
    proposal_id: ProposalId,
    proposal_init: &ProposalInit,
    timeout: Duration,
    batcher: &dyn BatcherClient,
    gas_prices: GasPrices,
) {
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
            gas_prices,
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
    batcher.propose_block(build_proposal_input).await.expect("Failed to initiate proposal build");
}

// 1. Receive chunks of content from the batcher.
// 2. Forward these to the stream handler to be streamed out to the network.
// 3. Once finished, receive the commitment from the batcher.
async fn get_proposal_content(
    proposal_id: ProposalId,
    batcher: &dyn BatcherClient,
    mut proposal_sender: mpsc::Sender<ProposalPart>,
    cende_write_success: AbortOnDropHandle<bool>,
) -> Option<(ProposalContentId, Vec<ExecutableTransaction>)> {
    let mut content = Vec::new();
    loop {
        // We currently want one part of the node failing to cause all components to fail. If this
        // changes, we can simply return None and consider this as a failed proposal which consensus
        // should support.
        let response = batcher
            .get_proposal_content(GetProposalContentInput { proposal_id })
            .await
            .expect("Failed to get proposal content");

        match response.content {
            GetProposalContent::Txs(txs) => {
                content.extend_from_slice(&txs[..]);
                // TODO(matan): Make sure this isn't too large for a single proto message.
                debug!(
                    hashes = ?txs.iter().map(|tx| tx.tx_hash()).collect::<Vec<TransactionHash>>(),
                    "Sending transaction batch with {} txs.",
                    txs.len()
                );
                let transactions =
                    txs.into_iter().map(|tx| tx.into()).collect::<Vec<Transaction>>();
                trace!(?transactions, "Sending transaction batch with {} txs.", transactions.len());
                proposal_sender
                    .send(ProposalPart::Transactions(TransactionBatch { transactions }))
                    .await
                    .expect("Failed to broadcast proposal content");
            }
            GetProposalContent::Finished(id) => {
                let proposal_content_id = BlockHash(id.state_diff_commitment.0.0);
                info!(?proposal_content_id, num_txs = content.len(), "Finished building proposal",);

                // If the blob writing operation to Aerospike doesn't return a success status, we
                // can't finish the proposal.
                match cende_write_success.now_or_never() {
                    Some(Ok(true)) => {
                        debug!("Writing blob to Aerospike completed.");
                    }
                    Some(Ok(false)) => {
                        warn!("Writing blob to Aerospike failed.");
                        return None;
                    }
                    Some(Err(e)) => {
                        warn!("Writing blob to Aerospike failed. Error: {e:?}");
                        return None;
                    }
                    None => {
                        warn!("Writing blob to Aerospike didn't return in time.");
                        return None;
                    }
                }

                proposal_sender
                    .send(ProposalPart::Fin(ProposalFin { proposal_content_id }))
                    .await
                    .expect("Failed to broadcast proposal fin");
                return Some((proposal_content_id, content));
            }
        }
    }
}

// TODO(Arni): Remove the clippy when switch to ProposalInit.
#[allow(clippy::too_many_arguments)]
async fn validate_proposal(
    chain_id: ChainId,
    proposal_id: ProposalId,
    batcher: &dyn BatcherClient,
    height: BlockNumber,
    proposer: ValidatorId,
    timeout: Duration,
    valid_proposals: Arc<Mutex<HeightToIdToContent>>,
    mut content_receiver: mpsc::Receiver<ProposalPart>,
    fin_sender: oneshot::Sender<(ProposalContentId, ProposalFin)>,
    cancel_token: CancellationToken,
    gas_prices: GasPrices,
) {
    initiate_validation(batcher, proposal_id, height, proposer, timeout, gas_prices).await;

    let mut content = Vec::new();
    let deadline = tokio::time::Instant::now() + timeout;
    let (built_block, received_fin) = loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                warn!("Proposal interrupted");
                batcher_abort_proposal(batcher, proposal_id).await;
                return;
            }
            _ = tokio::time::sleep_until(deadline) => {
                warn!("Validation timed out.");
                batcher_abort_proposal(batcher, proposal_id).await;
                return;
            }
            proposal_part = content_receiver.next() => {
                match handle_proposal_part(
                    proposal_id,
                    batcher,
                    proposal_part,
                    &mut content,
                    chain_id.clone()
                ).await {
                    HandledProposalPart::Finished(built_block, received_fin) => {
                        break (built_block, received_fin);
                    }
                    HandledProposalPart::Continue => {continue;}
                    HandledProposalPart::Invalid => {
                        warn!("Invalid proposal.");
                        // No need to abort since the Batcher is the source of this info.
                        return;
                    }
                    HandledProposalPart::Failed(fail_reason) => {
                        warn!("Failed to handle proposal part. {fail_reason}");
                        batcher_abort_proposal(batcher, proposal_id).await;
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
}

async fn initiate_validation(
    batcher: &dyn BatcherClient,
    proposal_id: ProposalId,
    height: BlockNumber,
    proposer: ValidatorId,
    timeout: Duration,
    gas_prices: GasPrices,
) {
    // Initiate the validation.
    let chrono_timeout = chrono::Duration::from_std(timeout + VALIDATE_PROPOSAL_MARGIN)
        .expect("Can't convert timeout to chrono::Duration");
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
            gas_prices,
            block_timestamp: BlockTimestamp(
                now.timestamp().try_into().expect("Failed to convert timestamp"),
            ),
            use_kzg_da: true,
            sequencer_address: proposer,
        },
    };
    debug!("Initiating validate proposal: input={input:?}");
    batcher.validate_block(input).await.expect("Failed to initiate proposal validation");
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
            debug!("Received transaction batch with {} txs", txs.len());
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
                ProposalStatus::InvalidProposal => HandledProposalPart::Invalid,
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
                network_block_id = ?id,
                ?batcher_block_id,
                num_txs = %content.len(),
                "Finished validating proposal."
            );
            HandledProposalPart::Finished(batcher_block_id, ProposalFin { proposal_content_id: id })
        }
        // TODO(Asmaa): Handle invalid proposal part by aborting the proposal, not the node.
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
