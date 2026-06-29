use std::collections::{BTreeSet, HashMap};
use std::sync::LazyLock;

use apollo_committer_types::committer_types::{
    AccessedKeys,
    CommitBlockRequest,
    ReadPathsAndCommitBlockRequest,
    ReadPathsAndCommitBlockResponse,
    RevertBlockRequest,
};
use indexmap::indexmap;
use starknet_api::block::BlockNumber;
use starknet_api::block_hash::state_diff_hash::calculate_state_diff_hash;
use starknet_api::core::{
    ClassHash,
    CompiledClassHash,
    ContractAddress,
    Nonce,
    StateDiffCommitment,
    PATRICIA_KEY_UPPER_BOUND_FELT,
};
use starknet_api::hash::HashOutput;
use starknet_api::state::ThinStateDiff;
use starknet_committer::block_committer::input::{
    contract_address_into_node_index,
    StarknetStorageKey,
    StarknetStorageValue,
};
use starknet_committer::db::forest_trait::forest_trait_witnesses::ForestReaderWithWitnesses;
use starknet_committer::db::serde_db_utils::accessed_keys_digest;
use starknet_committer::hash_function::hash::TreeHashFunctionImpl;
use starknet_committer::patricia_merkle_tree::leaf::leaf_impl::ContractState;
use starknet_committer::patricia_merkle_tree::tree::LeavesRequest;
use starknet_committer::patricia_merkle_tree::types::{
    class_hash_into_node_index,
    CommitmentInfo,
    CompiledClassHash as CommitterCompiledClassHash,
    StateCommitmentInfos,
};
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::{
    BinaryData,
    NodeData,
    Preimage,
    PreimageMap,
};
use starknet_patricia::patricia_merkle_tree::node_data::leaf::Leaf;
use starknet_patricia::patricia_merkle_tree::storage_proof_verification::verify_patricia_proof;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunction;

use crate::committer::committer_test::{new_test_committer, ApolloTestCommitter};

const ACCESSED_STORAGE_VALUE_1: u128 = 100;
const ACCESSED_STORAGE_VALUE_2: u128 = 200;
const UNACCESSED_CLASS_HASH: u64 = 2;
const UNACCESSED_CONTRACT: u128 = 3;
const UNACCESSED_STORAGE_KEY: u128 = 11;
const UNACCESSED_STORAGE_VALUE: u128 = 300;

static ACCESSED_CLASS_HASH: LazyLock<ClassHash> = LazyLock::new(|| ClassHash(1u64.into()));
static ACCESSED_CONTRACT_1: LazyLock<ContractAddress> =
    LazyLock::new(|| ContractAddress::from(1u128));
static ACCESSED_CONTRACT_2: LazyLock<ContractAddress> =
    LazyLock::new(|| ContractAddress::from(2u128));
static ACCESSED_STORAGE_KEY_1: LazyLock<StarknetStorageKey> =
    LazyLock::new(|| StarknetStorageKey::from(10u128));
static ACCESSED_STORAGE_KEY_2: LazyLock<StarknetStorageKey> =
    LazyLock::new(|| StarknetStorageKey::from(20u128));

static ACCESSED_KEYS: LazyLock<AccessedKeys> = LazyLock::new(|| AccessedKeys {
    accessed_class_hashes: BTreeSet::from([*ACCESSED_CLASS_HASH]),
    accessed_contracts: BTreeSet::from([*ACCESSED_CONTRACT_1, *ACCESSED_CONTRACT_2]),
    storage_keys: BTreeSet::from([
        (*ACCESSED_CONTRACT_1, ACCESSED_STORAGE_KEY_1.0),
        (*ACCESSED_CONTRACT_2, ACCESSED_STORAGE_KEY_2.0),
    ]),
});

static EXPECTED_ACCESSED_KEYS_DIGEST: LazyLock<[u8; 32]> = LazyLock::new(|| {
    let mut leaves_request = LeavesRequest::from(&*ACCESSED_KEYS);
    let sorted_leaves = leaves_request.sorted();
    accessed_keys_digest(&sorted_leaves)
});

