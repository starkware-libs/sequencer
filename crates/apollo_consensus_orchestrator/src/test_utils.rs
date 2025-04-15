use std::future::ready;
use std::sync::{Arc, LazyLock, OnceLock};
use std::time::Duration;

use apollo_batcher_types::batcher_types::{
    GetProposalContent,
    GetProposalContentResponse,
    ProposalCommitment,
    ProposeBlockInput,
};
use apollo_batcher_types::communication::MockBatcherClient;
use apollo_class_manager_types::transaction_converter::{
    TransactionConverter,
    TransactionConverterTrait,
};
use apollo_class_manager_types::EmptyClassManagerClient;
use apollo_consensus::types::ConsensusContext;
use apollo_l1_gas_price_types::{
    MockEthToStrkOracleClientTrait,
    MockL1GasPriceProviderClient,
    PriceInfo,
};
use apollo_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    BroadcastNetworkMock,
    TestSubscriberChannels,
};
use apollo_network::network_manager::BroadcastTopicChannels;
use apollo_protobuf::consensus::{
    ConsensusBlockInfo,
    HeightAndRound,
    ProposalInit,
    ProposalPart,
    Vote,
};
use apollo_state_sync_types::communication::MockStateSyncClient;
use futures::channel::{mpsc, oneshot};
use futures::executor::block_on;
use starknet_api::block::{
    BlockHash,
    BlockNumber,
    TEMP_ETH_BLOB_GAS_FEE_IN_WEI,
    TEMP_ETH_GAS_FEE_IN_WEI,
};
use starknet_api::consensus_transaction::{ConsensusTransaction, InternalConsensusTransaction};
use starknet_api::core::{ChainId, Nonce, StateDiffCommitment};
use starknet_api::data_availability::L1DataAvailabilityMode;
use starknet_api::felt;
use starknet_api::hash::PoseidonHash;
use starknet_api::test_utils::invoke::{rpc_invoke_tx, InvokeTxArgs};
use starknet_types_core::felt::Felt;

use crate::cende::MockCendeContext;
use crate::config::ContextConfig;
use crate::sequencer_consensus_context::{
    DefaultClock,
    SequencerConsensusContext,
    SequencerConsensusContextDeps,
};

pub const TIMEOUT: Duration = Duration::from_millis(1200);
const CHANNEL_SIZE: usize = 5000;
const NUM_VALIDATORS: u64 = 4;
pub const STATE_DIFF_COMMITMENT: StateDiffCommitment =
    StateDiffCommitment(PoseidonHash(Felt::ZERO));
const CHAIN_ID: ChainId = ChainId::Mainnet;

// In order for gas price in ETH to be greather than 0 (required) we must have large enough
// values here.
pub const ETH_TO_FRI_RATE: u128 = u128::pow(10, 18);

pub static TX_BATCH: LazyLock<Vec<ConsensusTransaction>> =
    LazyLock::new(|| (0..3).map(generate_invoke_tx).collect());

pub static INTERNAL_TX_BATCH: LazyLock<Vec<InternalConsensusTransaction>> = LazyLock::new(|| {
    // TODO(shahak): Use MockTransactionConverter instead.
    static TRANSACTION_CONVERTER: LazyLock<TransactionConverter> =
        LazyLock::new(|| TransactionConverter::new(Arc::new(EmptyClassManagerClient), CHAIN_ID));
    TX_BATCH
        .iter()
        .cloned()
        .map(|tx| {
            block_on(TRANSACTION_CONVERTER.convert_consensus_tx_to_internal_consensus_tx(tx))
                .unwrap()
        })
        .collect()
});

pub fn generate_invoke_tx(nonce: u8) -> ConsensusTransaction {
    ConsensusTransaction::RpcTransaction(rpc_invoke_tx(InvokeTxArgs {
        nonce: Nonce(felt!(nonce)),
        ..Default::default()
    }))
}

pub fn block_info(height: BlockNumber) -> ConsensusBlockInfo {
    ConsensusBlockInfo {
        height,
        timestamp: chrono::Utc::now().timestamp().try_into().expect("Timestamp conversion failed"),
        builder: Default::default(),
        l1_da_mode: L1DataAvailabilityMode::Blob,
        l2_gas_price_fri: 100000,
        l1_gas_price_wei: TEMP_ETH_GAS_FEE_IN_WEI,
        // TODO(guyn): I've put x10 on the data price, because currently
        // the minimal data gas price is 1 gwei, which is x10 this const.
        // Should adjust this when we have better min/max gas prices.
        l1_data_gas_price_wei: TEMP_ETH_BLOB_GAS_FEE_IN_WEI * 10,
        eth_to_fri_rate: ETH_TO_FRI_RATE,
    }
}
// Structs which aren't utilized but should not be dropped.
pub struct NetworkDependencies {
    _vote_network: BroadcastNetworkMock<Vote>,
    pub outbound_proposal_receiver: mpsc::Receiver<(HeightAndRound, mpsc::Receiver<ProposalPart>)>,
}

