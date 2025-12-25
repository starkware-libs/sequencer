use std::collections::HashMap;

use apollo_committer_config::config::CommitterConfig;
use apollo_committer_types::committer_types::CommitBlockRequest;
use apollo_committer_types::errors::CommitterError;
use assert_matches::assert_matches;
use async_trait::async_trait;
use indexmap::indexmap;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, CompiledClassHash, StateDiffCommitment};
use starknet_api::hash::{HashOutput, PoseidonHash};
use starknet_api::state::ThinStateDiff;
use starknet_committer::block_committer::commit::{BlockCommitmentResult, CommitBlockTrait};
use starknet_committer::block_committer::input::{Input, InputContext};
use starknet_committer::block_committer::timing_util::TimeMeasurement;
use starknet_committer::db::forest_trait::ForestReader;
use starknet_committer::forest::filled_forest::FilledForest;
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::FilledTreeImpl;
use starknet_patricia_storage::map_storage::MapStorage;

use super::Committer;

pub struct CommitBlockMock;

pub type ApolloTestStorage = MapStorage;
pub type ApolloTestCommitter = Committer<ApolloTestStorage, CommitBlockMock>;

#[async_trait]
impl CommitBlockTrait for CommitBlockMock {
    /// Sets the class trie root hash to the first class hash in the state diff.
    async fn commit_block<I: InputContext + Send, Reader: ForestReader<I> + Send>(
        input: Input<I>,
        _trie_reader: &mut Reader,
        _time_measurement: Option<&mut TimeMeasurement>,
    ) -> BlockCommitmentResult<FilledForest> {
        let class_hash =
            input.state_diff.class_hash_to_compiled_class_hash.iter().next().unwrap().0.0;
        Ok(FilledForest {
            storage_tries: HashMap::new(),
            contracts_trie: FilledTreeImpl {
                tree_map: HashMap::new(),
                root_hash: HashOutput::ROOT_OF_EMPTY_TREE,
            },
            classes_trie: FilledTreeImpl {
                tree_map: HashMap::new(),
                root_hash: HashOutput(class_hash),
            },
        })
    }
}

async fn new_test_committer() -> ApolloTestCommitter {
    Committer::new(CommitterConfig::default()).await
}

fn commit_block_request(
    state_diff: u64,
    state_diff_commitment: Option<u64>,
    height: u64,
) -> CommitBlockRequest {
    CommitBlockRequest {
        state_diff: ThinStateDiff {
            class_hash_to_compiled_class_hash: indexmap! {
                ClassHash(state_diff.into()) => CompiledClassHash(state_diff.into()),
            },
            ..Default::default()
        },
        state_diff_commitment: state_diff_commitment
            .map(|commitment| StateDiffCommitment(PoseidonHash(commitment.into()))),
        height: BlockNumber(height),
    }
}

#[tokio::test]
/// The committer returns an error when the input height is greater than the committer's offset.
async fn commit_height_hole() {
    let mut committer = new_test_committer().await;
    let response = committer.commit_block(commit_block_request(0, Some(1), 1)).await;
    assert_eq!(
        response,
        Err(CommitterError::HeightHole {
            input_height: BlockNumber(1),
            committer_offset: BlockNumber(0),
        })
    );
}

#[tokio::test]
/// The committer returns an error when the input state diff commitment does not match the stored
/// one.
async fn commit_invalid_state_diff_commitment() {
    let mut committer = new_test_committer().await;
    // Commit with a different state diff commitment.
    committer.commit_block(commit_block_request(1, Some(1), 0)).await.unwrap();
    let response = committer.commit_block(commit_block_request(1, Some(2), 0)).await;
    assert_eq!(
        response,
        Err(CommitterError::InvalidStateDiffCommitment {
            input_commitment: StateDiffCommitment(PoseidonHash(2.into())),
            stored_commitment: StateDiffCommitment(PoseidonHash(1.into())),
            height: BlockNumber(0),
        })
    );

    // Commit with a different state diff:
    // a. state diff commitment is given - ignore the state diff.
    committer.commit_block(commit_block_request(2, Some(1), 0)).await.unwrap();
    // b. state diff commitment is not given - compare by the commitment of the state diff.
    committer.commit_block(commit_block_request(3, None, 1)).await.unwrap();
    let response = committer.commit_block(commit_block_request(4, None, 1)).await;
    assert_matches!(
        response,
        Err(CommitterError::InvalidStateDiffCommitment { height: BlockNumber(1), .. })
    );
}

#[tokio::test]
/// The committer's offset starts at 0 and is incremented.
async fn committer_offset() {
    // TODO(Yoav): Test offset at initialization of the committer.
    let mut committer = new_test_committer().await;
    assert_eq!(committer.offset, BlockNumber(0));

    committer.commit_block(commit_block_request(1, Some(1), 0)).await.unwrap();
    assert_eq!(committer.offset, BlockNumber(1));

    committer.commit_block(commit_block_request(1, Some(1), 0)).await.unwrap();
    assert_eq!(committer.offset, BlockNumber(1));

    committer.commit_block(commit_block_request(2, Some(2), 1)).await.unwrap();
    assert_eq!(committer.offset, BlockNumber(2));

    // The offset is not incremented in case of error.
    committer.commit_block(commit_block_request(3, Some(3), 3)).await.unwrap_err();
    assert_eq!(committer.offset, BlockNumber(2));
}

#[tokio::test]
/// Committing the same block twice returns the same state root.
async fn commit_idempotent_test() {
    let mut committer = new_test_committer().await;
    let state_root_1 = committer.commit_block(commit_block_request(1, Some(1), 0)).await.unwrap();
    let state_root_2 = committer.commit_block(commit_block_request(1, Some(1), 0)).await.unwrap();
    assert_eq!(state_root_1, state_root_2);

    let state_root_3 = committer.commit_block(commit_block_request(2, None, 1)).await.unwrap();
    let state_root_4 = committer.commit_block(commit_block_request(2, None, 1)).await.unwrap();
    assert_eq!(state_root_3, state_root_4);

    assert_ne!(state_root_1, state_root_3);
}