/// Leaf values for accessed class indices. Required to build facts-layout storage from the
/// returned [`PreimageMap`], which contains inner nodes only.
static ACCESSED_CLASS_LEAVES: LazyLock<HashMap<ClassHash, CommitterCompiledClassHash>> =
    LazyLock::new(|| {
        HashMap::from([(*ACCESSED_CLASS_HASH, CommitterCompiledClassHash(ACCESSED_CLASS_HASH.0))])
    });

/// Leaf values for accessed storage indices, per contract. Required to build facts-layout
/// storage from the returned [`PreimageMap`], which contains inner nodes only.
static ACCESSED_STORAGE_LEAVES: LazyLock<
    HashMap<ContractAddress, HashMap<StarknetStorageKey, StarknetStorageValue>>,
> = LazyLock::new(|| {
    HashMap::from([
        (
            *ACCESSED_CONTRACT_1,
            HashMap::from([(
                *ACCESSED_STORAGE_KEY_1,
                StarknetStorageValue(ACCESSED_STORAGE_VALUE_1.into()),
            )]),
        ),
        (
            *ACCESSED_CONTRACT_2,
            HashMap::from([(
                *ACCESSED_STORAGE_KEY_2,
                StarknetStorageValue(ACCESSED_STORAGE_VALUE_2.into()),
            )]),
        ),
    ])
});

static BLOCK_0_STATE_DIFF: LazyLock<ThinStateDiff> = LazyLock::new(|| {
    let class_hash = *ACCESSED_CLASS_HASH;
    let unaccessed_class_hash = ClassHash(UNACCESSED_CLASS_HASH.into());
    let contract_1 = *ACCESSED_CONTRACT_1;
    let contract_2 = *ACCESSED_CONTRACT_2;
    let unaccessed_contract = ContractAddress::from(UNACCESSED_CONTRACT);

    ThinStateDiff {
        deployed_contracts: indexmap! {
            contract_1 => class_hash,
            contract_2 => class_hash,
            unaccessed_contract => class_hash,
        },
        storage_diffs: indexmap! {
            contract_1 => indexmap! {
                ACCESSED_STORAGE_KEY_1.0 => ACCESSED_STORAGE_VALUE_1.into(),
                UNACCESSED_STORAGE_KEY.into() => UNACCESSED_STORAGE_VALUE.into(),
            },
            contract_2 => indexmap! {
                ACCESSED_STORAGE_KEY_2.0 => ACCESSED_STORAGE_VALUE_2.into(),
            },
        },
        class_hash_to_compiled_class_hash: indexmap! {
            class_hash => CompiledClassHash(ACCESSED_CLASS_HASH.0),
            unaccessed_class_hash => CompiledClassHash(UNACCESSED_CLASS_HASH.into()),
        },
        ..Default::default()
    }
});

static BLOCK_1_STATE_DIFF: LazyLock<ThinStateDiff> = LazyLock::new(|| ThinStateDiff {
    storage_diffs: indexmap! {
        *ACCESSED_CONTRACT_1 => indexmap! {
            ACCESSED_STORAGE_KEY_1.0 => 101_u128.into(),
            UNACCESSED_STORAGE_KEY.into() => 301_u128.into(),
        },
    },
    ..Default::default()
});

static BLOCK_1_REVERSED_STATE_DIFF: LazyLock<ThinStateDiff> = LazyLock::new(|| ThinStateDiff {
    storage_diffs: indexmap! {
        *ACCESSED_CONTRACT_1 => indexmap! {
            ACCESSED_STORAGE_KEY_1.0 => ACCESSED_STORAGE_VALUE_1.into(),
            UNACCESSED_STORAGE_KEY.into() => UNACCESSED_STORAGE_VALUE.into(),
        },
    },
    ..Default::default()
});

/// Reverses the committer's compression of the response witness
/// (`base64(zstd(serde_json(StateCommitmentInfos)))`) so the structural assertions below can
/// inspect the trie commitment infos.
fn decompress_response_commitment_infos(
    response: &ReadPathsAndCommitBlockResponse,
) -> StateCommitmentInfos {
    let compressed =
        base64::decode(&response.state_commitment_infos).expect("response witness is valid base64");
    let json = zstd::decode_all(compressed.as_slice()).expect("response witness is valid zstd");
    serde_json::from_slice(&json).expect("response witness deserializes into StateCommitmentInfos")
}