pub fn default_context_dependencies() -> (SequencerConsensusContextDeps, NetworkDependencies) {
    let (outbound_proposal_sender, outbound_proposal_receiver) =
        mpsc::channel::<(HeightAndRound, mpsc::Receiver<ProposalPart>)>(CHANNEL_SIZE);

    let TestSubscriberChannels { mock_network: mock_vote_network, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels { broadcast_topic_client: votes_topic_client, .. } =
        subscriber_channels;

    let mut eth_to_strk_oracle_client = MockEthToStrkOracleClientTrait::new();
    eth_to_strk_oracle_client.expect_eth_to_fri_rate().returning(|_| Ok(ETH_TO_FRI_RATE));
    let sequencer_deps = SequencerConsensusContextDeps {
        class_manager_client: Arc::new(EmptyClassManagerClient),
        state_sync_client: Arc::new(MockStateSyncClient::new()),
        batcher: Arc::new(MockBatcherClient::new()),
        outbound_proposal_sender,
        vote_broadcast_client: votes_topic_client,
        cende_ambassador: Arc::new(success_cende_ammbassador()),
        eth_to_strk_oracle_client: Arc::new(eth_to_strk_oracle_client),
        l1_gas_price_provider: Arc::new(MockL1GasPriceProviderClient::new()),
        clock: Arc::new(DefaultClock::default()),
    };

    let network_dependencies =
        NetworkDependencies { _vote_network: mock_vote_network, outbound_proposal_receiver };

    (sequencer_deps, network_dependencies)
}

pub fn setup_with_custom_mocks(
    context_deps: SequencerConsensusContextDeps,
) -> SequencerConsensusContext {
    SequencerConsensusContext::new(
        ContextConfig {
            proposal_buffer_size: CHANNEL_SIZE,
            num_validators: NUM_VALIDATORS,
            chain_id: CHAIN_ID,
            ..Default::default()
        },
        context_deps,
    )
}

// TODO(guy.f): Remove this method and rename `setup_with_custom_mocks` to `setup`, replace
// all calls to pass in a `SequencerConsensusContextDeps` object.
pub fn setup(
    batcher: MockBatcherClient,
    cende_ambassador: MockCendeContext,
) -> (SequencerConsensusContext, NetworkDependencies) {
    let (default_deps, network_dependencies) = default_context_dependencies();
    let context_deps = SequencerConsensusContextDeps {
        batcher: Arc::new(batcher),
        cende_ambassador: Arc::new(cende_ambassador),
        l1_gas_price_provider: Arc::new(dummy_gas_price_provider()),
        ..default_deps
    };

    (setup_with_custom_mocks(context_deps), network_dependencies)
}

// Setup for test of the `build_proposal` function.
pub async fn build_proposal_setup(
    mock_cende_context: MockCendeContext,
) -> (oneshot::Receiver<BlockHash>, SequencerConsensusContext, NetworkDependencies) {
    let mut batcher = MockBatcherClient::new();
    let proposal_id = Arc::new(OnceLock::new());
    let proposal_id_clone = Arc::clone(&proposal_id);
    batcher.expect_propose_block().times(1).returning(move |input: ProposeBlockInput| {
        proposal_id_clone.set(input.proposal_id).unwrap();
        Ok(())
    });
    batcher
        .expect_start_height()
        .times(1)
        .withf(|input| input.height == BlockNumber(0))
        .return_once(|_| Ok(()));
    let proposal_id_clone = Arc::clone(&proposal_id);
    batcher.expect_get_proposal_content().times(1).returning(move |input| {
        assert_eq!(input.proposal_id, *proposal_id_clone.get().unwrap());
        Ok(GetProposalContentResponse {
            content: GetProposalContent::Txs(INTERNAL_TX_BATCH.clone()),
        })
    });
    let proposal_id_clone = Arc::clone(&proposal_id);
    batcher.expect_get_proposal_content().times(1).returning(move |input| {
        assert_eq!(input.proposal_id, *proposal_id_clone.get().unwrap());
        Ok(GetProposalContentResponse {
            content: GetProposalContent::Finished(ProposalCommitment {
                state_diff_commitment: STATE_DIFF_COMMITMENT,
            }),
        })
    });

    let (mut context, _network) = setup(batcher, mock_cende_context);
    let init = ProposalInit::default();

    (context.build_proposal(init, TIMEOUT).await, context, _network)
}

// Returns a mock CendeContext that will return a successful write_prev_height_blob.
pub fn success_cende_ammbassador() -> MockCendeContext {
    let mut mock_cende = MockCendeContext::new();
    mock_cende.expect_write_prev_height_blob().return_once(|_height| tokio::spawn(ready(true)));
    mock_cende
}

pub fn dummy_gas_price_provider() -> MockL1GasPriceProviderClient {
    let mut l1_gas_price_provider = MockL1GasPriceProviderClient::new();
    l1_gas_price_provider.expect_get_price_info().returning(|_| {
        Ok(PriceInfo {
            base_fee_per_gas: TEMP_ETH_GAS_FEE_IN_WEI,
            blob_fee: TEMP_ETH_BLOB_GAS_FEE_IN_WEI,
        })
    });

    l1_gas_price_provider
}
