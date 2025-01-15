use std::collections::VecDeque;
use std::iter;

use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Bootstrapper {
    pub catch_up_height: BlockNumber,
    pub current_provider_height: BlockNumber,
    pub commit_block_backlog: VecDeque<CommitBlockBacklog>,
}

impl Bootstrapper {
    /// Returns an iterator over consecutive backlogged commit block entries matching sequential
    /// heights.
    pub fn drain_applicable_commit_block_backlog(
        &mut self,
        _start_height: BlockNumber,
    ) -> impl Iterator<Item = Vec<TransactionHash>> + '_ {
        // TODO
        iter::empty()
    }

    pub fn is_complete(&self) -> bool {
        self.catch_up_height <= self.current_provider_height
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommitBlockBacklog {
    pub height: BlockNumber,
    pub committed_txs: Vec<TransactionHash>,
}
