use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Bootstrapper {
    pub catch_up_height: BlockNumber,
    pub commit_block_backlog: Vec<CommitBlockBacklog>,
}

impl Bootstrapper {
    pub fn is_caught_up(&self, current_provider_height: BlockNumber) -> bool {
        self.catch_up_height == current_provider_height
    }

    pub fn add_commit_block_to_backlog(
        &mut self,
        committed_txs: &[TransactionHash],
        height: BlockNumber,
    ) {
        assert!(
            self.commit_block_backlog
                .last()
                .is_none_or(|commit_block| commit_block.height.unchecked_next() == height),
            "Heights should be sequential."
        );

        self.commit_block_backlog
            .push(CommitBlockBacklog { height, committed_txs: committed_txs.to_vec() });
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommitBlockBacklog {
    pub height: BlockNumber,
    pub committed_txs: Vec<TransactionHash>,
}
