use std::env;

use blockifier_reexecution::state_reader::rpc_state_reader::RpcStateReader;
use rstest::fixture;
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use starknet_types_core::felt::Felt;
use url::Url;

use crate::storage_proofs::RpcStorageProofsProvider;

/// Block number to use for testing (mainnet block with known state).
pub const TEST_BLOCK_NUMBER: u64 = 800000;

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

/// Fixture that creates an RpcStateReader for testing.
#[fixture]
pub fn rpc_state_reader() -> RpcStateReader {
    let node_url = get_rpc_url();
    RpcStateReader::new_with_config_from_url(
        node_url,
        ChainId::Mainnet,
        BlockNumber(TEST_BLOCK_NUMBER),
    )
}

/// Fixture that creates an RpcStorageProofsProvider for testing.
#[fixture]
pub fn rpc_provider() -> RpcStorageProofsProvider {
    let rpc_url_str = get_rpc_url();
    let rpc_url = Url::parse(&rpc_url_str).expect("Invalid RPC URL");
    RpcStorageProofsProvider::new(rpc_url)
}
