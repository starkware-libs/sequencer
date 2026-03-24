use std::collections::HashSet;

use blockifier::context::BlockContext;
use blockifier::state::cached_state::StateMaps;
use rstest::rstest;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::block_hash::block_hash_calculator::BlockHeaderCommitments;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_rust::providers::Provider;
use starknet_rust_core::types::{
    BinaryNode,
    ContractLeafData,
    ContractStorageKeys,
    ContractsProof,
    GlobalRoots,
    MerkleNode,
    StorageProof as RpcStorageProof,
};
use starknet_types_core::felt::Felt;

use crate::running::storage_proofs::{
    collect_query,
    count_total_keys,
    flatten_query,
    merge_storage_proofs,
    split_query,
    RpcStorageProofsProvider,
    RpcStorageProofsQuery,
    StorageProofConfig,
    StorageProofProvider,
};
use crate::running::virtual_block_executor::{BaseBlockInfo, VirtualBlockExecutionData};
use crate::test_utils::{rpc_provider, STRK_TOKEN_ADDRESS};

/// Fixture: Creates initial reads with the STRK contract and storage slot 0.
#[rstest::fixture]
fn initial_reads() -> (StateMaps, ContractAddress, StorageKey) {
    let mut state_maps = StateMaps::default();
    let contract_address = ContractAddress::try_from(STRK_TOKEN_ADDRESS).unwrap();

    // Add a storage read for slot 0 (commonly used for total_supply or similar).
    let storage_key = StorageKey::from(0u32);
    state_maps.storage.insert((contract_address, storage_key), Felt::ZERO);

    (state_maps, contract_address, storage_key)
}

/// Sanity test that verifies storage proof fetching works with a real RPC endpoint.
///
/// This test is ignored by default because it requires a running RPC node.
/// Run with: `NODE_URL=<your_rpc_url> cargo test -p starknet_transaction_prover -- --ignored`
#[rstest]
#[ignore]
fn test_get_storage_proofs_from_rpc(
    rpc_provider: RpcStorageProofsProvider,
    initial_reads: (StateMaps, ContractAddress, StorageKey),
) {
    let (state_maps, contract_address, storage_key) = initial_reads;

    // Fetch latest block number.
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let block_number = runtime.block_on(async { rpc_provider.0.block_number().await }).unwrap();

    let execution_data = VirtualBlockExecutionData {
        execution_outputs: vec![],
        l2_to_l1_messages: Vec::new(),
        base_block_info: BaseBlockInfo {
            block_context: BlockContext::create_for_account_testing(),
            base_block_hash: BlockHash::default(),
            prev_base_block_hash: BlockHash::default(),
            base_block_header_commitments: BlockHeaderCommitments::default(),
        },
        initial_reads: state_maps,
        state_diff: StateMaps::default(),
        executed_class_hashes: HashSet::new(),
    };

    let config = StorageProofConfig::default();
    let result = runtime.block_on(async {
        rpc_provider.get_storage_proofs(BlockNumber(block_number), &execution_data, &config).await
    });
    assert!(result.is_ok(), "Failed to get storage proofs: {:?}", result.err());

    let storage_proofs = result.unwrap();

    // Verify contracts tree root is non-zero.
    assert!(
        storage_proofs.commitment_infos.contracts_trie_commitment_info.previous_root.0
            != Felt::ZERO,
        "Expected non-zero contracts tree root"
    );

    // Verify contracts tree commitment facts are not empty.
    assert!(
        !storage_proofs.commitment_infos.contracts_trie_commitment_info.commitment_facts.is_empty(),
        "Expected non-empty contracts tree commitment facts"
    );

    // Verify the queried contract is in contract_leaf_state.
    assert!(
        storage_proofs.extended_initial_reads.class_hashes.contains_key(&contract_address),
        "Expected contract address {contract_address:?} in class_hashes",
    );
    assert!(
        storage_proofs.extended_initial_reads.nonces.contains_key(&contract_address),
        "Expected contract address {contract_address:?} in nonces",
    );

    // Verify the queried storage is in the original execution_data (not contract_leaf_state,
    // which only has nonces/hashes)
    assert!(
        execution_data.initial_reads.storage.contains_key(&(contract_address, storage_key)),
        "Expected storage key {storage_key:?} in contract's storage",
    );

    // Verify the contract has a storage trie commitment info.
    assert!(
        storage_proofs
            .commitment_infos
            .storage_tries_commitment_infos
            .contains_key(&contract_address),
        "Expected contract address {contract_address:?} in storage_tries_commitment_infos",
    );

    // Verify the storage trie commitment facts are not empty.
    let storage_commitment =
        &storage_proofs.commitment_infos.storage_tries_commitment_infos[&contract_address];
    assert!(
        !storage_commitment.commitment_facts.is_empty(),
        "Expected non-empty storage trie commitment facts for contract {contract_address:?}",
    );

    // Verify the storage root is non-zero.
    assert!(
        storage_commitment.previous_root.0 != Felt::ZERO,
        "Expected non-zero storage root for contract {contract_address:?}",
    );
}

