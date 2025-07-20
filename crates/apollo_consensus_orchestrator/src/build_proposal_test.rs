use std::sync::{Arc, Mutex};
use std::time::Duration;

use apollo_batcher_types::batcher_types::{
    GetProposalContent,
    GetProposalContentResponse,
    ProposalCommitment,
    ProposalId,
};
use apollo_batcher_types::communication::BatcherClientError;
use apollo_class_manager_types::transaction_converter::{
    MockTransactionConverterTrait,
    TransactionConverterError,
};
use apollo_consensus::types::Round;
use apollo_infra::component_client::ClientError;
use apollo_protobuf::consensus::{ConsensusBlockInfo, ProposalInit, ProposalPart};
use apollo_state_sync_types::communication::StateSyncClientError;
use apollo_state_sync_types::errors::StateSyncError;
use assert_matches::assert_matches;
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use futures::channel::mpsc;
use num_rational::Ratio;
use starknet_api::block::{BlockHash, BlockNumber, GasPrice};
use starknet_api::core::{ClassHash, ContractAddress};
use starknet_api::data_availability::L1DataAvailabilityMode;
use tokio_util::sync::CancellationToken;
use tokio_util::task::AbortOnDropHandle;

use crate::build_proposal::{build_proposal, BuildProposalError, ProposalBuildArguments};
use crate::config::ContextConfig;
use crate::orchestrator_versioned_constants::VersionedConstants;
use crate::sequencer_consensus_context::BuiltProposals;
use crate::test_utils::{
    create_test_and_network_deps,
    TestDeps,
    CHANNEL_SIZE,
    INTERNAL_TX_BATCH,
    STATE_DIFF_COMMITMENT,
    TIMEOUT,
};
use crate::utils::{GasPriceParams, StreamSender};

struct TestProposalBuildArguments {
    pub deps: TestDeps,
    pub batcher_timeout: Duration,
    pub proposal_init: ProposalInit,
    pub l1_da_mode: L1DataAvailabilityMode,
    pub stream_sender: StreamSender,
    pub gas_price_params: GasPriceParams,
    pub valid_proposals: Arc<Mutex<BuiltProposals>>,
    pub proposal_id: ProposalId,
    pub cende_write_success: AbortOnDropHandle<bool>,
    pub l2_gas_price: GasPrice,
    pub builder_address: ContractAddress,
    pub cancel_token: CancellationToken,
    pub previous_block_info: Option<ConsensusBlockInfo>,
    pub proposal_round: Round,
}

impl From<TestProposalBuildArguments> for ProposalBuildArguments {
    fn from(args: TestProposalBuildArguments) -> Self {
        ProposalBuildArguments {
            deps: args.deps.into(),
            batcher_timeout: args.batcher_timeout,
            proposal_init: args.proposal_init,
            l1_da_mode: args.l1_da_mode,
            stream_sender: args.stream_sender,
            gas_price_params: args.gas_price_params,
            valid_proposals: args.valid_proposals,
            proposal_id: args.proposal_id,
            cende_write_success: args.cende_write_success,
            l2_gas_price: args.l2_gas_price,
            builder_address: args.builder_address,
            cancel_token: args.cancel_token,
            previous_block_info: args.previous_block_info,
            proposal_round: args.proposal_round,
        }
    }
}

fn create_proposal_build_arguments() -> (TestProposalBuildArguments, mpsc::Receiver<ProposalPart>) {
    let (mut deps, _) = create_test_and_network_deps();
    deps.setup_default_expectations();
    let batcher_timeout = TIMEOUT;
    let proposal_init = ProposalInit::default();
    let l1_da_mode = L1DataAvailabilityMode::Calldata;
    let (proposal_sender, proposal_receiver) = mpsc::channel::<ProposalPart>(CHANNEL_SIZE);
    let stream_sender = StreamSender { proposal_sender };
    let context_config = ContextConfig::default();

    let gas_price_params = GasPriceParams {
        min_l1_gas_price_wei: GasPrice(context_config.min_l1_gas_price_wei),
        max_l1_gas_price_wei: GasPrice(context_config.max_l1_gas_price_wei),
        min_l1_data_gas_price_wei: GasPrice(context_config.min_l1_data_gas_price_wei),
        max_l1_data_gas_price_wei: GasPrice(context_config.max_l1_data_gas_price_wei),
        l1_data_gas_price_multiplier: Ratio::new(
            context_config.l1_data_gas_price_multiplier_ppt,
            1000,
        ),
        l1_gas_tip_wei: GasPrice(context_config.l1_gas_tip_wei),
    };
    let valid_proposals = Arc::new(Mutex::new(BuiltProposals::new()));
    let proposal_id = ProposalId(1);
    let cende_write_success = AbortOnDropHandle::new(tokio::spawn(async { true }));
    let l2_gas_price = VersionedConstants::latest_constants().min_gas_price;
    let builder_address = ContractAddress::default();
    let cancel_token = CancellationToken::new();
    let previous_block_info = None;
    let proposal_round = 0;

    (
        TestProposalBuildArguments {
            deps,
            batcher_timeout,
            proposal_init,
            l1_da_mode,
            stream_sender,
            gas_price_params,
            valid_proposals,
            proposal_id,
            cende_write_success,
            l2_gas_price,
            builder_address,
            cancel_token,
            previous_block_info,
            proposal_round,
        },
        proposal_receiver,
    )
}

