//! Tests that detect on-chain changes to constants defined in test_utils.

use rstest::rstest;
use starknet_rust::providers::jsonrpc::HttpTransport;
use starknet_rust::providers::{JsonRpcClient, Provider};
use starknet_rust_core::types::BlockId;
use starknet_types_core::felt::Felt;
use url::Url;

use crate::test_utils::{
    get_sepolia_rpc_url,
    PRIVACY_POOL_CONTRACT_ADDRESS,
    PRIVACY_POOL_CONTRACT_NONCE,
};

/// Queries the Sepolia RPC for the current nonce of the privacy pool contract
/// and compares it to the expected value using `expect_test`.
///
/// If the nonce has changed on-chain, run with `UPDATE_EXPECT=1` to auto-update:
///
/// ```bash
/// UPDATE_EXPECT=1 SEPOLIA_NODE_URL=https://your-rpc-node cargo test -p starknet_os_runner test_privacy_pool_contract_nonce_unchanged -- --ignored
/// ```
///
/// Then update `PRIVACY_POOL_CONTRACT_NONCE` in `test_utils.rs` to match.
#[rstest]
#[tokio::test]
#[ignore] // Requires RPC access.
async fn test_privacy_pool_contract_nonce_unchanged() {
    // Fetch the latest block number from Sepolia.
    let rpc_url = Url::parse(&get_sepolia_rpc_url()).expect("Invalid Sepolia RPC URL");
    let transport = HttpTransport::new(rpc_url);
    let provider = JsonRpcClient::new(transport);
    let block_number = provider.block_number().await.expect("Failed to fetch block number");
    let block_id = BlockId::Number(block_number);
    // Fetch the nonce from the RPC.
    let actual_nonce = provider
        .get_nonce(block_id, PRIVACY_POOL_CONTRACT_ADDRESS)
        .await
        .expect("Failed to fetch nonce from RPC");
    let expected_nonce = Felt::from_hex_unchecked(PRIVACY_POOL_CONTRACT_NONCE.data());

    assert_eq!(expected_nonce, actual_nonce);
}
