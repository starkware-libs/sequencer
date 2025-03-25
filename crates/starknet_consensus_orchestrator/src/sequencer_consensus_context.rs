//! Implementation of the ConsensusContext interface for running the sequencer.
//!
//! It connects to the Batcher who is responsible for building/validating blocks.
#[cfg(test)]
#[path = "sequencer_consensus_context_test.rs"]
mod sequencer_consensus_context_test;

use std::cmp::max;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::channel::{mpsc, oneshot};
use futures::{FutureExt, SinkExt, StreamExt};
use papyrus_network::network_manager::{BroadcastTopicClient, BroadcastTopicClientTrait};
use papyrus_protobuf::consensus::{
    ConsensusBlockInfo,
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
    BlockNumber,
    BlockTimestamp,
    GasPrice,
    GasPricePerToken,
    GasPriceVector,
    GasPrices,
    NonzeroGasPrice,
};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::{ContractAddress, SequencerContractAddress};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::transaction::TransactionHash;
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
use starknet_batcher_types::communication::{
    BatcherClient,
    BatcherClientError,
    BatcherClientResult,
};
use starknet_class_manager_types::transaction_converter::{
    TransactionConverter,
    TransactionConverterTrait,
};
use starknet_class_manager_types::SharedClassManagerClient;
use starknet_consensus::types::{
    ConsensusContext,
    ConsensusError,
    ProposalCommitment,
    Round,
    ValidatorId,
};
use starknet_l1_gas_price_types::errors::EthToStrkOracleClientError;
use starknet_l1_gas_price_types::EthToStrkOracleClientTrait;
use starknet_state_sync_types::communication::SharedStateSyncClient;
use starknet_state_sync_types::state_sync_types::SyncBlock;
use starknet_types_core::felt::Felt;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;
use tracing::{debug, error, error_span, info, instrument, trace, warn, Instrument};

use crate::cende::{BlobParameters, CendeContext};
use crate::config::ContextConfig;
use crate::fee_market::{calculate_next_base_gas_price, FeeMarketInfo};
use crate::metrics::{CONSENSUS_NUM_BATCHES_IN_PROPOSAL, CONSENSUS_NUM_TXS_IN_PROPOSAL};
use crate::orchestrator_versioned_constants::VersionedConstants;

// Contains parameters required for validating block info.
#[derive(Clone, Debug)]
struct BlockInfoValidation {
    height: BlockNumber,
    block_timestamp_window: u64,
    last_block_timestamp: Option<u64>,
    l1_da_mode: L1DataAvailabilityMode,
}

const EMPTY_BLOCK_COMMITMENT: BlockHash = BlockHash(Felt::ONE);

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

// {height: {proposal_commitment: (block_info, content, [proposal_ids])}}
// Note that multiple proposals IDs can be associated with the same content, but we only need to
// store one of them.
type HeightToIdToContent = BTreeMap<
    BlockNumber,
    HashMap<
        ProposalCommitment,
        (ConsensusBlockInfo, Vec<Vec<InternalConsensusTransaction>>, ProposalId),
    >,
>;
type ValidationParams = (BlockNumber, ValidatorId, Duration, mpsc::Receiver<ProposalPart>);
type BuildProposalResult<T> = Result<T, BuildProposalError>;

enum HandledProposalPart {
    Continue,
    Invalid,
    Finished(ProposalCommitment, ProposalFin),
    Failed(String),
}

