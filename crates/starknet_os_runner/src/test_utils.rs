use std::env;

use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use blockifier_reexecution::state_reader::rpc_state_reader::RpcStateReader;
use expect_test::{expect, Expect};
use rstest::fixture;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::core::ChainId;
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::{AllResourceBounds, ResourceBounds, ValidResourceBounds};
use starknet_rust::providers::Provider;
use starknet_types_core::felt::Felt;
use url::Url;

use crate::runner::{RpcRunnerFactory, RunnerConfig};
use crate::storage_proofs::{RpcStorageProofsProvider, StorageProofConfig};
use crate::virtual_block_executor::RpcVirtualBlockExecutor;

// ================================================================================================
// Constants
// ================================================================================================

// --- Mainnet ---

/// Block number to use for testing (mainnet block with known state).
pub const TEST_BLOCK_NUMBER: u64 = 800000;

/// STRK token contract address on mainnet.
pub const STRK_TOKEN_ADDRESS: Felt =
    Felt::from_hex_unchecked("0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d");

/// A known account address on mainnet (Starknet Foundation).
pub const SENDER_ADDRESS: Felt =
    Felt::from_hex_unchecked("0x01176a1bd84444c89232ec27754698e5d2e7e1a7f1539f12027f28b23ec9f3d8");

// --- Sepolia ---

/// STRK token contract address on Sepolia.
pub const STRK_TOKEN_ADDRESS_SEPOLIA: Felt =
    Felt::from_hex_unchecked("0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d");

/// Dummy account on Sepolia (no signature validation required).
/// This account uses the `account_with_dummy_validate` contract which always returns VALIDATED.
pub const DUMMY_ACCOUNT_ADDRESS: Felt =
    Felt::from_hex_unchecked("0x0786ed7d8dcbf1489241d65a4dd55f18b984c078558ce12def69802526fa918e");

/// Privacy pool contract address on Sepolia.
pub const PRIVACY_POOL_CONTRACT_ADDRESS: Felt =
    Felt::from_hex_unchecked("0x712391ff6487c9232582442ea7eb4a10cad4892c3bcde3516e2a3955bf4f0da");

/// Expected nonce of the privacy pool contract on Sepolia.
/// If this changes, `test_privacy_pool_contract_nonce_unchanged` will fail with
/// the new value so you can update this constant.
pub(crate) static PRIVACY_POOL_CONTRACT_NONCE: Expect = expect!["0x7"];

// ================================================================================================
// RPC URL Helpers
// ================================================================================================

/// Gets the mainnet RPC URL from the environment (NODE_URL).
pub fn get_rpc_url() -> String {
    env::var("NODE_URL").expect("NODE_URL environment variable required for this test")
}

/// Gets the Sepolia RPC URL (defaults to local node, can be overridden via SEPOLIA_NODE_URL).
pub fn get_sepolia_rpc_url() -> String {
    env::var("SEPOLIA_NODE_URL").unwrap_or_else(|_| "http://localhost:9546/rpc/v0_10".to_string())
}

// ================================================================================================
// Mainnet Fixtures
// ================================================================================================

/// Fixture that creates an RpcStateReader for mainnet testing.
#[fixture]
pub fn rpc_state_reader() -> RpcStateReader {
    let node_url = get_rpc_url();
    RpcStateReader::new_with_config_from_url(
        node_url,
        ChainId::Mainnet,
        BlockId::Number(BlockNumber(TEST_BLOCK_NUMBER)),
    )
}

/// Fixture that creates an RpcVirtualBlockExecutor for mainnet testing.
#[fixture]
pub fn rpc_virtual_block_executor(rpc_state_reader: RpcStateReader) -> RpcVirtualBlockExecutor {
    RpcVirtualBlockExecutor {
        rpc_state_reader,
        // Skip transaction validation for testing.
        validate_txs: false,
    }
}

/// Fixture that creates an RpcStorageProofsProvider for mainnet testing.
#[fixture]
pub fn rpc_provider() -> RpcStorageProofsProvider {
    let rpc_url = Url::parse(&get_rpc_url()).expect("Invalid RPC URL");
    RpcStorageProofsProvider::new(rpc_url)
}

// ================================================================================================
// Sepolia Fixtures
// ================================================================================================

/// Fixture that creates an RpcRunnerFactory for Sepolia with committer enabled.
///
/// This factory is configured to run the committer, meaning it will:
/// - Build a FactsDb from RPC proofs and execution data.
/// - Execute the committer to compute new state roots.
/// - Generate commitment infos with actual root changes.
#[fixture]
pub fn sepolia_runner_factory() -> RpcRunnerFactory {
    let rpc_url = Url::parse(&get_sepolia_rpc_url()).expect("Invalid Sepolia RPC URL");
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());

    let runner_config =
        RunnerConfig { storage_proof_config: StorageProofConfig { include_state_changes: true } };

    RpcRunnerFactory::new(rpc_url, ChainId::Sepolia, contract_class_manager, runner_config)
}

/// Fetches the latest block number from Sepolia (async).
pub async fn fetch_sepolia_block_number() -> BlockId {
    let rpc_url = Url::parse(&get_sepolia_rpc_url()).expect("Invalid Sepolia RPC URL");
    let provider = RpcStorageProofsProvider::new(rpc_url);
    let block_number = provider.0.block_number().await.expect("Failed to fetch block number");
    BlockId::Number(BlockNumber(block_number))
}

// ================================================================================================
// Transaction Helpers
// ================================================================================================

pub(crate) fn default_resource_bounds_for_client_side_tx() -> ValidResourceBounds {
    ValidResourceBounds::AllResources(AllResourceBounds {
        l1_gas: ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: GasPrice(0) },
        l2_gas: ResourceBounds {
            max_amount: GasAmount(10_000_000),
            max_price_per_unit: GasPrice(0),
        },
        l1_data_gas: ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: GasPrice(0) },
    })
}