fn read_paths_and_commit_block_request(
    state_diff: ThinStateDiff,
    state_diff_commitment: Option<StateDiffCommitment>,
    height: u64,
    accessed_keys: AccessedKeys,
) -> ReadPathsAndCommitBlockRequest {
    ReadPathsAndCommitBlockRequest {
        commit: CommitBlockRequest {
            state_diff,
            state_diff_commitment,
            height: BlockNumber(height),
        },
        accessed_keys,
    }
}

/// Commits block 0 with `setup_state_diff` via [`crate::committer::Committer::commit_block`]
/// (no witnesses), then calls [`crate::committer::Committer::read_paths_and_commit_block`] at
/// block 1 with `state_diff` and `accessed_keys`, and returns its response.
async fn setup_and_read_paths(
    committer: &mut ApolloTestCommitter,
    setup_state_diff: ThinStateDiff,
    state_diff: ThinStateDiff,
    accessed_keys: AccessedKeys,
) -> ReadPathsAndCommitBlockResponse {
    committer
        .commit_block(CommitBlockRequest {
            state_diff: setup_state_diff.clone(),
            state_diff_commitment: Some(calculate_state_diff_hash(&setup_state_diff)),
            height: BlockNumber(0),
        })
        .await
        .unwrap();

    let state_diff_commitment = Some(calculate_state_diff_hash(&state_diff));
    committer
        .read_paths_and_commit_block(read_paths_and_commit_block_request(
            state_diff,
            state_diff_commitment,
            1,
            accessed_keys,
        ))
        .await
        .unwrap()
}

fn leaf_hashes<Key, PatriciaLeaf>(
    leaves: &HashMap<Key, PatriciaLeaf>,
    key_into_node_index: impl Fn(&Key) -> NodeIndex,
) -> HashMap<NodeIndex, HashOutput>
where
    PatriciaLeaf: Leaf,
    TreeHashFunctionImpl: TreeHashFunction<PatriciaLeaf>,
{
    leaves
        .iter()
        .map(|(key, leaf)| {
            (key_into_node_index(key), TreeHashFunctionImpl::compute_leaf_hash(leaf))
        })
        .collect()
}

/// Reconstructs a [`PreimageMap`] of inner nodes from a commitment info's flattened
/// `commitment_facts`. This is the inverse of `flatten_preimages`; it relies on `commitment_facts`
/// holding inner nodes only (binary `[left, right]` or edge `[length, path, bottom]`).
fn preimage_map_from_commitment_info(commitment_info: &CommitmentInfo) -> PreimageMap {
    commitment_info
        .commitment_facts
        .iter()
        .map(|(hash, raw_preimage)| {
            (*hash, Preimage::try_from(raw_preimage).expect("commitment fact is a valid preimage"))
        })
        .collect()
}

/// Verifies that `commitment_infos` contains valid Patricia proofs for the membership of each
/// accessed leaf.
fn verify_witness_patricia_paths(
    commitment_infos: &StateCommitmentInfos,
    accessed_keys: &AccessedKeys,
    class_leaves: &HashMap<ClassHash, CommitterCompiledClassHash>,
    contract_leaves: &HashMap<ContractAddress, ContractState>,
    storage_leaves: &HashMap<ContractAddress, HashMap<StarknetStorageKey, StarknetStorageValue>>,
) {
    let classes_trie_info = &commitment_infos.classes_trie_commitment_info;
    verify_patricia_proof::<CommitterCompiledClassHash, TreeHashFunctionImpl>(
        classes_trie_info.updated_root,
        &preimage_map_from_commitment_info(classes_trie_info),
        &leaf_hashes(class_leaves, class_hash_into_node_index),
    )
    .unwrap_or_else(|error| panic!("classes trie proof verification failed: {error}"));

    let contracts_trie_info = &commitment_infos.contracts_trie_commitment_info;
    verify_patricia_proof::<ContractState, TreeHashFunctionImpl>(
        contracts_trie_info.updated_root,
        &preimage_map_from_commitment_info(contracts_trie_info),
        &leaf_hashes(contract_leaves, contract_address_into_node_index),
    )
    .unwrap_or_else(|error| panic!("contracts trie proof verification failed: {error}"));

    for contract_address in &accessed_keys.accessed_contracts {
        let storage_info =
            commitment_infos.storage_tries_commitment_infos.get(contract_address).unwrap_or_else(
                || panic!("missing storage trie commitment info for {contract_address:?}"),
            );
        verify_patricia_proof::<StarknetStorageValue, TreeHashFunctionImpl>(
            storage_info.updated_root,
            &preimage_map_from_commitment_info(storage_info),
            &leaf_hashes(
                storage_leaves.get(contract_address).unwrap_or_else(|| {
                    panic!("missing storage leaves for contract {contract_address:?}")
                }),
                |key| NodeIndex::from(key),
            ),
        )
        .unwrap_or_else(|error| {
            panic!("storage trie proof verification failed for {contract_address:?}: {error}")
        });
    }
}