#[derive(Debug, thiserror::Error)]
enum BuildProposalError {
    #[error("Batcher error: {0}")]
    Batcher(#[from] BatcherClientError),

    #[error("EthToStrkOracle error: {0}")]
    EthToStrkOracle(#[from] EthToStrkOracleClientError),
}

pub struct SequencerConsensusContext {
    config: ContextConfig,
    // TODO(Shahak): change this into a dynamic TransactionConverterTrait.
    transaction_converter: TransactionConverter,
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
        BTreeMap<Round, (ValidationParams, oneshot::Sender<(ProposalCommitment, ProposalFin)>)>,
    outbound_proposal_sender: mpsc::Sender<(HeightAndRound, mpsc::Receiver<ProposalPart>)>,
    // Used to broadcast votes to other consensus nodes.
    vote_broadcast_client: BroadcastTopicClient<Vote>,
    cende_ambassador: Arc<dyn CendeContext>,
    // TODO(Asmaa): remove the option once e2e integration tests are updated with fake http server.
    eth_to_strk_oracle_client: Option<Arc<dyn EthToStrkOracleClientTrait>>,
    // The next block's l2 gas price, calculated based on EIP-1559, used for building and
    // validating proposals.
    l2_gas_price: u64,
    l1_da_mode: L1DataAvailabilityMode,
    last_block_timestamp: Option<u64>,
}

impl SequencerConsensusContext {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: ContextConfig,
        class_manager_client: SharedClassManagerClient,
        state_sync_client: SharedStateSyncClient,
        batcher: Arc<dyn BatcherClient>,
        outbound_proposal_sender: mpsc::Sender<(HeightAndRound, mpsc::Receiver<ProposalPart>)>,
        vote_broadcast_client: BroadcastTopicClient<Vote>,
        cende_ambassador: Arc<dyn CendeContext>,
        eth_to_strk_oracle_client: Option<Arc<dyn EthToStrkOracleClientTrait>>,
    ) -> Self {
        let chain_id = config.chain_id.clone();
        let num_validators = config.num_validators;
        let l1_da_mode = if config.l1_da_mode {
            L1DataAvailabilityMode::Blob
        } else {
            L1DataAvailabilityMode::Calldata
        };
        Self {
            config,
            transaction_converter: TransactionConverter::new(class_manager_client, chain_id),
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
            cende_ambassador,
            eth_to_strk_oracle_client,
            l2_gas_price: VersionedConstants::latest_constants().min_gas_price,
            l1_da_mode,
            last_block_timestamp: None,
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

struct ProposalBuildArguments {
    batcher_timeout: Duration,
    proposal_init: ProposalInit,
    l1_da_mode: L1DataAvailabilityMode,
    proposal_sender: mpsc::Sender<ProposalPart>,
    fin_sender: oneshot::Sender<ProposalCommitment>,
    batcher: Arc<dyn BatcherClient>,
    eth_to_strk_oracle_client: Option<Arc<dyn EthToStrkOracleClientTrait>>,
    valid_proposals: Arc<Mutex<HeightToIdToContent>>,
    proposal_id: ProposalId,
    cende_write_success: AbortOnDropHandle<bool>,
    gas_prices: GasPrices,
    transaction_converter: TransactionConverter,
    builder_address: ContractAddress,
    cancel_token: CancellationToken,
}

struct ProposalValidateArguments {
    block_info_validation: BlockInfoValidation,
    proposal_id: ProposalId,
    batcher: Arc<dyn BatcherClient>,
    timeout: Duration,
    batcher_timeout_margin: Duration,
    valid_proposals: Arc<Mutex<HeightToIdToContent>>,
    content_receiver: mpsc::Receiver<ProposalPart>,
    fin_sender: oneshot::Sender<(ProposalCommitment, ProposalFin)>,
    cancel_token: CancellationToken,
    transaction_converter: TransactionConverter,
}

#[async_trait]
impl ConsensusContext for SequencerConsensusContext {
    type ProposalPart = ProposalPart;

    #[instrument(skip_all)]
    async fn build_proposal(
        &mut self,
        proposal_init: ProposalInit,
        timeout: Duration,
    ) -> oneshot::Receiver<ProposalCommitment> {
        // TODO(dvir): consider start writing the blob in `decision_reached`, to reduce transactions
        // finality time. Use this option only for one special sequencer that is the same cluster as
        // the recorder.
        let cende_write_success = AbortOnDropHandle::new(
            self.cende_ambassador.write_prev_height_blob(proposal_init.height),
        );
        // Handles interrupting an active proposal from a previous height/round
        self.set_height_and_round(proposal_init.height, proposal_init.round).await;

        let (fin_sender, fin_receiver) = oneshot::channel();
        let batcher = Arc::clone(&self.batcher);
        let eth_to_strk_oracle_client = self.eth_to_strk_oracle_client.clone();
        let valid_proposals = Arc::clone(&self.valid_proposals);
        let proposal_id = ProposalId(self.proposal_id);
        self.proposal_id += 1;
        assert!(timeout > self.config.build_proposal_margin);
        let (proposal_sender, proposal_receiver) = mpsc::channel(self.config.proposal_buffer_size);
        let l1_da_mode = self.l1_da_mode;
        let stream_id = HeightAndRound(proposal_init.height.0, proposal_init.round);
        self.outbound_proposal_sender
            .send((stream_id, proposal_receiver))
            .await
            .expect("Failed to send proposal receiver");
        let gas_prices = self.gas_prices();
        let transaction_converter = self.transaction_converter.clone();
        let builder_address = self.config.builder_address;

        info!(?proposal_init, ?timeout, %proposal_id, "Building proposal");
        let batcher_timeout = timeout - self.config.build_proposal_margin;
        let cancel_token = CancellationToken::new();
        let cancel_token_clone = cancel_token.clone();
        let handle = tokio::spawn(
            async move {
                build_proposal(ProposalBuildArguments {
                    batcher_timeout,
                    proposal_init,
                    l1_da_mode,
                    proposal_sender,
                    fin_sender,
                    batcher,
                    eth_to_strk_oracle_client,
                    valid_proposals,
                    proposal_id,
                    cende_write_success,
                    gas_prices,
                    transaction_converter,
                    builder_address,
                    cancel_token,
                })
                .await;
            }
            .instrument(
                error_span!("consensus_build_proposal", %proposal_id, round=proposal_init.round),
            ),
        );
        assert!(self.active_proposal.is_none());
        self.active_proposal = Some((cancel_token_clone, handle));

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
    ) -> oneshot::Receiver<(ProposalCommitment, ProposalFin)> {
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
                let block_info_validation = BlockInfoValidation {
                    height: proposal_init.height,
                    block_timestamp_window: self.config.block_timestamp_window,
                    last_block_timestamp: self.last_block_timestamp,
                    l1_da_mode: self.l1_da_mode,
                };
                self.validate_current_round_proposal(
                    block_info_validation,
                    proposal_init.proposer,
                    timeout,
                    self.config.validate_proposal_margin,
                    content_receiver,
                    fin_sender,
                )
                .await;
                fin_receiver
            }
        }
    }

    async fn repropose(&mut self, id: ProposalCommitment, init: ProposalInit) {
        info!(?id, ?init, "Reproposing.");
        let height = init.height;
        let (block_info, txs, _) = self
            .valid_proposals
            .lock()
            .expect("Lock on active proposals was poisoned due to a previous panic")
            .get(&height)
            .unwrap_or_else(|| panic!("No proposals found for height {height}"))
            .get(&id)
            .unwrap_or_else(|| panic!("No proposal found for height {height} and id {id}"))
            .clone();

        let transaction_converter = self.transaction_converter.clone();
        let mut outbound_proposal_sender = self.outbound_proposal_sender.clone();
        let channel_size = self.config.proposal_buffer_size;
        tokio::spawn(
            async move {
                let (mut proposal_sender, proposal_receiver) = mpsc::channel(channel_size);
                let stream_id = HeightAndRound(height.0, init.round);
                outbound_proposal_sender
                    .send((stream_id, proposal_receiver))
                    .await
                    .expect("Failed to send proposal receiver");
                proposal_sender
                    .send(ProposalPart::Init(init))
                    .await
                    .expect("Failed to send proposal init");
                proposal_sender
                    .send(ProposalPart::BlockInfo(block_info.clone()))
                    .await
                    .expect("Failed to send block info");
                for batch in txs.iter() {
                    let transactions = futures::future::join_all(batch.iter().map(|tx| {
                        transaction_converter
                            .convert_internal_consensus_tx_to_consensus_tx(tx.clone())
                    }))
                    .await
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()
                    .expect("Failed converting transaction during repropose");

                    proposal_sender
                        .send(ProposalPart::Transactions(TransactionBatch { transactions }))
                        .await
                        .expect("Failed to broadcast proposal content");
                }
                proposal_sender
                    .send(ProposalPart::Fin(ProposalFin { proposal_commitment: id }))
                    .await
                    .expect("Failed to broadcast proposal fin");
            }
            .instrument(error_span!("consensus_repropose", round = init.round)),
        );
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
        block: ProposalCommitment,
        precommits: Vec<Vote>,
    ) -> Result<(), ConsensusError> {
        let height = precommits[0].height;
        info!("Finished consensus for height: {height}. Agreed on block: {:#064x}", block.0);

        self.interrupt_active_proposal().await;
        let proposal_id;
        let transactions;
        let block_info;
        {
            let mut proposals = self
                .valid_proposals
                .lock()
                .expect("Lock on active proposals was poisoned due to a previous panic");
            (block_info, transactions, proposal_id) =
                proposals.get(&BlockNumber(height)).unwrap().get(&block).unwrap().clone();

            proposals.retain(|&h, _| h > BlockNumber(height));
        }
        let transactions = transactions.concat();
        // TODO(dvir): return from the batcher's 'decision_reached' function the relevant data to
        // build a blob.
        let DecisionReachedResponse { state_diff, l2_gas_used, central_objects } = self
            .batcher
            .decision_reached(DecisionReachedInput { proposal_id })
            .await
            .expect("Failed to get state diff.");

        let next_l2_gas_price = calculate_next_base_gas_price(
            self.l2_gas_price,
            l2_gas_used.0,
            VersionedConstants::latest_constants().max_block_size / 2,
        );

        let transaction_hashes =
            transactions.iter().map(|tx| tx.tx_hash()).collect::<Vec<TransactionHash>>();

        let cende_block_info = convert_to_sn_api_block_info(&block_info);
        let l1_gas_price = GasPricePerToken {
            price_in_fri: cende_block_info.gas_prices.strk_gas_prices.l1_gas_price.get(),
            price_in_wei: GasPrice(block_info.l1_gas_price_wei),
        };
        let l1_data_gas_price = GasPricePerToken {
            price_in_fri: cende_block_info.gas_prices.strk_gas_prices.l1_data_gas_price.get(),
            price_in_wei: GasPrice(block_info.l1_data_gas_price_wei),
        };
        let sequencer = SequencerContractAddress(block_info.builder);

        let block_header_without_hash = BlockHeaderWithoutHash {
            block_number: BlockNumber(height),
            l1_gas_price,
            l1_data_gas_price,
            l2_gas_price: GasPricePerToken {
                price_in_fri: GasPrice(__self.l2_gas_price.into()),
                // TODO(guy.f): We shouldn't be sending price_in_wei for l2, should we remove
                // if from the Token or not use GasPriceToken here.
                price_in_wei: GasPrice(1),
            },
            l2_gas_consumed: l2_gas_used.0,
            next_l2_gas_price,
            sequencer,
            timestamp: BlockTimestamp(block_info.timestamp),
            l1_da_mode: block_info.l1_da_mode,
            // TODO(guy.f): Figure out where/if to get the values below from and fill them.
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

        self.l2_gas_price = next_l2_gas_price;

        // TODO(dvir): pass here real `BlobParameters` info.
        // TODO(dvir): when passing here the correct `BlobParameters`, also test that
        // `prepare_blob_for_next_height` is called with the correct parameters.
        self.last_block_timestamp = Some(block_info.timestamp);
        let _ = self
            .cende_ambassador
            .prepare_blob_for_next_height(BlobParameters {
                block_info: cende_block_info,
                state_diff,
                compressed_state_diff: central_objects.compressed_state_diff,
                transactions,
                execution_infos: central_objects.execution_infos,
                bouncer_weights: central_objects.bouncer_weights,
                fee_market_info: FeeMarketInfo {
                    l2_gas_consumed: l2_gas_used.0,
                    next_l2_gas_price: self.l2_gas_price,
                },
            })
            .await
            .inspect_err(|e| {
                error!("Failed to prepare blob for next height: {e:?}");
            });

        Ok(())
    }

    async fn try_sync(&mut self, height: BlockNumber) -> bool {
        let sync_block = match self.state_sync_client.get_block(height).await {
            Err(e) => {
                error!("Sync returned an error: {e:?}");
                return false;
            }
            Ok(None) => return false,
            Ok(Some(block)) => block,
        };
        // May be default for blocks older than 0.14.0, ensure min gas price is met.
        self.l2_gas_price = max(
            sync_block.block_header_without_hash.next_l2_gas_price,
            VersionedConstants::latest_constants().min_gas_price,
        );
        // TODO(Asmaa): validate starknet_version and parent_hash when they are stored.
        let block_number = sync_block.block_header_without_hash.block_number;
        let timestamp = sync_block.block_header_without_hash.timestamp;
        let now: u64 =
            chrono::Utc::now().timestamp().try_into().expect("Failed to convert timestamp to u64");
        if !(block_number == height
            && timestamp.0 >= self.last_block_timestamp.unwrap_or(0)
            && timestamp.0 <= now + self.config.block_timestamp_window)
        {
            warn!(
                "Invalid block info: expected block number {}, got {}, expected timestamp range \
                 [{}, {}], got {}",
                height,
                block_number,
                self.last_block_timestamp.unwrap_or(0),
                now + self.config.block_timestamp_window,
                timestamp.0,
            );
            return false;
        }
        self.interrupt_active_proposal().await;
        self.batcher.add_sync_block(sync_block).await.unwrap();
        true
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
        let block_info_validation = BlockInfoValidation {
            height,
            block_timestamp_window: self.config.block_timestamp_window,
            last_block_timestamp: self.last_block_timestamp,
            l1_da_mode: self.l1_da_mode,
        };
        self.validate_current_round_proposal(
            block_info_validation,
            validator,
            timeout,
            self.config.validate_proposal_margin,
            content,
            fin_sender,
        )
        .await;
    }
}

impl SequencerConsensusContext {
    async fn validate_current_round_proposal(
        &mut self,
        block_info_validation: BlockInfoValidation,
        proposer: ValidatorId,
        timeout: Duration,
        batcher_timeout_margin: Duration,
        content_receiver: mpsc::Receiver<ProposalPart>,
        fin_sender: oneshot::Sender<(ProposalCommitment, ProposalFin)>,
    ) {
        let cancel_token = CancellationToken::new();
        let cancel_token_clone = cancel_token.clone();
        let batcher = Arc::clone(&self.batcher);
        let transaction_converter = self.transaction_converter.clone();
        let valid_proposals = Arc::clone(&self.valid_proposals);
        let proposal_id = ProposalId(self.proposal_id);
        self.proposal_id += 1;

        info!(?timeout, %proposal_id, %proposer, round=self.current_round, "Validating proposal.");
        let handle = tokio::spawn(
            async move {
                validate_proposal(ProposalValidateArguments {
                    block_info_validation,
                    proposal_id,
                    batcher,
                    timeout,
                    batcher_timeout_margin,
                    valid_proposals,
                    content_receiver,
                    fin_sender,
                    cancel_token: cancel_token_clone,
                    transaction_converter,
                })
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
async fn build_proposal(mut args: ProposalBuildArguments) {
    let block_info = initiate_build(&args).await;
    let block_info = match block_info {
        Ok(info) => info,
        Err(e) => {
            error!("Failed to initiate proposal build. {e:?}");
            return;
        }
    };
    args.proposal_sender
        .send(ProposalPart::Init(args.proposal_init))
        .await
        .expect("Failed to send proposal init");
    args.proposal_sender
        .send(ProposalPart::BlockInfo(block_info.clone()))
        .await
        .expect("Failed to send block info");

    let Some((proposal_commitment, content)) = get_proposal_content(
        args.proposal_id,
        args.batcher.as_ref(),
        args.proposal_sender,
        args.cende_write_success,
        &args.transaction_converter,
        args.cancel_token,
    )
    .await
    else {
        return;
    };

    // Update valid_proposals before sending fin to avoid a race condition
    // with `repropose` being called before `valid_proposals` is updated.
    let mut valid_proposals = args.valid_proposals.lock().expect("Lock was poisoned");
    valid_proposals
        .entry(args.proposal_init.height)
        .or_default()
        .insert(proposal_commitment, (block_info, content, args.proposal_id));
    if args.fin_sender.send(proposal_commitment).is_err() {
        // Consensus may exit early (e.g. sync).
        warn!("Failed to send proposal content id");
    }
}

async fn initiate_build(args: &ProposalBuildArguments) -> BuildProposalResult<ConsensusBlockInfo> {
    let batcher_timeout = chrono::Duration::from_std(args.batcher_timeout)
        .expect("Can't convert timeout to chrono::Duration");
    // TODO(guy.f): Replace this with a mockable call to be able to test the correct time is set.
    let now = chrono::Utc::now();
    let timestamp = now.timestamp().try_into().expect("Failed to convert timestamp");
    let eth_to_fri_rate = match &args.eth_to_strk_oracle_client {
        Some(eth_to_strk_oracle_client) => {
            eth_to_strk_oracle_client.eth_to_fri_rate(timestamp).await?
        }
        None => 1,
    };
    // TODO(Asmaa): change this to the real values.
    let block_info = ConsensusBlockInfo {
        height: args.proposal_init.height,
        timestamp,
        builder: args.builder_address,
        l1_da_mode: args.l1_da_mode,
        l2_gas_price_fri: args.gas_prices.strk_gas_prices.l2_gas_price.get().0,
        l1_gas_price_wei: args.gas_prices.eth_gas_prices.l1_gas_price.get().0,
        l1_data_gas_price_wei: args.gas_prices.eth_gas_prices.l1_data_gas_price.get().0,
        eth_to_fri_rate,
    };
    let build_proposal_input = ProposeBlockInput {
        proposal_id: args.proposal_id,
        deadline: now + batcher_timeout,
        // TODO(Matan): This is not part of Milestone 1.
        retrospective_block_hash: Some(BlockHashAndNumber {
            number: BlockNumber::default(),
            hash: BlockHash::default(),
        }),
        block_info: convert_to_sn_api_block_info(&block_info),
    };
    // TODO(Matan): Should we be returning an error?
    // I think this implies defining an error type in this crate and moving the trait definition
    // here also.
    debug!("Initiating build proposal: {build_proposal_input:?}");
    args.batcher.propose_block(build_proposal_input).await?;
    Ok(block_info)
}

// 1. Receive chunks of content from the batcher.
// 2. Forward these to the stream handler to be streamed out to the network.
// 3. Once finished, receive the commitment from the batcher.
async fn get_proposal_content(
    proposal_id: ProposalId,
    batcher: &dyn BatcherClient,
    mut proposal_sender: mpsc::Sender<ProposalPart>,
    cende_write_success: AbortOnDropHandle<bool>,
    transaction_converter: &TransactionConverter,
    cancel_token: CancellationToken,
) -> Option<(ProposalCommitment, Vec<Vec<InternalConsensusTransaction>>)> {
    let mut content = Vec::new();
    loop {
        if cancel_token.is_cancelled() {
            warn!("Proposal interrupted during building.");
            return None;
        }
        // We currently want one part of the node failing to cause all components to fail. If this
        // changes, we can simply return None and consider this as a failed proposal which consensus
        // should support.
        let response = batcher.get_proposal_content(GetProposalContentInput { proposal_id }).await;
        let response = match response {
            Ok(resp) => resp,
            Err(e) => {
                error!("Failed to get proposal content. {e:?}");
                return None;
            }
        };

        match response.content {
            GetProposalContent::Txs(txs) => {
                content.push(txs.clone());
                // TODO(matan): Make sure this isn't too large for a single proto message.
                debug!(
                    hashes = ?txs.iter().map(|tx| tx.tx_hash()).collect::<Vec<TransactionHash>>(),
                    "Sending transaction batch with {} txs.",
                    txs.len()
                );
                let transactions = futures::future::join_all(txs.into_iter().map(|tx| {
                    transaction_converter.convert_internal_consensus_tx_to_consensus_tx(tx)
                }))
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>();
                let transactions = match transactions {
                    Ok(txs) => txs,
                    Err(e) => {
                        error!("Failed to convert transactions. {e:?}");
                        return None;
                    }
                };

                trace!(?transactions, "Sending transaction batch with {} txs.", transactions.len());
                proposal_sender
                    .send(ProposalPart::Transactions(TransactionBatch { transactions }))
                    .await
                    .expect("Failed to broadcast proposal content");
            }
            GetProposalContent::Finished(id) => {
                let proposal_commitment = BlockHash(id.state_diff_commitment.0.0);
                let num_txs: usize = content.iter().map(|batch| batch.len()).sum();
                info!(?proposal_commitment, num_txs = num_txs, "Finished building proposal",);

                // If the blob writing operation to Aerospike doesn't return a success status, we
                // can't finish the proposal.
                match cende_write_success.now_or_never() {
                    Some(Ok(true)) => {
                        info!("Writing blob to Aerospike completed successfully.");
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
                    .send(ProposalPart::Fin(ProposalFin { proposal_commitment }))
                    .await
                    .expect("Failed to broadcast proposal fin");
                return Some((proposal_commitment, content));
            }
        }
    }
}

async fn validate_proposal(mut args: ProposalValidateArguments) {
    let mut content = Vec::new();
    let deadline = tokio::time::Instant::now() + args.timeout;
    let Some((block_info, fin_sender)) = await_second_proposal_part(
        &args.cancel_token,
        deadline,
        &mut args.content_receiver,
        args.fin_sender,
    )
    .await
    else {
        return;
    };
    if !is_block_info_valid(args.block_info_validation.clone(), block_info.clone()).await {
        warn!(
            "Invalid BlockInfo. block_info_validation={:?}, block_info={:?}",
            args.block_info_validation, block_info
        );
        return;
    }
    if let Err(e) = initiate_validation(
        args.batcher.as_ref(),
        block_info.clone(),
        args.proposal_id,
        args.timeout + args.batcher_timeout_margin,
    )
    .await
    {
        error!("Failed to initiate proposal validation. {e:?}");
        return;
    }
    // Validating the rest of the proposal parts.
    let (built_block, received_fin) = loop {
        tokio::select! {
            _ = args.cancel_token.cancelled() => {
                warn!("Proposal interrupted during validation.");
                batcher_abort_proposal(args.batcher.as_ref(), args.proposal_id).await;
                return;
            }
            _ = tokio::time::sleep_until(deadline) => {
                warn!("Validation timed out.");
                batcher_abort_proposal(args.batcher.as_ref(), args.proposal_id).await;
                return;
            }
            proposal_part = args.content_receiver.next() => {
                match handle_proposal_part(
                    args.proposal_id,
                    args.batcher.as_ref(),
                    proposal_part,
                    &mut content,
                    &args.transaction_converter,
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
                        batcher_abort_proposal(args.batcher.as_ref(), args.proposal_id).await;
                        return;
                    }
                }
            }
        }
    };

    let num_txs: usize = content.iter().map(|batch| batch.len()).sum();
    CONSENSUS_NUM_BATCHES_IN_PROPOSAL.set_lossy(content.len());
    CONSENSUS_NUM_TXS_IN_PROPOSAL.set_lossy(num_txs);

    // Update valid_proposals before sending fin to avoid a race condition
    // with `get_proposal` being called before `valid_proposals` is updated.
    // TODO(Matan): Consider validating the ProposalFin signature here.
    let mut valid_proposals = args.valid_proposals.lock().unwrap();
    valid_proposals
        .entry(args.block_info_validation.height)
        .or_default()
        .insert(built_block, (block_info, content, args.proposal_id));
    if fin_sender.send((built_block, received_fin)).is_err() {
        // Consensus may exit early (e.g. sync).
        warn!("Failed to send proposal content ids");
    }
}

async fn is_block_info_valid(
    block_info_validation: BlockInfoValidation,
    block_info: ConsensusBlockInfo,
) -> bool {
    let now: u64 =
        chrono::Utc::now().timestamp().try_into().expect("Failed to convert timestamp to u64");
    // TODO(Asmaa): Validate the rest of the block info.
    block_info.height == block_info_validation.height
        && block_info.timestamp >= block_info_validation.last_block_timestamp.unwrap_or(0)
        && block_info.timestamp <= now + block_info_validation.block_timestamp_window
        && block_info.l1_da_mode == block_info_validation.l1_da_mode
}

// The second proposal part when validating a proposal must be:
// 1. Fin - empty proposal.
// 2. BlockInfo - required to begin executing TX batches.
async fn await_second_proposal_part(
    cancel_token: &CancellationToken,
    deadline: Instant,
    content_receiver: &mut mpsc::Receiver<ProposalPart>,
    fin_sender: oneshot::Sender<(ProposalCommitment, ProposalFin)>,
) -> Option<(ConsensusBlockInfo, oneshot::Sender<(ProposalCommitment, ProposalFin)>)> {
    tokio::select! {
        _ = cancel_token.cancelled() => {
            warn!("Proposal interrupted");
            None
        }
        _ = tokio::time::sleep_until(deadline) => {
            warn!("Validation timed out.");
            None
        }
        proposal_part = content_receiver.next() => {
            match proposal_part {
                Some(ProposalPart::BlockInfo(block_info)) => {
                    Some((block_info, fin_sender))
                }
                Some(ProposalPart::Fin(ProposalFin { proposal_commitment })) => {
                    warn!("Received an empty proposal.");
                    if fin_sender
                        .send((EMPTY_BLOCK_COMMITMENT, ProposalFin { proposal_commitment }))
                        .is_err()
                    {
                        // Consensus may exit early (e.g. sync).
                        warn!("Failed to send proposal content ids");
                    }
                    None
                }
                x => {
                    warn!("Invalid second proposal part: {x:?}");
                    None
                }
            }
        }
    }
}

async fn initiate_validation(
    batcher: &dyn BatcherClient,
    block_info: ConsensusBlockInfo,
    proposal_id: ProposalId,
    timeout_plus_margin: Duration,
) -> BatcherClientResult<()> {
    let chrono_timeout = chrono::Duration::from_std(timeout_plus_margin)
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
        block_info: convert_to_sn_api_block_info(&block_info),
    };
    debug!("Initiating validate proposal: input={input:?}");
    batcher.validate_block(input).await
}

// Handles receiving a proposal from another node without blocking consensus:
// 1. Receives the proposal part from the network.
// 2. Pass this to the batcher.
// 3. Once finished, receive the commitment from the batcher.
async fn handle_proposal_part(
    proposal_id: ProposalId,
    batcher: &dyn BatcherClient,
    proposal_part: Option<ProposalPart>,
    content: &mut Vec<Vec<InternalConsensusTransaction>>,
    transaction_converter: &TransactionConverter,
) -> HandledProposalPart {
    match proposal_part {
        None => HandledProposalPart::Failed("Failed to receive proposal content".to_string()),
        Some(ProposalPart::Fin(ProposalFin { proposal_commitment: id })) => {
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
            let num_txs: usize = content.iter().map(|batch| batch.len()).sum();
            info!(
                network_block_id = ?id,
                ?batcher_block_id,
                num_txs,
                "Finished validating proposal."
            );
            HandledProposalPart::Finished(batcher_block_id, ProposalFin { proposal_commitment: id })
        }
        Some(ProposalPart::Transactions(TransactionBatch { transactions: txs })) => {
            debug!("Received transaction batch with {} txs", txs.len());
            let txs =
                futures::future::join_all(txs.into_iter().map(|tx| {
                    transaction_converter.convert_consensus_tx_to_internal_consensus_tx(tx)
                }))
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>();
            let txs = match txs {
                Ok(txs) => txs,
                Err(e) => {
                    return HandledProposalPart::Failed(format!(
                        "Failed to convert transactions. Stopping the build of the current \
                         proposal. {e:?}"
                    ));
                }
            };
            debug!("Converted transactions to internal representation.");

            content.push(txs.clone());
            let input =
                SendProposalContentInput { proposal_id, content: SendProposalContent::Txs(txs) };
            let response = batcher.send_proposal_content(input).await.unwrap_or_else(|e| {
                panic!("Failed to send proposal content to batcher: {proposal_id:?}. {e:?}")
            });
            match response.response {
                ProposalStatus::Processing => HandledProposalPart::Continue,
                ProposalStatus::InvalidProposal => HandledProposalPart::Invalid,
                status => panic!("Unexpected status: for {proposal_id:?}, {status:?}"),
            }
        }
        _ => HandledProposalPart::Failed("Invalid proposal part".to_string()),
    }
}

async fn batcher_abort_proposal(batcher: &dyn BatcherClient, proposal_id: ProposalId) {
    let input = SendProposalContentInput { proposal_id, content: SendProposalContent::Abort };
    batcher
        .send_proposal_content(input)
        .await
        .unwrap_or_else(|e| panic!("Failed to send Abort to batcher: {proposal_id:?}. {e:?}"));
}

fn convert_to_sn_api_block_info(block_info: &ConsensusBlockInfo) -> starknet_api::block::BlockInfo {
    let l1_gas_price_fri =
        NonzeroGasPrice::new(GasPrice(block_info.l1_gas_price_wei * block_info.eth_to_fri_rate))
            .unwrap();
    let l1_data_gas_price_fri = NonzeroGasPrice::new(GasPrice(
        block_info.l1_data_gas_price_wei * block_info.eth_to_fri_rate,
    ))
    .unwrap();
    let l2_gas_price_fri = NonzeroGasPrice::new(GasPrice(block_info.l2_gas_price_fri)).unwrap();

    let l1_gas_price_wei = NonzeroGasPrice::new(GasPrice(block_info.l1_gas_price_wei)).unwrap();
    let l1_data_gas_price_wei =
        NonzeroGasPrice::new(GasPrice(block_info.l1_data_gas_price_wei)).unwrap();

    starknet_api::block::BlockInfo {
        block_number: block_info.height,
        block_timestamp: BlockTimestamp(block_info.timestamp),
        sequencer_address: block_info.builder,
        gas_prices: GasPrices {
            strk_gas_prices: GasPriceVector {
                l1_gas_price: l1_gas_price_fri,
                l1_data_gas_price: l1_data_gas_price_fri,
                l2_gas_price: l2_gas_price_fri,
            },
            eth_gas_prices: GasPriceVector {
                l1_gas_price: l1_gas_price_wei,
                l1_data_gas_price: l1_data_gas_price_wei,
                ..TEMPORARY_GAS_PRICES.eth_gas_prices
            },
        },
        use_kzg_da: block_info.l1_da_mode == L1DataAvailabilityMode::Blob,
    }
}
