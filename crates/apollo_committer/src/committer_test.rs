use std::collections::HashMap;
use std::path::PathBuf;

use apollo_committer_config::config::CommitterConfig;
use apollo_committer_types::committer_types::{
    CommitBlockRequest,
    RevertBlockRequest,
    RevertBlockResponse,
};
use apollo_committer_types::errors::CommitterError;
use assert_matches::assert_matches;
use async_trait::async_trait;
use indexmap::indexmap;
use starknet_api::block::BlockNumber;
use starknet_api::block_hash::state_diff_hash::calculate_state_diff_hash;
use starknet_api::core::{ClassHash, CompiledClassHash, StateDiffCommitment};
use starknet_api::hash::{HashOutput, PoseidonHash};
use starknet_api::state::ThinStateDiff;
use starknet_committer::block_committer::commit::{BlockCommitmentResult, CommitBlockTrait};
use starknet_committer::block_committer::input::Input;
use starknet_committer::block_committer::measurements_util::MeasurementsTrait;
use starknet_committer::db::forest_trait::ForestReader;
use starknet_committer::db::mock_forest_storage::MockForestStorage;
use starknet_committer::forest::filled_forest::FilledForest;
use starknet_patricia::patricia_merkle_tree::filled_tree::tree::FilledTreeImpl;
use starknet_patricia_storage::map_storage::MapStorage;

use super::Committer;
use crate::committer::StorageConstructor;

pub struct CommitBlockMock;

#[async_trait]
impl CommitBlockTrait for CommitBlockMock {
    /// Sets the class trie root hash to the first class hash in the state diff (sorted
    /// deterministically).
    async fn commit_block<Reader: ForestReader + Send, M: MeasurementsTrait + Send>(
        input: Input<Reader::InitialReadContext>,
        _trie_reader: &mut Reader,
        _measurements: &mut M,
    ) -> BlockCommitmentResult<FilledForest> {
        // Sort class hashes deterministically to ensure all nodes get the same "first" class hash
        let mut sorted_class_hashes: Vec<_> =
            input.state_diff.class_hash_to_compiled_class_hash.keys().collect();
        sorted_class_hashes.sort();

        let root_class_hash = match sorted_class_hashes.first() {
            Some(class_hash) => HashOutput(class_hash.0),
            None => HashOutput::ROOT_OF_EMPTY_TREE,
        };
        Ok(FilledForest {
            storage_tries: HashMap::new(),
            contracts_trie: FilledTreeImpl {
                tree_map: HashMap::new(),
                root_hash: HashOutput::ROOT_OF_EMPTY_TREE,
            },
            classes_trie: FilledTreeImpl { tree_map: HashMap::new(), root_hash: root_class_hash },
        })
    }
}

pub type ApolloTestStorage = MapStorage;
pub type ApolloTestCommitter =
    Committer<ApolloTestStorage, MockForestStorage<ApolloTestStorage>, CommitBlockMock>;

impl StorageConstructor for ApolloTestStorage {
    fn create_storage(_db_path: PathBuf, _storage_config: Self::Config) -> Self {
        MapStorage::default()
    }
}

async fn new_test_committer() -> ApolloTestCommitter {
    Committer::new(CommitterConfig { verify_state_diff_hash: false, ..Default::default() }).await
}

fn get_state_diff(state_diff_info: u64) -> ThinStateDiff {
    ThinStateDiff {
        class_hash_to_compiled_class_hash: indexmap! {
            ClassHash(state_diff_info.into()) => CompiledClassHash(state_diff_info.into()),
        },
        ..Default::default()
    }
}

fn commit_block_request(
    state_diff_info: u64,
    state_diff_commitment: Option<u64>,
    height: u64,
) -> CommitBlockRequest {
    CommitBlockRequest {
        state_diff: get_state_diff(state_diff_info),
        state_diff_commitment: state_diff_commitment
            .map(|commitment| StateDiffCommitment(PoseidonHash(commitment.into()))),
        height: BlockNumber(height),
    }
}

fn revert_block_request(reversed_state_diff_info: u64, height: u64) -> RevertBlockRequest {
    // Note: for CommitBlockMock, the reversed state diff of "height" is the state diff of
    // "height-1".
    RevertBlockRequest {
        reversed_state_diff: get_state_diff(reversed_state_diff_info),
        height: BlockNumber(height),
    }
}

#[tokio::test]
async fn commit_height_hole() {
    let mut committer = new_test_committer().await;
    let response = committer.commit_block(commit_block_request(0, Some(1), 1)).await;
    // The input height is greater than the committer's offset.
    assert_matches!(
        response,
        Err(CommitterError::CommitHeightHole {
            input_height: BlockNumber(input_height),
            committer_offset: BlockNumber(committer_offset),
        })
        if input_height == 1 && committer_offset == 0
    );
}

#[tokio::test]
async fn commit_different_state_diff_commitment() {
    let mut committer = new_test_committer().await;
    let block_number = 0;
    let state_diff = 1;
    let state_diff_commitment = 1;
    let another_state_diff_commitment = 2;

    committer
        .commit_block(commit_block_request(state_diff, Some(state_diff_commitment), block_number))
        .await
        .unwrap();

    // Commit with a different state diff commitment.
    let response = committer
        .commit_block(commit_block_request(
            state_diff,
            Some(another_state_diff_commitment),
            block_number,
        ))
        .await;
    // The input state diff commitment does not match the stored one.
    assert_matches!(
        response,
        Err(CommitterError::InvalidStateDiffCommitment {
            input_commitment: StateDiffCommitment(PoseidonHash(
                input_commitment
            )),
            stored_commitment: StateDiffCommitment(PoseidonHash(stored_commitment)),
            height: BlockNumber(error_block_number),
        })
        if (
            input_commitment == another_state_diff_commitment.into()
            && stored_commitment == state_diff_commitment.into()
            && error_block_number == block_number
        )
    );
}

