use std::collections::VecDeque;

use indexmap::IndexMap;
use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::transaction::TransactionHash;
use starknet_l1_provider_types::ValidationStatus;

use crate::staged_removal_index_map::StagedRemovalIndexMap;

#[derive(Debug, Default)]
pub struct TransactionManager {
    pub txs: StagedRemovalIndexMap,
    pub committed: IndexMap<TransactionHash, Option<L1HandlerTransaction>>,
    pub commit_block_backlog: VecDeque<CommitBlockBacklog>,
}

impl TransactionManager {
    pub fn start_block(&mut self) {
        self.txs.rollback();
    }

    pub fn get_txs(&mut self, n_txs: usize) -> Vec<L1HandlerTransaction> {
        let mut txs = Vec::with_capacity(n_txs);
        for _ in 0..n_txs {
            match self.txs.stage_pop_back() {
                Some(tx) => txs.push(tx),
                None => break,
            }
        }
        txs
    }

    pub fn validate_tx(&mut self, tx_hash: TransactionHash) -> ValidationStatus {
        if self.committed.contains_key(&tx_hash) {
            return ValidationStatus::AlreadyIncludedOnL2;
        }

        let tx = self.txs.stage_remove(tx_hash);
        match tx {
            Some(true) => ValidationStatus::Validated,
            Some(false) => ValidationStatus::AlreadyIncludedInPropsedBlock,
            None => ValidationStatus::ConsumedOnL1OrUnknown,
        }
    }

    pub fn commit_block(&mut self, committed_txs: &[TransactionHash]) {
        let committed = self.txs.commit(committed_txs).into_iter().map(|tx| (tx.tx_hash, Some(tx)));
        self.committed.extend(committed);
    }

    pub fn apply_commit_block_txs(&mut self, committed_txs: &[TransactionHash]) {
        self.commit_block(committed_txs);
    }

    pub fn add_commit_block_to_backlog(
        &mut self,
        committed_txs: &[TransactionHash],
        height: BlockNumber,
    ) {
        self.commit_block_backlog
            .push_back(CommitBlockBacklog { height, committed_txs: committed_txs.to_vec() });
    }

    pub fn apply_backlogged_commit_blocks(
        &mut self,
        mut current_height: BlockNumber,
    ) -> BlockNumber {
        while let Some(backlog) = self.commit_block_backlog.front() {
            if backlog.height == current_height {
                let txs = self.commit_block_backlog.pop_front().unwrap().committed_txs;
                self.apply_commit_block_txs(&txs);
                current_height = current_height.unchecked_next();
            } else {
                break;
            }
        }
        current_height
    }

    pub fn _add_unconsumed_l1_not_in_l2_block_tx(&mut self, _tx: L1HandlerTransaction) {
        todo!(
            "Check if tx is in L2, if it isn't on L2 add it to the txs buffer, otherwise print
             debug and do nothing."
        )
    }

    pub fn _mark_tx_included_on_l2(&mut self, _tx_hash: &TransactionHash) {
        todo!("Adds the tx hash to l2 buffer; remove tx from the txs storage if it's there.")
    }
}

#[derive(Debug, Default)]
pub struct CommitBlockBacklog {
    height: BlockNumber,
    committed_txs: Vec<TransactionHash>,
}