async fn assert_witnesses_and_digest_present(
    committer: &mut ApolloTestCommitter,
    height: BlockNumber,
    expected_commitment_infos: &StateCommitmentInfos,
) {
    assert_eq!(
        committer.load_witnesses_digest(height).await.unwrap(),
        Some(*EXPECTED_ACCESSED_KEYS_DIGEST),
    );
    assert_eq!(
        committer.forest_storage.read_commitment_infos(height).await.unwrap().as_ref(),
        Some(expected_commitment_infos),
    );
}

async fn assert_witnesses_and_digest_absent(
    committer: &mut ApolloTestCommitter,
    height: BlockNumber,
) {
    assert!(committer.load_witnesses_digest(height).await.unwrap().is_none());
    assert!(committer.forest_storage.read_commitment_infos(height).await.unwrap().is_none());
}

/// Flow overview:
/// 1. Commit block 0 via [crate::committer::Committer::read_paths_and_commit_block], requesting
///    witnesses for [`ACCESSED_KEYS`].
/// 2. Verify the returned Patricia proofs via [verify_witness_patricia_paths].
/// 3. Clear trie storage and replay the same request to verify witnesses are loaded from storage
///    rather than recomputed.
/// 4. Assert witnesses and the accessed-keys digest are stored for block 0 via
///    [assert_witnesses_and_digest_present].
#[tokio::test]
async fn read_paths_and_commit_block_happy_flow() {
    let mut committer = new_test_committer().await;
    let height = 0;
    let state_diff = BLOCK_0_STATE_DIFF.clone();
    let state_diff_commitment = Some(calculate_state_diff_hash(&state_diff));
    let accessed_keys = ACCESSED_KEYS.clone();
    let request = read_paths_and_commit_block_request(
        state_diff,
        state_diff_commitment,
        height,
        accessed_keys.clone(),
    );

    let response = committer.read_paths_and_commit_block(request.clone()).await.unwrap();
    assert_eq!(committer.offset, BlockNumber(height + 1));
    let commitment_infos = decompress_response_commitment_infos(&response);
    // The commitment infos don't retain the contract-trie leaves, so reconstruct the expected
    // contract states for the accessed contracts: storage root from the commitment infos, class
    // hash from the block-0 deployment, and the default (zero) nonce.
    let contract_leaves: HashMap<ContractAddress, ContractState> = commitment_infos
        .storage_tries_commitment_infos
        .iter()
        .map(|(address, storage_info)| {
            (
                *address,
                ContractState {
                    nonce: Nonce::default(),
                    storage_root_hash: storage_info.updated_root,
                    class_hash: *ACCESSED_CLASS_HASH,
                },
            )
        })
        .collect();
    verify_witness_patricia_paths(
        &commitment_infos,
        &accessed_keys,
        &ACCESSED_CLASS_LEAVES,
        &contract_leaves,
        &ACCESSED_STORAGE_LEAVES,
    );

    // Historical replay should load the persisted commitment infos; remove trie nodes to assert
    // the historical path reads from storage rather than recomputing.
    committer.forest_storage.clear_patricia_trie_nodes_for_test();

    let replay_response = committer.read_paths_and_commit_block(request).await.unwrap();
    assert_eq!(response.global_root, replay_response.global_root);
    // Compare the decompressed witnesses, not the compressed strings: `StateCommitmentInfos`
    // contains `HashMap`s whose JSON serialization order is not stable, so two equal witnesses can
    // compress to different byte strings.
    assert_eq!(commitment_infos, decompress_response_commitment_infos(&replay_response));
    assert_witnesses_and_digest_present(&mut committer, BlockNumber(height), &commitment_infos)
        .await;
}