#[tokio::test]
async fn commit_different_state_diff() {
    // Compare by the commitment of the state diff, where the state diff commitment is not given.
    let mut committer = new_test_committer().await;
    let block_number = 0;
    let state_diff = 1;
    let another_state_diff = 2;

    committer.commit_block(commit_block_request(state_diff, None, block_number)).await.unwrap();
    let response =
        committer.commit_block(commit_block_request(another_state_diff, None, block_number)).await;
    assert_matches!(response, Err(CommitterError::InvalidStateDiffCommitment { .. }));
}

#[tokio::test]
/// The committer's offset starts at 0 and is incremented.
async fn committer_offset() {
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

    committer.commit_block(commit_block_request(2, Some(4), 1)).await.unwrap_err();
    assert_eq!(committer.offset, BlockNumber(2));

    let offset = ApolloTestCommitter::load_offset_or_panic(&mut committer.forest_storage).await;
    assert_eq!(offset, BlockNumber(2));
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

#[tokio::test]
/// Commits blocks 0, 1. Reverts block 1.
async fn revert_happy_flow() {
    let mut committer = new_test_committer().await;
    let mut height = 0;
    let state_diff_1 = 1;
    let state_diff_2 = 2;
    let state_root = committer
        .commit_block(commit_block_request(state_diff_1, Some(1), height))
        .await
        .unwrap()
        .global_root;

    height = 1;
    committer.commit_block(commit_block_request(state_diff_2, Some(2), height)).await.unwrap();
    assert_eq!(committer.offset, BlockNumber(height + 1));

    let response =
        committer.revert_block(revert_block_request(state_diff_1, height)).await.unwrap();

    assert_matches!(
        response,
        RevertBlockResponse::RevertedTo(reverted_state_root)
        if reverted_state_root == state_root
    );
    assert_eq!(committer.offset, BlockNumber(height));
}

#[tokio::test]
/// Commits blocks 0, 1. Reverts block 1 with a state diff that is not the reversed one.
async fn revert_to_invalid_global_root() {
    let mut committer = new_test_committer().await;
    let height_0 = 0;
    let state_root_0 = committer
        .commit_block(commit_block_request(1, Some(1), height_0))
        .await
        .unwrap()
        .global_root;

    let height_1 = 1;
    committer.commit_block(commit_block_request(2, Some(2), height_1)).await.unwrap();
    let response = committer.revert_block(revert_block_request(3, height_1)).await;

    assert_matches!(response, Err(CommitterError::InvalidRevertedGlobalRoot {
        stored_global_root: state_root,
        height: BlockNumber(height),
        ..
    })
    if height == height_0 && state_root == state_root_0
    );
    assert_eq!(committer.offset, BlockNumber(height_1 + 1));
}

#[tokio::test]
async fn revert_invalid_height() {
    let mut committer = new_test_committer().await;
    let mut height = 0;
    let state_diff_0 = 1;
    let state_diff_1 = 2;

    // Revert before any blocks are committed.
    let response = committer.revert_block(revert_block_request(5, height)).await.unwrap();
    assert_matches!(response, RevertBlockResponse::Uncommitted);

    committer.commit_block(commit_block_request(state_diff_0, Some(1), 0)).await.unwrap();
    height += 1;
    let state_root_1 = committer
        .commit_block(commit_block_request(state_diff_1, Some(2), 1))
        .await
        .unwrap()
        .global_root;
    let offset = height + 1;

    // Revert a future height.
    let response =
        committer.revert_block(revert_block_request(state_diff_0, offset + 1)).await.unwrap();
    assert_matches!(response, RevertBlockResponse::Uncommitted);

    // Revert the next height.
    let response =
        committer.revert_block(revert_block_request(state_diff_0, offset)).await.unwrap();
    assert_matches!(response, RevertBlockResponse::AlreadyReverted(reverted_state_root)
        if reverted_state_root == state_root_1
    );

    // Revert an old height.
    let response =
        committer.revert_block(revert_block_request(state_diff_0, offset - 2)).await.unwrap_err();
    assert_matches!(response, CommitterError::RevertHeightHole {
        input_height: BlockNumber(input_height),
        last_committed_block: BlockNumber(last_committed_block),
    }
    if input_height == offset - 2 && last_committed_block == offset - 1
    );

    assert_eq!(committer.offset, BlockNumber(offset));
}

#[tokio::test]
async fn verify_state_diff_hash_succeeds() {
    let mut committer = new_test_committer().await;
    committer.config.verify_state_diff_hash = true;
    let state_diff = get_state_diff(1);
    let state_diff_commitment = Some(calculate_state_diff_hash(&state_diff));
    let height = BlockNumber(0);
    committer
        .commit_block(CommitBlockRequest { state_diff, state_diff_commitment, height })
        .await
        .unwrap();
    assert_eq!(committer.offset, BlockNumber(height.0 + 1));
}

#[tokio::test]
async fn verify_state_diff_hash_fails() {
    let mut committer = new_test_committer().await;
    committer.config.verify_state_diff_hash = true;
    let state_diff = get_state_diff(1);
    let state_diff_commitment = Some(StateDiffCommitment(PoseidonHash(17.into())));
    let height = BlockNumber(0);
    let result = committer
        .commit_block(CommitBlockRequest { state_diff, state_diff_commitment, height })
        .await;
    assert_matches!(result, Err(CommitterError::StateDiffHashMismatch { .. }));
}
