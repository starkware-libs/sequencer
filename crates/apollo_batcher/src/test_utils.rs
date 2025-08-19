use std::ops::Range;

use async_trait::async_trait;
use blockifier::bouncer::{BouncerWeights, CasmHashComputationData};
use blockifier::fee::receipt::TransactionReceipt;
use blockifier::state::cached_state::CommitmentStateDiff;
use blockifier::transaction::objects::TransactionExecutionInfo;
use indexmap::IndexMap;
use starknet_api::consensus_transaction::InternalConsensusTransaction;
use starknet_api::execution_resources::GasAmount;
use starknet_api::test_utils::invoke::{internal_invoke_tx, InvokeTxArgs};
use starknet_api::test_utils::l1_handler::{executable_l1_handler_tx, L1HandlerTxArgs};
use starknet_api::transaction::fields::Fee;
use starknet_api::transaction::TransactionHash;
use starknet_api::{class_hash, contract_address, nonce, tx_hash};
use tokio::sync::mpsc::UnboundedSender;

use crate::block_builder::{
    BlockBuilderResult,
    BlockBuilderTrait,
    BlockExecutionArtifacts,
    BlockTransactionExecutionData,
};
use crate::transaction_provider::TransactionProvider;

pub const EXECUTION_INFO_LEN: usize = 10;
pub const DUMMY_FINAL_N_EXECUTED_TXS: usize = 12;

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
}

#[async_trait]
impl BlockBuilderTrait for FakeProposeBlockBuilder {
    async fn build_block(&mut self) -> BlockBuilderResult<BlockExecutionArtifacts> {
        for tx in &self.output_txs {
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
fn indexed_execution_infos() -> IndexMap<TransactionHash, TransactionExecutionInfo> {
    test_txs(0..EXECUTION_INFO_LEN)
        .iter()
        .enumerate()
        .map(|(i, tx)| {
            (
                tx.tx_hash(),
                TransactionExecutionInfo {
                    receipt: TransactionReceipt {
                        fee: Fee(i.try_into().unwrap()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
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
    pub fn create_for_testing() -> Self {
        // Use a non-empty commitment_state_diff to get a valuable test verification of the result.
        Self {
            execution_data: BlockTransactionExecutionData {
                execution_infos: indexed_execution_infos(),
                rejected_tx_hashes: test_txs(10..15).iter().map(|tx| tx.tx_hash()).collect(),
                consumed_l1_handler_tx_hashes: Default::default(),
            },
            commitment_state_diff: CommitmentStateDiff {
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
            l2_gas_used: GasAmount::default(),
            casm_hash_computation_data_sierra_gas: CasmHashComputationData::empty(),
            casm_hash_computation_data_proving_gas: CasmHashComputationData::empty(),
            final_n_executed_txs: DUMMY_FINAL_N_EXECUTED_TXS,
        }
    }
}