/// Flow overview:
/// 1. Commit block 0 via [crate::committer::Committer::commit_block] (no witnesses fetched).
/// 2. Commit block 1 via [crate::committer::Committer::read_paths_and_commit_block], requesting
///    witnesses for [`ACCESSED_KEYS`].
/// 3. Assert witnesses and the accessed-keys digest are present for block 1 via
///    [assert_witnesses_and_digest_present].
/// 4. Revert block 1 via [crate::committer::Committer::revert_block].
/// 5. Assert witnesses and the accessed-keys digest are absent for block 1 via
///    [assert_witnesses_and_digest_absent].
#[tokio::test]
async fn revert_removes_witnesses_and_digest() {
    let mut committer = new_test_committer().await;
    let height_1 = 1;
    let block_1_state_diff = BLOCK_1_STATE_DIFF.clone();
    let accessed_keys = ACCESSED_KEYS.clone();

    let block_1_response = setup_and_read_paths(
        &mut committer,
        BLOCK_0_STATE_DIFF.clone(),
        block_1_state_diff,
        accessed_keys.clone(),
    )
    .await;
    assert_witnesses_and_digest_present(
        &mut committer,
        BlockNumber(height_1),
        &decompress_response_commitment_infos(&block_1_response),
    )
    .await;

    committer
        .revert_block(RevertBlockRequest {
            reversed_state_diff: BLOCK_1_REVERSED_STATE_DIFF.clone(),
            height: BlockNumber(height_1),
        })
        .await
        .unwrap();
    assert_witnesses_and_digest_absent(&mut committer, BlockNumber(height_1)).await;
    assert_eq!(committer.offset, BlockNumber(height_1));
}

/// Flow overview:
/// 1. Commit block 0 with three class leaves that form this Patricia topology:
/// ```text
///         R
///        / \
///       E   F
///       |   |
///       G   |
///      / \   \
///     A   B   D
/// ```
/// 2. Commit block 1 via [crate::committer::Committer::read_paths_and_commit_block], deleting `D`
///    and requesting witnesses only for the deleted key.
/// 3. Assert the returned classes-trie proof contains node `G` (not strictly necessary, see comment
///    below).
#[tokio::test]
async fn test_bottom_of_new_edge_to_an_unmoidifed_subtree_is_present() {
    // Set the two leftmost and the rightmost leaves.
    let class_hash_a = ClassHash(0_u64.into());
    let class_hash_b = ClassHash(1_u64.into());
    let class_hash_d = ClassHash(PATRICIA_KEY_UPPER_BOUND_FELT - 1_u64);

    let compiled_class_hash_a_felt = 100_u64.into();
    let compiled_class_hash_b_felt = 101_u64.into();
    let compiled_class_hash_d_felt = 102_u64.into();

    let mut committer = new_test_committer().await;
    let setup_state_diff = ThinStateDiff {
        class_hash_to_compiled_class_hash: indexmap! {
            class_hash_a => CompiledClassHash(compiled_class_hash_a_felt),
            class_hash_b => CompiledClassHash(compiled_class_hash_b_felt),
            class_hash_d => CompiledClassHash(compiled_class_hash_d_felt),
        },
        ..Default::default()
    };
    let block_1_state_diff = ThinStateDiff {
        class_hash_to_compiled_class_hash: indexmap! {
            class_hash_d => CompiledClassHash(0_u64.into()),
        },
        ..Default::default()
    };
    let accessed_keys = AccessedKeys {
        accessed_class_hashes: BTreeSet::from([class_hash_d]),
        ..Default::default()
    };

    let response =
        setup_and_read_paths(&mut committer, setup_state_diff, block_1_state_diff, accessed_keys)
            .await;

    let leaf_a_hash = TreeHashFunctionImpl::compute_leaf_hash(&CommitterCompiledClassHash(
        compiled_class_hash_a_felt,
    ));
    let leaf_b_hash = TreeHashFunctionImpl::compute_leaf_hash(&CommitterCompiledClassHash(
        compiled_class_hash_b_felt,
    ));
    let node_g_hash = TreeHashFunctionImpl::compute_node_hash(&NodeData::<
        CommitterCompiledClassHash,
        HashOutput,
    >::Binary(BinaryData {
        left_data: leaf_a_hash,
        right_data: leaf_b_hash,
    }));

    // TODO(Ariel): the preimage of G is not really needed by the OS (it only needs R, F, and the
    // new root R', whose opening contains the hash of G). Change this to not contains or delete
    // this test after making request_paths_and_commit_block_request stricter.
    //
    // For completeness, the OS verifies:
    // hash(G_hash, truncated_path) + (len([R',G]) - 1) == E_hash.
    // This in turn also proves that G is not an edge node, as it's the bottom of an old
    // edge node, without explicitly requesting an opening of E.
    assert!(
        decompress_response_commitment_infos(&response)
            .classes_trie_commitment_info
            .commitment_facts
            .contains_key(&node_g_hash),
        "missing bottom of a new edge node in a proof",
    );
}

