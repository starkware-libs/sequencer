use async_trait::async_trait;
use futures::future::BoxFuture;
use mockall::automock;
use starknet_api::block::BlockNumber;
use starknet_api::executable_transaction::Transaction;
use starknet_batcher_types::batcher_types::ProposalId;

use crate::proposal_manager::{ProposalManagerResult, ProposalManagerTrait};

// A wrapper trait to allow mocking the ProposalManagerTrait in tests.
#[automock]
trait ProposalManagerTraitWrapper: Send + Sync {
    fn wrap_start_height(&mut self, height: BlockNumber) -> ProposalManagerResult<()>;

    fn wrap_build_block_proposal(
        &mut self,
        proposal_id: ProposalId,
        deadline: tokio::time::Instant,
        output_content_sender: tokio::sync::mpsc::Sender<Transaction>,
    ) -> BoxFuture<'_, ProposalManagerResult<()>>;
}

#[async_trait]
impl<T: ProposalManagerTraitWrapper> ProposalManagerTrait for T {
    fn start_height(&mut self, height: BlockNumber) -> ProposalManagerResult<()> {
        self.wrap_start_height(height)
    }

    async fn build_block_proposal(
        &mut self,
        proposal_id: ProposalId,
        deadline: tokio::time::Instant,
        output_content_sender: tokio::sync::mpsc::Sender<Transaction>,
    ) -> ProposalManagerResult<()> {
        self.wrap_build_block_proposal(proposal_id, deadline, output_content_sender).await
    }
}
