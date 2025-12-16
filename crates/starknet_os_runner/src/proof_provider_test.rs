//! Tests for RpcProofProvider.

use std::env;

use blockifier::state::cached_state::StateMaps;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::state::StorageKey;
use starknet_rust::providers::jsonrpc::HttpTransport;
use starknet_rust::providers::JsonRpcClient;
use starknet_types_core::felt::Felt;
use url::Url;

use crate::proof_provider::RpcProofProvider;

/// Environment variable name for the RPC URL.
const RPC_URL_ENV_VAR: &str = "STARKNET_RPC_URL";

/// ETH token contract address on Starknet (same on mainnet and Sepolia).
const ETH_TOKEN_ADDRESS: &str =
    "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7";

/// Block number for testing (Sepolia).
const TEST_BLOCK_NUMBER: u64 = 4292694;

/// Create an RPC client from the environment variable.
fn create_rpc_client() -> JsonRpcClient<HttpTransport> {
    let rpc_url = env::var(RPC_URL_ENV_VAR)
        .unwrap_or_else(|_| panic!("Environment variable {} must be set", RPC_URL_ENV_VAR));
    let url = Url::parse(&rpc_url).expect("Invalid RPC URL");
    JsonRpcClient::new(HttpTransport::new(url))
}

#[tokio::test]
async fn test_fetch_storage_proof_single_storage_read() {
    // Skip test if RPC URL is not set.
    if env::var(RPC_URL_ENV_VAR).is_err() {
        eprintln!("Skipping test: {} not set", RPC_URL_ENV_VAR);
        return;
    }

    // Create provider.
    let client = create_rpc_client();
    let provider = RpcProofProvider::new(client);

    // Create initial reads with a single storage read from ETH token contract.
    let mut initial_reads = StateMaps::default();

    let eth_token_address = ContractAddress(
        PatriciaKey::try_from(Felt::from_hex_unchecked(ETH_TOKEN_ADDRESS))
            .expect("Invalid address"),
    );

    // Read storage key 0 (a common storage slot).
    let storage_key = StorageKey(PatriciaKey::try_from(Felt::ZERO).expect("Invalid storage key"));

    initial_reads.storage.insert((eth_token_address, storage_key), Felt::ZERO);

    // Prepare query.
    let query = RpcProofProvider::prepare_query(initial_reads);

    // Verify query params.
    assert_eq!(query.contract_addresses.len(), 1, "Should have 1 contract address");
    assert_eq!(query.contract_storage_keys.len(), 1, "Should have 1 storage key entry");
    assert_eq!(query.contract_storage_keys[0].storage_keys.len(), 1, "Should have 1 storage key");

    // Use the test block number.
    let block_number = BlockNumber(TEST_BLOCK_NUMBER);

    // Fetch storage proof.
    let result = provider.fetch_storage_proof(block_number, &query).await;

    match result {
        Ok(proof) => {
            println!("Successfully fetched storage proof!");
            println!("Contracts tree root: {:?}", proof.global_roots.contracts_tree_root);
            println!("Classes tree root: {:?}", proof.global_roots.classes_tree_root);
            println!("Block hash: {:?}", proof.global_roots.block_hash);
            println!("Number of contract storage proofs: {}", proof.contracts_storage_proofs.len());
            println!(
                "Number of contract leaf data entries: {}",
                proof.contracts_proof.contract_leaves_data.len()
            );

            // Verify we got proof data.
            assert_eq!(proof.contracts_storage_proofs.len(), 1, "Should have 1 storage proof");
            assert_eq!(
                proof.contracts_proof.contract_leaves_data.len(),
                1,
                "Should have 1 contract leaf"
            );
        }
        Err(e) => {
            panic!("Failed to fetch storage proof: {:?}", e);
        }
    }
}
