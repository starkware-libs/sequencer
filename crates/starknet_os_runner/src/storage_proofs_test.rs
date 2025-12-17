use std::env;

use blockifier::state::cached_state::StateMaps;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_rust::providers::Provider;
use starknet_types_core::felt::Felt;
use url::Url;

use crate::storage_proofs::{RpcStorageProofsProvider, StorageProofProvider};

/// Mainnet STRK token contract address.
const STRK_CONTRACT_ADDRESS: Felt =
    Felt::from_hex_unchecked("0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d");

/// Fixture: Creates initial reads with the STRK contract and storage slot 0.
#[fixture]
fn initial_reads() -> (StateMaps, ContractAddress, StorageKey) {
    let mut state_maps = StateMaps::default();
    let contract_address = ContractAddress::try_from(STRK_CONTRACT_ADDRESS).unwrap();

    // Add a storage read for slot 0 (commonly used for total_supply or similar).
    let storage_key = StorageKey::from(0u32);
    state_maps.storage.insert((contract_address, storage_key), Felt::ZERO);

    (state_maps, contract_address, storage_key)
}

/// Fixture: Creates an RPC provider from the RPC_URL environment variable.
#[fixture]
fn rpc_provider() -> RpcStorageProofsProvider {
    let rpc_url_str = env::var("RPC_URL").expect("RPC_URL environment variable must be set");
    let rpc_url = Url::parse(&rpc_url_str).expect("Invalid RPC URL");
    RpcStorageProofsProvider::new(rpc_url)
}

/// Sanity test that verifies storage proof fetching works with a real RPC endpoint.
///
/// This test is ignored by default because it requires a running RPC node.
/// Run with: `RPC_URL=<your_rpc_url> cargo test -p starknet_os_runner -- --ignored`
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

    let result = rpc_provider.get_storage_proofs(BlockNumber(block_number), &state_maps);
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

    // Verify the queried contract is in cached_state_input.
    assert!(
        storage_proofs.cached_state_input.address_to_class_hash.contains_key(&contract_address),
        "Expected contract address {:?} in address_to_class_hash",
        contract_address
    );
    assert!(
        storage_proofs.cached_state_input.address_to_nonce.contains_key(&contract_address),
        "Expected contract address {:?} in address_to_nonce",
        contract_address
    );

    // Verify the contract has storage data (STRK contract should have storage).
    assert!(
        storage_proofs.cached_state_input.storage.contains_key(&contract_address),
        "Expected contract address {:?} in storage",
        contract_address
    );
    assert!(
        storage_proofs.cached_state_input.storage[&contract_address].contains_key(&storage_key),
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
