use std::collections::VecDeque;

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
        start_height: BlockNumber,
    ) -> impl Iterator<Item = Vec<TransactionHash>> + '_ {
        let mut current_height = start_height;
        std::iter::from_fn(move || {
            let next = self.commit_block_backlog.front()?;
            if next.height != current_height.unchecked_next() {
                return None;
            }

            let item = self.commit_block_backlog.pop_front().unwrap();
            current_height = current_height.unchecked_next();
            self.current_provider_height = current_height;
            Some(item.committed_txs)
        })
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
