// TODO(Arni): Delete this file once the consensus manager is integrated into the end-to-end
// test.
use starknet_api::block::BlockNumber;
use starknet_batcher_types::batcher_types::{
    BuildProposalInput,
    DecisionReachedInput,
    GetProposalContentInput,
    GetProposalContentResponse,
    StartHeightInput,
};
use starknet_batcher_types::communication::SharedBatcherClient;

pub struct MockConsensusManager {
    pub batcher_client: SharedBatcherClient,
    current_height: BlockNumber,
}

impl MockConsensusManager {
    pub fn new(batcher_client: SharedBatcherClient) -> Self {
        Self { batcher_client, current_height: BlockNumber(1) }
    }

    async fn start_height(&self, input: StartHeightInput) {
        self.batcher_client.start_height(input).await.unwrap()
    }

    #[allow(dead_code)]
    async fn build_proposal(&self, input: BuildProposalInput) {
        self.batcher_client.build_proposal(input).await.unwrap()
    }

    #[allow(dead_code)]
    async fn get_proposal_content(
        &self,
        input: GetProposalContentInput,
    ) -> GetProposalContentResponse {
        self.batcher_client.get_proposal_content(input).await.unwrap()
    }

    #[allow(dead_code)]
    async fn decision_reached(&self, input: DecisionReachedInput) {
        self.batcher_client.decision_reached(input).await.unwrap()
    }

    #[allow(rustdoc::broken_intra_doc_links)]
    /// This function should mirror
    /// [`run_consensus`](papyrus_consensus::manager::run_consensus). It makes requests
    /// from the batcher client and asserts the expected responses were received.
    pub async fn run_consensus_for_end_to_end_test(&self) {
        self.start_height(StartHeightInput { height: self.current_height }).await;
    }
}
