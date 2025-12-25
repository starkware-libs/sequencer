use std::collections::{HashMap, HashSet};

use bitvec::order::Msb0;
use bitvec::prelude::BitVec;
use blockifier::context::BlockContext;
use blockifier::state::cached_state::StateMaps;
use rand::seq::IteratorRandom;
use rstest::rstest;
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::hash::HashOutput;
use starknet_api::state::StorageKey;
use starknet_patricia::patricia_merkle_tree::node_data::inner_node::Preimage;
use starknet_patricia::patricia_merkle_tree::types::SubTreeHeight;
use starknet_rust::providers::Provider;
use starknet_types_core::felt::Felt;

use crate::storage_proofs::{RpcStorageProofsProvider, StorageProofProvider};
use crate::test_utils::{rpc_provider, STRK_TOKEN_ADDRESS, STRK_TOKEN_STORAGE_KEY};
use crate::virtual_block_executor::VirtualBlockExecutionData;

/// Verifies that a storage proof contains all nodes needed to traverse from root to the given key.
///
/// This checks that the OS will be able to process the proof without encountering missing preimage
/// errors.
///
/// # Arguments
/// * `storage_root` - The root hash of the storage tree
/// * `storage_key` - The key to traverse to
/// * `commitment_facts` - Map of node hashes to their preimages
/// * `key_exists` - Whether the key is expected to exist in the tree. If true, panics on
///   non-existence proof.
///
/// # Algorithm
/// 1. Convert storage_key to bit path (251 bits, skip first 5 bits)
/// 2. Start at storage_root
/// 3. Traverse the tree:
///    - Lookup current node hash in commitment_facts
///    - If not found → return error
///    - Parse the node (length 2 = binary, length 3 = edge)
///    - For binary: go left (bit=0) or right (bit=1)
///    - For edge: follow path, advance index by path length, then get bottom node
///    - Continue until we reach the end (index == 256)
/// 4. Panic if the proof is incomplete.
fn verify_storage_proof_completeness(
    storage_root: HashOutput,
    storage_key: Felt,
    commitment_facts: &HashMap<HashOutput, Vec<Felt>>,
    key_exists: bool,
) {
    // Tree height is 251 (SubTreeHeight::ACTUAL_HEIGHT).
    // EdgePath uses 251 bits (EdgePath::BITS).
    // Felt is 256 bits, so the first 5 bits are ignored (256 - 251 = 5).
    const FELT_BITS: usize = 256; // Size of Felt in bits. 
    const START_BIT: usize = FELT_BITS - SubTreeHeight::ACTUAL_HEIGHT.0 as usize;

    // Convert key to bits (MSB first).
    let bits: BitVec<_, Msb0> = BitVec::from_slice(&storage_key.to_bytes_be());

    let mut index = START_BIT;
    let mut next_node_hash = storage_root;

    while index < FELT_BITS {
        // Lookup the node in commitment_facts.
        let raw_preimage = commitment_facts.get(&next_node_hash).expect(format!(
            "Missing node in commitment_facts: hash=0x{:x} at index={}",
            next_node_hash.0, index
        ));

        // Parse the preimage
        let preimage =
            Preimage::try_from(raw_preimage).expect(format!("Failed to parse preimage: {:?}", e));

        match preimage {
            Preimage::Binary(binary_data) => {
                // Binary node: go left or right based on the current bit
                next_node_hash =
                    if bits[index] { binary_data.right_data } else { binary_data.left_data };
                index += 1;
            }
            Preimage::Edge(edge_data) => {
                // Edge node: follow the path
                let path_length = u8::from(edge_data.path_to_bottom.length) as usize;

                // Extract the relevant bits from our key.
                if index + path_length > FELT_BITS {
                    panic!(format!(
                        "Edge path extends beyond tree height: index={}, path_length={}, total={}",
                        index, path_length, FELT_BITS
                    ));
                }

                let relevant_key_path = &bits[index..index + path_length];

                // Extract the edge path bits.
                // EdgePath::BITS is 251, but U256 is stored in 256 bits.
                let edge_path_bits: BitVec<_, Msb0> =
                    BitVec::from_slice(&edge_data.path_to_bottom.path.0.to_be_bytes());

                // Take exactly path_length bits starting from position (FELT_BITS - path_length).
                // This gets the rightmost path_length bits from the right-aligned U256 value.
                let start_bit = FELT_BITS - path_length;
                let relevant_edge_path = &edge_path_bits[start_bit..start_bit + path_length];

                // Check if paths match.
                if relevant_key_path != relevant_edge_path {
                    // Paths don't match - this is a proof of non-existence
                    // The key doesn't exist in the tree, but the proof is still valid.
                    if key_exists {
                        panic!(
                            "Expected key 0x{:x} to exist, but got non-existence proof at index \
                             {}. Edge path mismatch: key_path={:?}, edge_path={:?}",
                            storage_key,
                            index,
                            relevant_key_path.iter().map(|b| *b as u8).collect::<Vec<_>>(),
                            relevant_edge_path.iter().map(|b| *b as u8).collect::<Vec<_>>()
                        );
                    }
                    return Ok(());
                }

                // Follow the edge to its bottom node.
                next_node_hash = edge_data.bottom_data;
                index += path_length;
            }
        }
    }

    // If we've traversed all 256 bits, we've reached a leaf.
    Ok(())
}