#[tokio::test]
async fn build_proposal_succeed() {
    let (mut proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    // Setup batcher.
    proposal_args.deps.batcher.expect_propose_block().returning(|_| Ok(()));
    proposal_args.deps.batcher.expect_get_proposal_content().returning(|_| {
        Ok(GetProposalContentResponse {
            content: GetProposalContent::Finished {
                id: ProposalCommitment { state_diff_commitment: STATE_DIFF_COMMITMENT },
                final_n_executed_txs: 0,
            },
        })
    });
    // Make sure cende returns on time.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let res = build_proposal(proposal_args.into()).await.unwrap();
    assert_eq!(res, BlockHash::default());
}

#[tokio::test]
async fn state_sync_client_error() {
    let (mut proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    // Make sure state_sync_client being called, by setting height to >= STORED_BLOCK_HASH_BUFFER.
    proposal_args.proposal_init.height = BlockNumber(STORED_BLOCK_HASH_BUFFER);
    // Setup state sync client to return an error.
    proposal_args.deps.state_sync_client.expect_get_block_hash().returning(|_| {
        Err(StateSyncClientError::ClientError(ClientError::CommunicationFailure("".to_string())))
    });

    let res = build_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(BuildProposalError::StateSyncClientError(_))));
}

#[tokio::test]
async fn state_sync_not_ready_error() {
    let (mut proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    // Make sure state_sync_client being called, by setting height to >= STORED_BLOCK_HASH_BUFFER.
    proposal_args.proposal_init.height = BlockNumber(STORED_BLOCK_HASH_BUFFER);
    // Setup state sync client to return None, indicating that the state sync is not ready.
    proposal_args.deps.state_sync_client.expect_get_block_hash().returning(|block_number| {
        Err(StateSyncClientError::StateSyncError(StateSyncError::BlockNotFound(block_number)))
    });

    let res = build_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(BuildProposalError::StateSyncNotReady(_))));
}

#[tokio::test]
async fn propose_block_fail() {
    let (mut proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    // Setup batcher to return an error on propose_block.
    proposal_args.deps.batcher.expect_propose_block().returning(|_| {
        Err(BatcherClientError::ClientError(ClientError::CommunicationFailure("".to_string())))
    });

    let res = build_proposal(proposal_args.into()).await;
    assert_matches!(
        res,
        Err(BuildProposalError::Batcher(msg, _)) if msg.contains("Failed to initiate build proposal")
    );
}

#[tokio::test]
async fn get_proposal_content_fail() {
    let (mut proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    // Setup batcher to return an error on get_proposal_content.
    proposal_args.deps.batcher.expect_propose_block().returning(|_| Ok(()));
    proposal_args.deps.batcher.expect_get_proposal_content().returning(|_| {
        Err(BatcherClientError::ClientError(ClientError::CommunicationFailure("".to_string())))
    });

    let res = build_proposal(proposal_args.into()).await;
    assert_matches!(
        res,
        Err(BuildProposalError::Batcher(msg, _)) if msg.contains("Failed to get proposal content")
    );
}

#[tokio::test]
async fn interrupt_proposal() {
    let (mut proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    // Setup batcher to return Ok on propose_block.
    proposal_args.deps.batcher.expect_propose_block().returning(|_| Ok(()));
    // Interrupt the proposal.
    proposal_args.cancel_token.cancel();

    let res = build_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(BuildProposalError::Interrupted)));
}

#[tokio::test]
async fn convert_internal_consensus_tx_to_consensus_tx_fail() {
    let (mut proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    // Setup batcher to return Ok on propose_block and TX from get_proposal_content.
    proposal_args.deps.batcher.expect_propose_block().returning(|_| Ok(()));
    proposal_args.deps.batcher.expect_get_proposal_content().times(1).returning(|_| {
        Ok(GetProposalContentResponse {
            content: GetProposalContent::Txs(INTERNAL_TX_BATCH.clone()),
        })
    });
    // Overwrite the transaction converter to return an error, since by default it returns Ok.
    let mut transaction_converter = MockTransactionConverterTrait::new();
    transaction_converter.expect_convert_internal_consensus_tx_to_consensus_tx().returning(|_| {
        Err(TransactionConverterError::ClassNotFound { class_hash: ClassHash::default() })
    });
    proposal_args.deps.transaction_converter = transaction_converter;

    let res = build_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(BuildProposalError::TransactionConverterError(_))));
}

#[tokio::test]
async fn cende_fail() {
    let (mut proposal_args, _proposal_receiver) = create_proposal_build_arguments();
    // Setup batcher to return Ok on propose_block and Finished from get_proposal_content.
    proposal_args.deps.batcher.expect_propose_block().returning(|_| Ok(()));
    proposal_args.deps.batcher.expect_get_proposal_content().times(1).returning(|_| {
        Ok(GetProposalContentResponse {
            content: GetProposalContent::Finished {
                id: ProposalCommitment { state_diff_commitment: STATE_DIFF_COMMITMENT },
                final_n_executed_txs: 0,
            },
        })
    });
    // Setup cende to return false, indicating a failure.
    proposal_args.cende_write_success = AbortOnDropHandle::new(tokio::spawn(async { false }));

    let res = build_proposal(proposal_args.into()).await;
    assert!(matches!(res, Err(BuildProposalError::CendeWriteError(_))));
}
