use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use apollo_batcher_config::config::{BatcherConfig, FirstBlockWithPartialBlockHash};
use apollo_batcher_types::batcher_types::{ProposalId, ProposeBlockInput};
use apollo_class_manager_types::{EmptyClassManagerClient, SharedClassManagerClient};
use apollo_committer_types::communication::{MockCommitterClient, SharedCommitterClient};
use apollo_l1_provider_types::MockL1ProviderClient;
use apollo_mempool_types::communication::MockMempoolClient;
use apollo_mempool_types::mempool_types::CommitBlockArgs;
use async_trait::async_trait;
use blockifier::blockifier::transaction_executor::BlockExecutionSummary;
use blockifier::bouncer::{BouncerWeights, CasmHashComputationData};
use blockifier::fee::receipt::TransactionReceipt;
use blockifier::state::cached_state::CommitmentStateDiff;
use blockifier::transaction::objects::TransactionExecutionInfo;
use indexmap::{indexmap, IndexMap};
use mockall::predicate::eq;
use starknet_api::block::{BlockHash, BlockInfo, BlockNumber};
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::state::ThinStateDiff;
use starknet_api::test_utils::invoke::{internal_invoke_tx, InvokeTxArgs};
use starknet_api::test_utils::l1_handler::{executable_l1_handler_tx, L1HandlerTxArgs};
use starknet_api::transaction::fields::{Fee, TransactionSignature};
use starknet_api::transaction::TransactionHash;
use starknet_api::{class_hash, contract_address, nonce, tx_hash};
use starknet_types_core::felt::Felt;
use tokio::sync::mpsc::{channel, Receiver, Sender, UnboundedSender};
use tokio::task::JoinHandle;

use crate::batcher::{MockBatcherStorageReader, MockBatcherStorageWriter};
use crate::block_builder::{
    BlockBuilderError,
    BlockBuilderResult,
    BlockBuilderTrait,
    BlockExecutionArtifacts,
    BlockTransactionExecutionData,
    FailOnErrorCause,
    MockBlockBuilderFactoryTrait,
};
use crate::commitment_manager::state_committer::StateCommitterTrait;
use crate::commitment_manager::types::{CommitmentTaskInput, CommitmentTaskOutput};
use crate::pre_confirmed_block_writer::{
    MockPreconfirmedBlockWriterFactoryTrait,
    MockPreconfirmedBlockWriterTrait,
};
use crate::transaction_provider::{TransactionProvider, TxProviderPhase};

pub const EXECUTION_INFO_LEN: usize = 10;
pub const DUMMY_FINAL_N_EXECUTED_TXS: usize = 12;

pub(crate) const INITIAL_HEIGHT: BlockNumber = BlockNumber(3);
pub(crate) const FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH: BlockNumber =
    INITIAL_HEIGHT.prev().unwrap();
pub(crate) const DUMMY_BLOCK_HASH: BlockHash = BlockHash(Felt::from_hex_unchecked("0xdeadbeef"));
pub(crate) const LATEST_BLOCK_IN_STORAGE: BlockNumber = BlockNumber(INITIAL_HEIGHT.0 - 1);
pub(crate) const STREAMING_CHUNK_SIZE: usize = 3;
pub(crate) const BLOCK_GENERATION_TIMEOUT: tokio::time::Duration =
    tokio::time::Duration::from_secs(1);
pub(crate) const PROPOSAL_ID: ProposalId = ProposalId(0);
pub(crate) const BUILD_BLOCK_FAIL_ON_ERROR: BlockBuilderError =
    BlockBuilderError::FailOnError(FailOnErrorCause::BlockFull);

// A fake block builder for validate flow, that fetches transactions from the transaction provider
// until it is exhausted.
// This ensures the block builder (and specifically the tx_provider) is not dropped before all
// transactions are processed. Otherwise, the batcher would fail during tests when attempting to
// send transactions to it.
pub(crate) struct FakeValidateBlockBuilder {
    pub tx_provider: Box<dyn TransactionProvider>,
    pub build_block_result: Option<BlockBuilderResult<BlockExecutionArtifacts>>,
}

#[async_trait]
impl BlockBuilderTrait for FakeValidateBlockBuilder {
    async fn build_block(&mut self) -> BlockBuilderResult<BlockExecutionArtifacts> {
        // build_block should be called only once, so we can safely take the result.
        let build_block_result = self.build_block_result.take().unwrap();

        if build_block_result.is_ok() {
            while self.tx_provider.get_final_n_executed_txs().await.is_none() {
                self.tx_provider.get_txs(1).await.unwrap();
                tokio::task::yield_now().await;
            }
        }
        build_block_result
    }
}

// A fake block builder for propose flow, that sends the given transactions to the output content
// sender.
pub(crate) struct FakeProposeBlockBuilder {
    pub output_content_sender: UnboundedSender<InternalConsensusTransaction>,
    pub output_txs: Vec<InternalConsensusTransaction>,
    pub build_block_result: Option<BlockBuilderResult<BlockExecutionArtifacts>>,
    pub tx_provider: Box<dyn TransactionProvider>,
}

