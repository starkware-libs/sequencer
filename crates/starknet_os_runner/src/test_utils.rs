use std::env;

use blockifier::state::state_api::StateReader;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use blockifier_reexecution::state_reader::rpc_state_reader::RpcStateReader;
use rstest::fixture;
use starknet_api::block::{BlockNumber, GasPriceVector};
use starknet_api::core::{ChainId, ContractAddress, Nonce};
use starknet_api::rpc_transaction::{RpcInvokeTransaction, RpcInvokeTransactionV3, RpcTransaction};
use starknet_api::transaction::{InvokeTransaction, Transaction};
use starknet_types_core::felt::Felt;
use url::Url;

use crate::storage_proofs::RpcStorageProofsProvider;
use crate::virtual_block_executor::RpcVirtualBlockExecutor;

/// Block number to use for testing (mainnet block with known state).
pub const TEST_BLOCK_NUMBER: u64 = 800000;

/// STRK token contract address on mainnet.
pub const STRK_TOKEN_ADDRESS: Felt =
    Felt::from_hex_unchecked("0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d");

/// A known Cairo 0 account address on mainnet (Starknet Foundation, OZ account).
pub const SENDER_ADDRESS: Felt =
    Felt::from_hex_unchecked("0x01176a1bd84444c89232ec27754698e5d2e7e1a7f1539f12027f28b23ec9f3d8");

/// A known Cairo 1 account address on mainnet (Argent account).
/// Used for E2E proving tests that require all classes to be Cairo 1.
/// This account must have sufficient STRK balance to cover gas resource bounds.
pub const CAIRO1_SENDER_ADDRESS: Felt =
    Felt::from_hex_unchecked("0x555f20aba7a900163739ca67dacc7b018487efaece41e53f9c749339272f26e");

/// Gets the RPC URL from the environment (NODE_URL).
pub fn get_rpc_url() -> String {
    env::var("NODE_URL").expect("NODE_URL environment variable required for this test")
}

/// Fetches the latest block number from the RPC node.
///
/// Used by E2E tests that need a recent block (e.g., for storage proofs).
pub fn get_latest_block_number() -> u64 {
    let reader = RpcStateReader::new_with_config_from_url(
        get_rpc_url(),
        ChainId::Mainnet,
        BlockId::Latest,
    );
    reader.get_block_info().expect("Failed to fetch latest block info").block_number.0
}

/// Fetches the nonce for a contract at the latest block.
pub fn get_nonce_at_latest(contract_address: ContractAddress) -> Nonce {
    let reader = RpcStateReader::new_with_config_from_url(
        get_rpc_url(),
        ChainId::Mainnet,
        BlockId::Latest,
    );
    reader.get_nonce_at(contract_address).expect("Failed to fetch nonce")
}

/// Fetches the STRK gas prices at the latest block.
pub fn get_gas_prices_at_latest() -> GasPriceVector {
    let reader = RpcStateReader::new_with_config_from_url(
        get_rpc_url(),
        ChainId::Mainnet,
        BlockId::Latest,
    );
    let block_info = reader.get_block_info().expect("Failed to fetch latest block info");
    block_info.gas_prices.strk_gas_prices
}

/// Fetches the STRK fee token balance for a contract at the latest block.
///
/// Returns the low 128-bit part of the balance as a u128 (sufficient for most accounts).
pub fn get_strk_balance_at_latest(contract_address: ContractAddress) -> u128 {
    let reader = RpcStateReader::new_with_config_from_url(
        get_rpc_url(),
        ChainId::Mainnet,
        BlockId::Latest,
    );
    let strk_token = ContractAddress::try_from(STRK_TOKEN_ADDRESS).unwrap();
    let (balance_low, _balance_high) =
        reader.get_fee_token_balance(contract_address, strk_token).expect("Failed to fetch balance");
    let bytes = balance_low.to_bytes_le();
    u128::from_le_bytes(bytes[..16].try_into().unwrap())
}

/// Fetches a simple real V3 invoke transaction from a block on mainnet.
///
/// Searches the block for a V3 invoke with:
/// - Generous L2 gas bounds (to survive minor gas fluctuations when replaying).
/// - Short calldata (a simple call touches fewer storage keys, staying within
///   the RPC storage proof key limit).
///
/// Returns the `RpcTransaction` and the block number it was found in.
pub fn fetch_real_v3_invoke(block_number: u64) -> (RpcTransaction, u64) {
    let reader = RpcStateReader::new_with_config_from_url(
        get_rpc_url(),
        ChainId::Mainnet,
        BlockId::Number(BlockNumber(block_number)),
    );
    let all_txs = reader.get_all_txs_in_block().expect("Failed to fetch block transactions");
    for (tx, _tx_hash) in all_txs {
        if let Transaction::Invoke(InvokeTransaction::V3(ref invoke_v3)) = tx {
            let l2_gas = invoke_v3.resource_bounds.get_l2_bounds().max_amount;
            let calldata_len = invoke_v3.calldata.0.len();
            // Sufficient gas (>= 5M) and simple tx (short calldata, fewer storage reads).
            if l2_gas.0 >= 5_000_000 && calldata_len <= 20 {
                let invoke_v3_clone = invoke_v3.clone();
                let rpc_v3: RpcInvokeTransactionV3 = invoke_v3_clone
                    .try_into()
                    .expect("Failed to convert InvokeTransactionV3 to RPC format");
                return (
                    RpcTransaction::Invoke(RpcInvokeTransaction::V3(rpc_v3)),
                    block_number,
                );
            }
        }
    }
    panic!("No suitable V3 invoke found in block {block_number}");
}

/// Fixture that creates an RpcStateReader for testing.
#[fixture]
pub fn rpc_state_reader() -> RpcStateReader {
    let node_url = get_rpc_url();
    RpcStateReader::new_with_config_from_url(
        node_url,
        ChainId::Mainnet,
        BlockId::Number(BlockNumber(TEST_BLOCK_NUMBER)),
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
