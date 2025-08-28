use std::future::ready;
use std::sync::{Arc, LazyLock, OnceLock};
use std::time::Duration;

use apollo_batcher_types::batcher_types::{
    GetProposalContent,
    GetProposalContentResponse,
    ProposalCommitment,
    ProposalStatus,
    ProposeBlockInput,
    SendProposalContent,
    SendProposalContentInput,
    SendProposalContentResponse,
    ValidateBlockInput,
};
use apollo_batcher_types::communication::MockBatcherClient;
use apollo_class_manager_types::transaction_converter::{
    MockTransactionConverterTrait,
    TransactionConverter,
    TransactionConverterTrait,
};
use apollo_class_manager_types::EmptyClassManagerClient;
use apollo_l1_gas_price_types::{MockL1GasPriceProviderClient, PriceInfo};
use apollo_network::network_manager::test_utils::{
    mock_register_broadcast_topic,
    BroadcastNetworkMock,
    TestSubscriberChannels,
};
use apollo_network::network_manager::{BroadcastTopicChannels, BroadcastTopicClient};
use apollo_protobuf::consensus::{ConsensusBlockInfo, HeightAndRound, ProposalPart, Vote};
use apollo_state_sync_types::communication::MockStateSyncClient;
use apollo_time::time::{Clock, DefaultClock};
use futures::channel::mpsc;
use futures::executor::block_on;
use starknet_api::block::{
    BlockNumber,
    GasPrice,
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
use apollo_consensus_orchestrator_config::ContextConfig;
use crate::orchestrator_versioned_constants::VersionedConstants;
use crate::sequencer_consensus_context::{
    SequencerConsensusContext,
    SequencerConsensusContextDeps,
};

pub(crate) const TIMEOUT: Duration = Duration::from_millis(1200);
pub(crate) const CHANNEL_SIZE: usize = 5000;
pub(crate) const NUM_VALIDATORS: u64 = 4;
pub(crate) const STATE_DIFF_COMMITMENT: StateDiffCommitment =
    StateDiffCommitment(PoseidonHash(Felt::ZERO));
pub(crate) const CHAIN_ID: ChainId = ChainId::Mainnet;

// In order for gas price in ETH to be greather than 0 (required) we must have large enough
// values here.
pub(crate) const ETH_TO_FRI_RATE: u128 = u128::pow(10, 18);

pub(crate) static TX_BATCH: LazyLock<Vec<ConsensusTransaction>> =
    LazyLock::new(|| (0..3).map(generate_invoke_tx).collect());

pub(crate) static INTERNAL_TX_BATCH: LazyLock<Vec<InternalConsensusTransaction>> =
    LazyLock::new(|| {
        // TODO(shahak): Use MockTransactionConverter instead.
        static TRANSACTION_CONVERTER: LazyLock<TransactionConverter> = LazyLock::new(|| {
            TransactionConverter::new(Arc::new(EmptyClassManagerClient), CHAIN_ID)
        });
        TX_BATCH
            .iter()
            .cloned()
            .map(|tx| {
                block_on(TRANSACTION_CONVERTER.convert_consensus_tx_to_internal_consensus_tx(tx))
                    .unwrap()
            })
            .collect()
    });

pub(crate) struct TestDeps {
    pub transaction_converter: MockTransactionConverterTrait,
    pub state_sync_client: MockStateSyncClient,
    pub batcher: MockBatcherClient,
    pub cende_ambassador: MockCendeContext,
    pub l1_gas_price_provider: MockL1GasPriceProviderClient,
    pub clock: Arc<dyn Clock>,
    pub outbound_proposal_sender: mpsc::Sender<(HeightAndRound, mpsc::Receiver<ProposalPart>)>,
    pub vote_broadcast_client: BroadcastTopicClient<Vote>,
}

impl From<TestDeps> for SequencerConsensusContextDeps {
    fn from(deps: TestDeps) -> Self {
        SequencerConsensusContextDeps {
            transaction_converter: Arc::new(deps.transaction_converter),
            state_sync_client: Arc::new(deps.state_sync_client),
            batcher: Arc::new(deps.batcher),
            cende_ambassador: Arc::new(deps.cende_ambassador),
            l1_gas_price_provider: Arc::new(deps.l1_gas_price_provider),
            clock: deps.clock,
            outbound_proposal_sender: deps.outbound_proposal_sender,
            vote_broadcast_client: deps.vote_broadcast_client,
        }
    }
}

impl TestDeps {
    pub(crate) fn setup_default_expectations(&mut self) {
        self.setup_default_transaction_converter();
        self.setup_default_cende_ambassador();
        self.setup_default_gas_price_provider();
    }

    pub(crate) fn setup_deps_for_build(
        &mut self,
        block_number: BlockNumber,
        final_n_executed_txs: usize,
    ) {
        assert!(final_n_executed_txs <= INTERNAL_TX_BATCH.len());
        self.setup_default_expectations();
        let proposal_id = Arc::new(OnceLock::new());
        let proposal_id_clone = Arc::clone(&proposal_id);
        self.batcher.expect_propose_block().times(1).returning(move |input: ProposeBlockInput| {
            proposal_id_clone.set(input.proposal_id).unwrap();
            Ok(())
        });
        self.batcher
            .expect_start_height()
            .times(1)
            .withf(move |input| input.height == block_number)
            .return_const(Ok(()));
        let proposal_id_clone = Arc::clone(&proposal_id);
        self.batcher.expect_get_proposal_content().times(1).returning(move |input| {
            assert_eq!(input.proposal_id, *proposal_id_clone.get().unwrap());
            Ok(GetProposalContentResponse {
                content: GetProposalContent::Txs(INTERNAL_TX_BATCH.clone()),
            })
        });
        let proposal_id_clone = Arc::clone(&proposal_id);
        self.batcher.expect_get_proposal_content().times(1).returning(move |input| {
            assert_eq!(input.proposal_id, *proposal_id_clone.get().unwrap());
            Ok(GetProposalContentResponse {
                content: GetProposalContent::Finished {
                    id: ProposalCommitment { state_diff_commitment: STATE_DIFF_COMMITMENT },
                    final_n_executed_txs,
                },
            })
        });
    }

    pub(crate) fn setup_deps_for_validate(
        &mut self,
        block_number: BlockNumber,
        final_n_executed_txs: usize,
    ) {
        assert!(final_n_executed_txs <= INTERNAL_TX_BATCH.len());
        self.setup_default_expectations();
        let proposal_id = Arc::new(OnceLock::new());
        let proposal_id_clone = Arc::clone(&proposal_id);
        self.batcher.expect_validate_block().times(1).returning(
            move |input: ValidateBlockInput| {
                proposal_id_clone.set(input.proposal_id).unwrap();
                Ok(())
            },
        );
        self.batcher
            .expect_start_height()
            .withf(move |input| input.height == block_number)
            .return_const(Ok(()));
        let proposal_id_clone = Arc::clone(&proposal_id);
        self.batcher.expect_send_proposal_content().times(1).returning(
            move |input: SendProposalContentInput| {
                assert_eq!(input.proposal_id, *proposal_id_clone.get().unwrap());
                let SendProposalContent::Txs(txs) = input.content else {
                    panic!("Expected SendProposalContent::Txs, got {:?}", input.content);
                };
                assert_eq!(txs, *INTERNAL_TX_BATCH);
                Ok(SendProposalContentResponse { response: ProposalStatus::Processing })
            },
        );
        let proposal_id_clone = Arc::clone(&proposal_id);
        self.batcher.expect_send_proposal_content().times(1).returning(
            move |input: SendProposalContentInput| {
                assert_eq!(input.proposal_id, *proposal_id_clone.get().unwrap());
                assert_eq!(input.content, SendProposalContent::Finish(final_n_executed_txs));
                Ok(SendProposalContentResponse {
                    response: ProposalStatus::Finished(ProposalCommitment {
                        state_diff_commitment: STATE_DIFF_COMMITMENT,
                    }),
                })
            },
        );
    }

    pub(crate) fn setup_default_transaction_converter(&mut self) {
        for (tx, internal_tx) in TX_BATCH.iter().zip(INTERNAL_TX_BATCH.iter()) {
            self.transaction_converter
                .expect_convert_internal_consensus_tx_to_consensus_tx()
                .withf(move |tx| tx == internal_tx)
                .returning(|_| Ok(tx.clone()));
            self.transaction_converter
                .expect_convert_consensus_tx_to_internal_consensus_tx()
                .withf(move |internal_tx| internal_tx == tx)
                .returning(|_| Ok(internal_tx.clone()));
        }
    }

    pub(crate) fn setup_default_cende_ambassador(&mut self) {
        self.cende_ambassador
            .expect_write_prev_height_blob()
            .return_once(|_height| tokio::spawn(ready(true)));
    }

    pub(crate) fn setup_default_gas_price_provider(&mut self) {
        self.l1_gas_price_provider.expect_get_price_info().return_const(Ok(PriceInfo {
            base_fee_per_gas: GasPrice(TEMP_ETH_GAS_FEE_IN_WEI),
            blob_fee: GasPrice(TEMP_ETH_BLOB_GAS_FEE_IN_WEI),
        }));
        self.l1_gas_price_provider.expect_get_eth_to_fri_rate().return_const(Ok(ETH_TO_FRI_RATE));
    }

    pub(crate) fn build_context(self) -> SequencerConsensusContext {
        SequencerConsensusContext::new(
            ContextConfig {
                proposal_buffer_size: CHANNEL_SIZE,
                num_validators: NUM_VALIDATORS,
                chain_id: CHAIN_ID,
                ..Default::default()
            },
            self.into(),
        )
    }
}

pub(crate) fn create_test_and_network_deps() -> (TestDeps, NetworkDependencies) {
    let (outbound_proposal_sender, outbound_proposal_receiver) =
        mpsc::channel::<(HeightAndRound, mpsc::Receiver<ProposalPart>)>(CHANNEL_SIZE);

    let TestSubscriberChannels { mock_network: mock_vote_network, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels { broadcast_topic_client: votes_topic_client, .. } =
        subscriber_channels;

    let transaction_converter = MockTransactionConverterTrait::new();
    let state_sync_client = MockStateSyncClient::new();
    let batcher = MockBatcherClient::new();
    let cende_ambassador = MockCendeContext::new();
    let l1_gas_price_provider = MockL1GasPriceProviderClient::new();
    let clock = Arc::new(DefaultClock);

    let test_deps = TestDeps {
        transaction_converter,
        state_sync_client,
        batcher,
        cende_ambassador,
        l1_gas_price_provider,
        clock,
        outbound_proposal_sender,
        vote_broadcast_client: votes_topic_client,
    };

    let network_deps =
        NetworkDependencies { _vote_network: mock_vote_network, outbound_proposal_receiver };

    (test_deps, network_deps)
}

pub(crate) fn generate_invoke_tx(nonce: u8) -> ConsensusTransaction {
    ConsensusTransaction::RpcTransaction(rpc_invoke_tx(InvokeTxArgs {
        nonce: Nonce(felt!(nonce)),
        ..Default::default()
    }))
}

pub(crate) fn block_info(height: BlockNumber) -> ConsensusBlockInfo {
    let context_config = ContextConfig::default();
    ConsensusBlockInfo {
        height,
        timestamp: chrono::Utc::now().timestamp().try_into().expect("Timestamp conversion failed"),
        builder: Default::default(),
        l1_da_mode: L1DataAvailabilityMode::Blob,
        l2_gas_price_fri: VersionedConstants::latest_constants().min_gas_price,
        l1_gas_price_wei: GasPrice(TEMP_ETH_GAS_FEE_IN_WEI + context_config.l1_gas_tip_wei),
        l1_data_gas_price_wei: GasPrice(
            TEMP_ETH_BLOB_GAS_FEE_IN_WEI * context_config.l1_data_gas_price_multiplier_ppt / 1000,
        ),
        eth_to_fri_rate: ETH_TO_FRI_RATE,
    }
}
// Structs which aren't utilized but should not be dropped.
pub(crate) struct NetworkDependencies {
    _vote_network: BroadcastNetworkMock<Vote>,
    pub outbound_proposal_receiver: mpsc::Receiver<(HeightAndRound, mpsc::Receiver<ProposalPart>)>,
}