#[async_trait]
impl BlockBuilderTrait for FakeProposeBlockBuilder {
    async fn build_block(&mut self) -> BlockBuilderResult<BlockExecutionArtifacts> {
        for tx in &self.output_txs {
            // Skip L1 txs if the tx_provider was set to mempool phase.
            if matches!(tx, InternalConsensusTransaction::L1Handler(_))
                && self.tx_provider.phase() == TxProviderPhase::Mempool
            {
                continue;
            }
            self.output_content_sender.send(tx.clone()).unwrap();
        }

        // build_block should be called only once, so we can safely take the result.
        self.build_block_result.take().unwrap()
    }
}

pub fn test_txs(tx_hash_range: Range<usize>) -> Vec<InternalConsensusTransaction> {
    tx_hash_range
        .map(|i| {
            InternalConsensusTransaction::RpcTransaction(internal_invoke_tx(InvokeTxArgs {
                tx_hash: tx_hash!(i),
                ..Default::default()
            }))
        })
        .collect()
}

pub fn test_l1_handler_txs(tx_hash_range: Range<usize>) -> Vec<InternalConsensusTransaction> {
    tx_hash_range
        .map(|i| {
            InternalConsensusTransaction::L1Handler(executable_l1_handler_tx(L1HandlerTxArgs {
                tx_hash: tx_hash!(i),
                ..Default::default()
            }))
        })
        .collect()
}

