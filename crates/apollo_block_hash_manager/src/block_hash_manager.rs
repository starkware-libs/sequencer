use std::sync::Arc;

use apollo_batcher_types::batcher_types::SetBlockCommitmentInput;
use apollo_batcher_types::communication::BatcherClient;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::hash::{BlockCommitment, CommitmentOutput as CommitterCommitmentOutput};
use starknet_api::state::ThinStateDiff;
use starknet_committer::block_committer::input::StateDiff;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tracing::warn;

use crate::errors::BlockHashManagerError;
const N_SET_RETRIES: usize = 10;
pub type BlockHashManagerResult<T> = Result<T, BlockHashManagerError>;

pub struct CommitmentInput {
    pub block_number: BlockNumber,
    pub state_diff: ThinStateDiff,
    pub commitment_output: CommitterCommitmentOutput,
}

struct CommitmentOutput {
    committer_output: CommitterCommitmentOutput,
    block_number: BlockNumber,
}

pub struct BlockHashManager {
    batcher_client: Arc<dyn BatcherClient>,
}

impl BlockHashManager {
    pub async fn new(batcher_client: Arc<dyn BatcherClient>) -> Self {
        Self { batcher_client }
    }

    // TODO(Nimrod): Consider taking `mut self` to make sure this function is called only once.
    pub async fn start(&mut self, mut commitment_input_receiver: Receiver<CommitmentInput>) {
        // Spawn a task to listen to the commitment input channel and send it to the committer.
        let (commitment_output_sender, mut commitment_output_receiver) = channel(10);
        let batcher_client = self.batcher_client.clone();
        tokio::spawn(async move {
            while let Some(commitment_input) = commitment_input_receiver.recv().await {
                // TODO(Nimrod): Handle errors properly.
                handle_commitment_input(commitment_input, &commitment_output_sender).await.unwrap();
            }
        });
        // Spawn a task to handle commitment outputs and send them to the batcher.
        tokio::spawn(async move {
            while let Some(commitment_output) = commitment_output_receiver.recv().await {
                // TODO(Nimrod): Handle errors properly.
                handle_commitment_output(commitment_output, batcher_client.clone()).await.unwrap();
            }
        });
    }
}

async fn handle_commitment_output(
    CommitmentOutput { block_number, committer_output }: CommitmentOutput,
    batcher_client: Arc<dyn BatcherClient>,
) -> BlockHashManagerResult<()> {
    // TODO(Nimrod): Get real values by adding such API to `BatcherClient`.
    let _partial_block_hash_components = 8;
    let _parent_hash = 9;

    let _global_root = committer_output.global_root();
    // Finalize block hash.
    let block_hash = BlockHash(80.into());
    let input = SetBlockCommitmentInput {
        block_number,
        commitment: BlockCommitment { block_hash, state_commitment: committer_output },
    };
    for _ in 0..N_SET_RETRIES {
        match batcher_client.set_block_commitments(input.clone()).await {
            Ok(_) => return Ok(()),
            Err(err) => {
                warn!(
                    "Failed to set block commitment for block {block_number}, retrying...\n \
                     error: {err}"
                );
                continue;
            }
        }
    }
    Err(BlockHashManagerError::SetBlockCommitment(block_number))
}

async fn handle_commitment_input(
    CommitmentInput { block_number, state_diff, commitment_output: _commitment_output }: CommitmentInput,
    commitment_output_sender: &Sender<CommitmentOutput>,
) -> BlockHashManagerResult<()> {
    let _committer_state_diff: StateDiff = state_diff.into();
    // TODO(Nimrod): Call the committer client here to get real result.
    let committer_output = CommitterCommitmentOutput::default();
    commitment_output_sender
        .send(CommitmentOutput { committer_output, block_number })
        .await
        .expect("Failed to send commitment output in the channel.");
    Ok(())
}