pub(crate) fn make_query(
    n_class_hashes: u64,
    n_contract_addresses: u64,
    storage_keys_per_contract: &[u64],
) -> RpcStorageProofsQuery {
    let class_hashes: Vec<Felt> = (0..n_class_hashes).map(Felt::from).collect();
    let contract_addresses: Vec<ContractAddress> = (0..n_contract_addresses)
        .map(|i| ContractAddress::try_from(Felt::from(1000 + i)).unwrap())
        .collect();
    let contract_storage_keys: Vec<ContractStorageKeys> = storage_keys_per_contract
        .iter()
        .zip(2000_u64..)
        .map(|(&n_keys, addr)| ContractStorageKeys {
            contract_address: Felt::from(addr),
            storage_keys: (0..n_keys).map(Felt::from).collect(),
        })
        .collect();
    RpcStorageProofsQuery { class_hashes, contract_addresses, contract_storage_keys }
}

/// Builds a deterministic `RpcStorageProof` purely from the query content.
/// Using the actual key values as IndexMap keys means split+merge must reproduce the same result.
fn make_dummy_proof(query: &RpcStorageProofsQuery) -> RpcStorageProof {
    let node =
        MerkleNode::BinaryNode(BinaryNode { left: Felt::from(0u64), right: Felt::from(1u64) });

    let classes_proof = query.class_hashes.iter().map(|h| (*h, node.clone())).collect();
    let contracts_nodes =
        query.contract_addresses.iter().map(|a| (*a.0.key(), node.clone())).collect();
    let contract_leaves_data = query
        .contract_addresses
        .iter()
        .map(|addr| ContractLeafData {
            nonce: *addr.0.key(),
            class_hash: Felt::from(42u64),
            storage_root: Some(Felt::from(99u64)),
        })
        .collect();
    let contracts_storage_proofs = query
        .contract_storage_keys
        .iter()
        .map(|csk| csk.storage_keys.iter().map(|k| (*k, node.clone())).collect())
        .collect();

    RpcStorageProof {
        classes_proof,
        contracts_proof: ContractsProof { nodes: contracts_nodes, contract_leaves_data },
        contracts_storage_proofs,
        global_roots: GlobalRoots {
            contracts_tree_root: Felt::from(100u64),
            classes_tree_root: Felt::from(200u64),
            block_hash: Felt::from(300u64),
        },
    }
}

fn assert_split_merge_identity(query: &RpcStorageProofsQuery, max_keys: usize) {
    let expected = make_dummy_proof(query);
    let chunks = split_query(query, max_keys);
    let proofs: Vec<_> = chunks.iter().map(make_dummy_proof).collect();
    let merged = merge_storage_proofs(proofs, &chunks, query);
    assert_eq!(merged, expected);
}

#[test]
fn test_flatten_collect_roundtrip() {
    let query = make_query(3, 5, &[10, 7]);
    let items = flatten_query(&query);
    assert_eq!(items.len(), count_total_keys(&query));
    let reconstructed = collect_query(&items);
    assert_eq!(reconstructed, query);
}

#[test]
fn test_flatten_collect_drops_empty_storage_entries() {
    let mut query = make_query(1, 1, &[3]);
    // Add a contract storage entry with no keys (e.g. from `unwrap_or_default` in prepare_query).
    query
        .contract_storage_keys
        .push(ContractStorageKeys { contract_address: Felt::from(9999_u64), storage_keys: vec![] });

    let items = flatten_query(&query);
    let reconstructed = collect_query(&items);

    // The empty-key entry is intentionally dropped: it has no effect on the proof result.
    assert_eq!(reconstructed.contract_storage_keys.len(), 1);
    assert_eq!(reconstructed, make_query(1, 1, &[3]));
}

#[test]
fn test_split_query() {
    // 3 class_hashes + 2 addresses + [4, 3] storage keys = 12 total, max=5.
    let query = make_query(3, 2, &[4, 3]);
    let chunks = split_query(&query, 5);
    assert_eq!(chunks.len(), 3);
    assert_eq!(count_total_keys(&chunks[0]), 5); // 3ch + 2addr
    assert_eq!(count_total_keys(&chunks[1]), 5); // 4sk + 1sk
    assert_eq!(count_total_keys(&chunks[2]), 2); // 2sk
    assert_eq!(chunks.iter().map(count_total_keys).sum::<usize>(), 12);
}

#[test]
fn test_split_query_empty() {
    let query = make_query(0, 0, &[]);
    let chunks = split_query(&query, 100);
    assert!(chunks.is_empty());
}

#[test]
#[should_panic(expected = "cannot merge zero proofs")]
fn test_merge_empty_panics() {
    let query = make_query(0, 0, &[]);
    merge_storage_proofs(vec![], &[], &query);
}

#[test]
fn test_split_merge_roundtrip() {
    // Exceeds Pathfinder's 100-key limit: mixed class hashes, addresses, and storage keys.
    assert_split_merge_identity(&make_query(5, 15, &[30, 25, 20, 10, 5, 4]), 100);
    // Single contract's keys split across many chunks.
    assert_split_merge_identity(&make_query(0, 0, &[50]), 7);
    // Everything fits in one chunk (identity case).
    assert_split_merge_identity(&make_query(2, 3, &[4, 5]), 100);
    // Only class hashes and addresses, no storage keys.
    assert_split_merge_identity(&make_query(10, 20, &[]), 15);
    // Tight limit forces many small chunks.
    assert_split_merge_identity(&make_query(3, 4, &[8, 6, 3]), 3);
}