// Create `execution_infos` with an indexed field to enable verification of the order.
fn indexed_execution_infos_and_signatures()
-> IndexMap<TransactionHash, (TransactionExecutionInfo, Option<TransactionSignature>)> {
    test_txs(0..EXECUTION_INFO_LEN)
        .iter()
        .enumerate()
        .map(|(i, tx)| {
            (
                tx.tx_hash(),
                (
                    TransactionExecutionInfo {
                        receipt: TransactionReceipt {
                            fee: Fee(i.try_into().unwrap()),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    None,
                ),
            )
        })
        .collect()
}

// Verify that `execution_infos` was initiated with an indexed fields.
pub fn verify_indexed_execution_infos(
    execution_infos: &IndexMap<TransactionHash, TransactionExecutionInfo>,
) {
    for (i, execution_info) in execution_infos.iter().enumerate() {
        assert_eq!(execution_info.1.receipt.fee, Fee(i.try_into().unwrap()));
    }
}

impl BlockExecutionArtifacts {
    pub async fn create_for_testing() -> Self {
        // Use a non-empty commitment_state_diff to get a valuable test verification of the result.
        let execution_data = BlockTransactionExecutionData {
            execution_infos_and_signatures: indexed_execution_infos_and_signatures(),
            rejected_tx_hashes: test_txs(10..15).iter().map(|tx| tx.tx_hash()).collect(),
            consumed_l1_handler_tx_hashes: Default::default(),
        };
        let block_execution_summary = BlockExecutionSummary {
            state_diff: CommitmentStateDiff {
                address_to_class_hash: IndexMap::from_iter([(
                    contract_address!("0x7"),
                    class_hash!("0x11111111"),
                )]),
                storage_updates: IndexMap::new(),
                class_hash_to_compiled_class_hash: IndexMap::new(),
                address_to_nonce: IndexMap::from_iter([(contract_address!("0x7"), nonce!(1_u64))]),
            },
            compressed_state_diff: Default::default(),
            bouncer_weights: BouncerWeights::empty(),
            casm_hash_computation_data_sierra_gas: CasmHashComputationData::empty(),
            casm_hash_computation_data_proving_gas: CasmHashComputationData::empty(),
            compiled_class_hashes_for_migration: vec![],
            block_info: BlockInfo::create_for_testing(),
        };
        Self::new(block_execution_summary, execution_data, DUMMY_FINAL_N_EXECUTED_TXS).await
    }
}

pub(crate) fn propose_block_input(proposal_id: ProposalId) -> ProposeBlockInput {
    ProposeBlockInput {
        proposal_id,
        proposal_round: 0,
        retrospective_block_hash: None,
        deadline: chrono::Utc::now() + BLOCK_GENERATION_TIMEOUT,
        block_info: BlockInfo { block_number: INITIAL_HEIGHT, ..BlockInfo::create_for_testing() },
    }
}

pub(crate) fn test_contract_nonces() -> HashMap<ContractAddress, Nonce> {
    HashMap::from_iter((0..3u8).map(|i| (contract_address!(i + 33), nonce!(i + 9))))
}

pub(crate) fn test_state_diff() -> ThinStateDiff {
    ThinStateDiff {
        storage_diffs: indexmap! {
            4u64.into() => indexmap! {
                5u64.into() => 6u64.into(),
                7u64.into() => 8u64.into(),
            },
            9u64.into() => indexmap! {
                10u64.into() => 11u64.into(),
            },
        },
        nonces: test_contract_nonces().into_iter().collect(),
        ..Default::default()
    }
}

pub(crate) struct MockDependencies {
    pub(crate) storage_reader: MockBatcherStorageReader,
    pub(crate) storage_writer: MockBatcherStorageWriter,
    pub(crate) batcher_config: BatcherConfig,
    pub(crate) clients: MockClients,
}

pub(crate) struct MockClients {
    pub(crate) committer_client: MockCommitterClient,
    pub(crate) mempool_client: MockMempoolClient,
    pub(crate) l1_provider_client: MockL1ProviderClient,
    pub(crate) block_builder_factory: MockBlockBuilderFactoryTrait,
    pub(crate) pre_confirmed_block_writer_factory: MockPreconfirmedBlockWriterFactoryTrait,
    pub(crate) class_manager_client: SharedClassManagerClient,
}

impl Default for MockClients {
    fn default() -> Self {
        let mut mempool_client = MockMempoolClient::new();
        let expected_gas_price = propose_block_input(PROPOSAL_ID)
            .block_info
            .gas_prices
            .strk_gas_prices
            .l2_gas_price
            .get();
        mempool_client.expect_update_gas_price().with(eq(expected_gas_price)).returning(|_| Ok(()));
        mempool_client
            .expect_commit_block()
            .with(eq(CommitBlockArgs::default()))
            .returning(|_| Ok(()));
        let block_builder_factory = MockBlockBuilderFactoryTrait::new();
        let mut pre_confirmed_block_writer_factory = MockPreconfirmedBlockWriterFactoryTrait::new();
        pre_confirmed_block_writer_factory.expect_create().returning(|_, _, _| {
            let (non_working_candidate_tx_sender, _) = tokio::sync::mpsc::channel(1);
            let (non_working_pre_confirmed_tx_sender, _) = tokio::sync::mpsc::channel(1);
            let mut mock_writer = Box::new(MockPreconfirmedBlockWriterTrait::new());
            mock_writer.expect_run().return_once(|| Ok(()));
            (mock_writer, non_working_candidate_tx_sender, non_working_pre_confirmed_tx_sender)
        });

        Self {
            committer_client: MockCommitterClient::new(),
            l1_provider_client: MockL1ProviderClient::new(),
            mempool_client,
            block_builder_factory,
            pre_confirmed_block_writer_factory,
            // TODO(noamsp): use MockClassManagerClient
            class_manager_client: Arc::new(EmptyClassManagerClient),
        }
    }
}

impl Default for MockDependencies {
    fn default() -> Self {
        let mut storage_reader = MockBatcherStorageReader::new();
        storage_reader.expect_height().returning(|| Ok(INITIAL_HEIGHT));
        storage_reader.expect_global_root_height().returning(|| Ok(INITIAL_HEIGHT));
        storage_reader.expect_get_state_diff().returning(|_| Ok(Some(test_state_diff())));

        let batcher_config = BatcherConfig {
            outstream_content_buffer_size: STREAMING_CHUNK_SIZE,
            first_block_with_partial_block_hash: Some(FirstBlockWithPartialBlockHash {
                block_number: FIRST_BLOCK_NUMBER_WITH_PARTIAL_BLOCK_HASH,
                parent_block_hash: DUMMY_BLOCK_HASH,
                ..Default::default()
            }),
            ..Default::default()
        };
        Self {
            storage_reader,
            storage_writer: MockBatcherStorageWriter::new(),
            clients: MockClients::default(),
            batcher_config,
        }
    }
}

pub(crate) struct MockStateCommitter {
    _task_performer_handle: JoinHandle<()>,
    mock_task_sender: Sender<()>,
}

impl StateCommitterTrait for MockStateCommitter {
    fn create(
        tasks_receiver: Receiver<CommitmentTaskInput>,
        results_sender: Sender<CommitmentTaskOutput>,
        _committer_client: SharedCommitterClient,
    ) -> Self {
        let (mock_task_sender, mock_task_receiver) = channel(10);
        let handle = tokio::spawn(async move {
            Self::do_nothing(tasks_receiver, results_sender, mock_task_receiver).await;
        });
        Self { _task_performer_handle: handle, mock_task_sender }
    }
    fn get_handle(&self) -> &JoinHandle<()> {
        &self._task_performer_handle
    }
}

impl MockStateCommitter {
    /// Run forever so channels aren't closed, and do nothing.
    pub(crate) async fn do_nothing(
        mut tasks_receiver: Receiver<CommitmentTaskInput>,
        mut _results_sender: Sender<CommitmentTaskOutput>,
        mut mock_task_receiver: Receiver<()>,
    ) {
        while let Some(_) = mock_task_receiver.recv().await {
            tasks_receiver.try_recv().unwrap();
            _results_sender.try_send(CommitmentTaskOutput::default()).unwrap();
        }
    }

    pub(crate) async fn pop_task_and_insert_result(&self) {
        self.mock_task_sender.send(()).await.unwrap();
    }
}
