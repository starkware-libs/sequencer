use std::env;

use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use blockifier_reexecution::state_reader::rpc_state_reader::RpcStateReader;
use rstest::fixture;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::execution_resources::GasAmount;
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::transaction::fields::{AllResourceBounds, ResourceBounds, ValidResourceBounds};
use starknet_api::transaction::{InvokeTransaction, Transaction, TransactionHash};
use starknet_api::{calldata, felt, invoke_tx_args};
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

/// Gets the Privacy node RPC URL (defaults to local node, can be overridden via PRIVACY_NODE_URL).
pub fn get_privacy_rpc_url() -> String {
    env::var("PRIVACY_NODE_URL").unwrap_or_else(|_| "http://localhost:9547/rpc/v0_10".to_string())
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

/// Creates the privacy test invoke transaction.
///
/// This transaction is for testing on the privacy-starknet-pathfinder node.
pub fn privacy_invoke_tx() -> (InvokeTransaction, TransactionHash) {
    use starknet_api::data_availability::DataAvailabilityMode;
    use starknet_api::transaction::fields::{ProofFacts, Tip, TransactionSignature};
    use starknet_api::transaction::InvokeTransactionV3;

    let sender_address = ContractAddress::try_from(Felt::from_hex_unchecked(
        "0x7bfcd6bd5b220a1d46921d92924ddec46bb7e49d05354c76a8714b41269b2f8",
    ))
    .unwrap();

    let signature = TransactionSignature(std::sync::Arc::new(vec![
        Felt::from_hex_unchecked(
            "0x2696e74dfd65f95a434f9bb5b19bc21add3161bbe272bbb37e6d114426eef76",
        ),
        Felt::from_hex_unchecked(
            "0x1cdaa27fd6b624cb78aca8169cdb49c2ffb470fc4e797b7f462a09ff81577c7",
        ),
    ]));

    let resource_bounds = ValidResourceBounds::AllResources(AllResourceBounds {
        l1_gas: ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: GasPrice(0) },
        l2_gas: ResourceBounds {
            max_amount: GasAmount(10_000_000),
            max_price_per_unit: GasPrice(0),
        },
        l1_data_gas: ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: GasPrice(0) },
    });

    let calldata = starknet_api::transaction::fields::Calldata(std::sync::Arc::new(vec![
        Felt::from_hex_unchecked(
            "0x7bfcd6bd5b220a1d46921d92924ddec46bb7e49d05354c76a8714b41269b2f8",
        ),
        Felt::from_hex_unchecked(
            "0x9874a02fe5bbda5d097a608675f2a5a71e2ea38b4438c51e90d8084a1e88e1",
        ),
        Felt::from_hex_unchecked("0x1"),
        Felt::from_hex_unchecked("0x0"),
        Felt::from_hex_unchecked("0x123456789"),
    ]));

    let invoke_v3 = InvokeTransactionV3 {
        resource_bounds,
        tip: Tip(0),
        signature,
        nonce: starknet_api::core::Nonce(Felt::ONE),
        sender_address,
        calldata,
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
        paymaster_data: starknet_api::transaction::fields::PaymasterData(vec![]),
        account_deployment_data: starknet_api::transaction::fields::AccountDeploymentData(vec![]),
        proof_facts: ProofFacts(std::sync::Arc::new(vec![])),
    };

    let invoke_tx = InvokeTransaction::V3(invoke_v3);
    // Use Sepolia chain ID for hash calculation since the privacy network
    // infrastructure expects Sepolia-compatible configuration.
    let tx_hash = Transaction::Invoke(invoke_tx.clone())
        .calculate_transaction_hash(&ChainId::Sepolia)
        .unwrap();

    (invoke_tx, tx_hash)
}

/// Fetches the latest block number from the privacy node (async).
pub async fn fetch_privacy_block_number() -> BlockId {
    let rpc_url = Url::parse(&get_privacy_rpc_url()).expect("Invalid Privacy RPC URL");
    let provider = RpcStorageProofsProvider::new(rpc_url);
    let block_number = provider.0.block_number().await.expect("Failed to fetch block number");
    BlockId::Number(BlockNumber(block_number))
}

/// Creates an invoke transaction that calls `balanceOf` on the STRK token.
///
/// Uses the dummy account which requires no signature validation.
/// The dummy account's `__execute__` format is: (contract_address, selector, calldata).
pub fn strk_balance_of_invoke() -> (InvokeTransaction, TransactionHash) {
    let strk_token = ContractAddress::try_from(STRK_TOKEN_ADDRESS_SEPOLIA).unwrap();
    let account = ContractAddress::try_from(DUMMY_ACCOUNT_ADDRESS).unwrap();

    // Calldata matches dummy account's __execute__(contract_address, selector, calldata).
    let calldata = calldata![
        *strk_token.0.key(),
        selector_from_name("balanceOf").0,
        felt!("1"),
        *account.0.key()
    ];

    let resource_bounds = ValidResourceBounds::AllResources(AllResourceBounds {
        l1_gas: ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: GasPrice(0) },
        l2_gas: ResourceBounds {
            max_amount: GasAmount(10_000_000),
            max_price_per_unit: GasPrice(0),
        },
        l1_data_gas: ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: GasPrice(0) },
    });

    let invoke_tx = invoke_tx(invoke_tx_args! {
        sender_address: account,
        calldata,
        resource_bounds,
    });

    let tx_hash = Transaction::Invoke(invoke_tx.clone())
        .calculate_transaction_hash(&ChainId::Sepolia)
        .unwrap();

    (invoke_tx, tx_hash)
}