/// Fixture: Creates initial reads with the test contract.
#[rstest::fixture]
fn initial_reads() -> (StateMaps, ContractAddress, StorageKey) {
    let mut state_maps = StateMaps::default();
    let contract_address = ContractAddress::try_from(STRK_TOKEN_ADDRESS).unwrap();

    // Add a storage read for slot 0.
    let storage_key = StorageKey::from(0u32);
    state_maps.storage.insert((contract_address, storage_key), Felt::ZERO);

    (state_maps, contract_address, storage_key)
}

/// Sanity test that verifies storage proof fetching works with a real RPC endpoint.
///
/// This test is ignored by default because it requires a running RPC node.
/// Run with: `NODE_URL=<your_rpc_url> cargo test -p starknet_os_runner -- --ignored`
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
        block_context: BlockContext::create_for_account_testing(),
        initial_reads: state_maps,
        executed_class_hashes: HashSet::new(),
    };

    let result = rpc_provider.get_storage_proofs(BlockNumber(block_number), &execution_data);
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

    // Verify the queried contract is in proof_state.
    assert!(
        storage_proofs.proof_state.class_hashes.contains_key(&contract_address),
        "Expected contract address {:?} in class_hashes",
        contract_address
    );
    assert!(
        storage_proofs.proof_state.nonces.contains_key(&contract_address),
        "Expected contract address {:?} in nonces",
        contract_address
    );

    // Verify the queried storage is in the original execution_data (not proof_state, which only has
    // nonces/hashes).
    assert!(
        execution_data.initial_reads.storage.contains_key(&(contract_address, storage_key)),
        "Expected storage key {:?} in contract's storage",
        storage_key
    );

    // Verify the contract has a storage trie commitment info.
    assert!(
        storage_proofs
            .commitment_infos
            .storage_tries_commitment_infos
            .contains_key(&contract_address),
        "Expected contract address {:?} in storage_tries_commitment_infos",
        contract_address
    );

    // Verify the storage trie commitment facts are not empty.
    let storage_commitment =
        &storage_proofs.commitment_infos.storage_tries_commitment_infos[&contract_address];
    assert!(
        !storage_commitment.commitment_facts.is_empty(),
        "Expected non-empty storage trie commitment facts for contract {:?}",
        contract_address
    );

    // Verify the storage root is non-zero.
    assert!(
        storage_commitment.previous_root.0 != Felt::ZERO,
        "Expected non-zero storage root for contract {:?}",
        contract_address
    );
}

