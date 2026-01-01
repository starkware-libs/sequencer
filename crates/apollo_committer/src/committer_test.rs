use std::collections::HashMap;

use apollo_committer_config::config::CommitterConfig;
use apollo_committer_types::committer_types::CommitBlockRequest;
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
    state_diff_info: u64,
    state_diff_commitment: Option<u64>,
    height: u64,
) -> CommitBlockRequest {
    CommitBlockRequest {
        state_diff: ThinStateDiff {
            class_hash_to_compiled_class_hash: indexmap! {
                ClassHash(state_diff_info.into()) => CompiledClassHash(state_diff_info.into()),
            },
            ..Default::default()
        },
        state_diff_commitment: state_diff_commitment
            .map(|commitment| StateDiffCommitment(PoseidonHash(commitment.into()))),
        height: BlockNumber(height),
    }
}