/// Flow overview:
/// 1. Commit block 0 with three class leaves that form this Patricia topology:
/// ```text
///         R
///         |
///         T
///        / \
///       E   F
///      / \   \
///     A   B   D
/// ```
/// 2. Commit block 1 via [crate::committer::Committer::read_paths_and_commit_block], deleting `D`
///    and requesting witnesses only for the deleted key.
/// 3. Assert the returned classes-trie proof contains node `E` (this will allow verifying that the
///    new edge's bottom is not an edge).
#[tokio::test]
async fn test_bottom_of_new_edge_which_was_not_bottom_of_an_old_edge_is_present() {
    let class_hash_a = ClassHash(0_u64.into());
    let class_hash_b = ClassHash(1_u64.into());
    let class_hash_d = ClassHash(3_u64.into());

    let compiled_class_hash_a_felt = 100_u64.into();
    let compiled_class_hash_b_felt = 101_u64.into();
    let compiled_class_hash_d_felt = 102_u64.into();

    let mut committer = new_test_committer().await;
    let setup_state_diff = ThinStateDiff {
        class_hash_to_compiled_class_hash: indexmap! {
            class_hash_a => CompiledClassHash(compiled_class_hash_a_felt),
            class_hash_b => CompiledClassHash(compiled_class_hash_b_felt),
            class_hash_d => CompiledClassHash(compiled_class_hash_d_felt),
        },
        ..Default::default()
    };
    let block_1_state_diff = ThinStateDiff {
        class_hash_to_compiled_class_hash: indexmap! {
            class_hash_d => CompiledClassHash(0_u64.into()),
        },
        ..Default::default()
    };
    let accessed_keys = AccessedKeys {
        accessed_class_hashes: BTreeSet::from([class_hash_d]),
        ..Default::default()
    };

    let response =
        setup_and_read_paths(&mut committer, setup_state_diff, block_1_state_diff, accessed_keys)
            .await;

    let leaf_a_hash = TreeHashFunctionImpl::compute_leaf_hash(&CommitterCompiledClassHash(
        compiled_class_hash_a_felt,
    ));
    let leaf_b_hash = TreeHashFunctionImpl::compute_leaf_hash(&CommitterCompiledClassHash(
        compiled_class_hash_b_felt,
    ));
    let node_e_hash = TreeHashFunctionImpl::compute_node_hash(&NodeData::<
        CommitterCompiledClassHash,
        HashOutput,
    >::Binary(BinaryData {
        left_data: leaf_a_hash,
        right_data: leaf_b_hash,
    }));

    assert!(
        decompress_response_commitment_infos(&response)
            .classes_trie_commitment_info
            .commitment_facts
            .contains_key(&node_e_hash),
        "missing bottom of a new edge node in a proof",
    );
}