/// Test storage proof verification with multiple cases.
///
/// This test verifies that RPC storage proofs contain all nodes needed for the OS to process them
/// without encountering missing preimage errors.
///
/// Test cases:
/// - existing_key: A storage key that definitely exists in the test contract (complete proof)
/// - non_existing_key: A random key that likely doesn't exist (complete proof of non-existence)
/// - existing_key_incomplete: Existing key with a node removed (should panic)
/// - non_existing_key_incomplete: Non-existing key with a node removed (should panic)
///
/// Run with: `NODE_URL=<your_rpc_url> cargo test -p starknet_os_runner -- --ignored
/// test_verify_storage_proof`
#[rstest]
#[case::existing_key(
    StorageKey::try_from(STRK_TOKEN_STORAGE_KEY).unwrap(),
    "storage key that exists",
    true,
    false
)]
#[case::non_existing_key(StorageKey::from(999999u32), "non-existing storage key", false, false)]
#[should_panic(expected = "Missing node in commitment_facts")]
#[case::existing_key_incomplete(
    StorageKey::try_from(STRK_TOKEN_STORAGE_KEY).unwrap(),
    "storage key that exists (incomplete proof)",
    true,
    true
)]
#[should_panic(expected = "Missing node in commitment_facts")]
#[case::non_existing_key_incomplete(
    StorageKey::from(999999u32),
    "non-existing storage key (incomplete proof)",
    false,
    true
)]
#[ignore]
fn test_verify_storage_proof(
    rpc_provider: RpcStorageProofsProvider,
    #[case] storage_key: StorageKey,
    #[case] description: &str,
    #[case] key_exists: bool,
    #[case] remove_random_node: bool,
) {
    let contract_address = ContractAddress::try_from(STRK_TOKEN_ADDRESS).unwrap();
    // Build initial_reads with the storage key.
    let mut state_maps = StateMaps::default();
    state_maps.storage.insert((contract_address, storage_key), Felt::ZERO);

    // Fetch proof from RPC.
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let block_number = runtime.block_on(async { rpc_provider.0.block_number().await }).unwrap();

    let execution_data = VirtualBlockExecutionData {
        execution_outputs: vec![],
        block_context: BlockContext::create_for_account_testing(),
        initial_reads: state_maps,
        executed_class_hashes: HashSet::new(),
    };

    let mut storage_proofs = rpc_provider
        .get_storage_proofs(BlockNumber(block_number), &execution_data)
        .expect("Failed to get storage proofs");

    // Verify the contract has a storage trie commitment info.
    assert!(
        storage_proofs
            .commitment_infos
            .storage_tries_commitment_infos
            .contains_key(&contract_address),
        "Expected contract address {:?} in storage_tries_commitment_infos",
        contract_address
    );

    // Get a mutable reference to the storage commitment info for verification.
    let storage_commitment_info = storage_proofs
        .commitment_infos
        .storage_tries_commitment_infos
        .get_mut(&contract_address)
        .expect("Expected contract address in storage_tries_commitment_infos");

    // Conditionally remove a random node to test incomplete proof handling.
    if remove_random_node {
        let mut rng = rand::thread_rng();
        // Remove a random node (not the root) to create an incomplete proof.
        if let Some(&hash_to_remove) = storage_commitment_info
            .commitment_facts
            .keys()
            .filter(|&k| k != &storage_commitment_info.previous_root)
            .choose(&mut rng)
        {
            storage_commitment_info.commitment_facts.remove(&hash_to_remove);
        }
    }

    // Verify the proof is complete - all nodes from root to key are present.
    // This will panic with the error message if the proof is incomplete.
    verify_storage_proof_completeness(
        storage_commitment_info.previous_root,
        *storage_key.0.key(),
        &storage_commitment_info.commitment_facts,
        key_exists,
    )
    .unwrap_or_else(|e| panic!("Proof verification failed for {}: {}", description, e));
}
