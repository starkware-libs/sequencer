use std::env;

use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use blockifier_reexecution::state_reader::rpc_state_reader::RpcStateReader;
use rstest::fixture;
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use starknet_rust::providers::Provider;
use starknet_types_core::felt::Felt;
use url::Url;

use crate::storage_proofs::RpcStorageProofsProvider;
use crate::virtual_block_executor::RpcVirtualBlockExecutor;

/// STRK token contract address on mainnet.
pub const STRK_TOKEN_ADDRESS: Felt =
    Felt::from_hex_unchecked("0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d");

/// A known account address on mainnet (Starknet Foundation).
pub const SENDER_ADDRESS: Felt =
    Felt::from_hex_unchecked("0x01176a1bd84444c89232ec27754698e5d2e7e1a7f1539f12027f28b23ec9f3d8");

/// Gets the RPC URL from the environment (NODE_URL).
pub fn get_rpc_url() -> String {
    env::var("NODE_URL").expect("NODE_URL environment variable required for this test")
}

/// Fixture that fetches the latest block number from RPC.
/// Note: This fixture creates its own tokio runtime, so it should NOT be used
/// in tests that use #[tokio::test]. For async tests, fetch the block number
/// directly using rpc_provider.0.block_number().await.
#[fixture]
pub fn latest_block_number(rpc_provider: RpcStorageProofsProvider) -> BlockNumber {
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let block_number = runtime
        .block_on(async { rpc_provider.0.block_number().await })
        .expect("Failed to fetch latest block number");
    BlockNumber(block_number)
}

/// Fixture that creates an RpcStateReader for testing.
/// Uses the latest block number fetched via rpc_provider.
#[fixture]
pub fn rpc_state_reader(latest_block_number: BlockNumber) -> RpcStateReader {
    let node_url = get_rpc_url();
    RpcStateReader::new_with_config_from_url(
        node_url,
        ChainId::Mainnet,
        BlockId::Number(latest_block_number),
    )
}

/// Fixture that creates an RpcStateReader for a specific block.
/// Use this in async tests where you've already fetched the block number.
pub fn rpc_state_reader_for_block(block_number: BlockNumber) -> RpcStateReader {
    let node_url = get_rpc_url();
    RpcStateReader::new_with_config_from_url(
        node_url,
        ChainId::Mainnet,
        BlockId::Number(block_number),
    )
}

#[fixture]
pub fn rpc_virtual_block_executor(rpc_state_reader: RpcStateReader) -> RpcVirtualBlockExecutor {
    RpcVirtualBlockExecutor {
        rpc_state_reader,
        // Skip transaction validation for testing.
        validate_txs: false,
    }
}

/// Fixture that creates an RpcStorageProofsProvider for testing.
#[fixture]
pub fn rpc_provider() -> RpcStorageProofsProvider {
    let rpc_url_str = get_rpc_url();
    let rpc_url = Url::parse(&rpc_url_str).expect("Invalid RPC URL");
    RpcStorageProofsProvider::new(rpc_url)
}
