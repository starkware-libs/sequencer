use std::collections::HashSet;

use blockifier::context::BlockContext;
use blockifier::state::cached_state::StateMaps;
use rstest::rstest;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_rust::providers::Provider;
use starknet_types_core::felt::Felt;

use crate::storage_proofs::{RpcStorageProofsProvider, StorageProofProvider};
use crate::test_utils::{rpc_provider, STRK_TOKEN_ADDRESS};
use crate::virtual_block_executor::{BaseBlockInfo, VirtualBlockExecutionData};

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
        base_block_info: BaseBlockInfo {
            block_context: BlockContext::create_for_account_testing(),
            base_block_hash: BlockHash::default(),
            prev_base_block_hash: BlockHash::default(),
        },
        initial_reads: state_maps,
        executed_class_hashes: HashSet::new(),
    };

    let result = runtime.block_on(async {
        rpc_provider.get_storage_proofs(BlockNumber(block_number), &execution_data).await
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
    // nonces/hashes)
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
