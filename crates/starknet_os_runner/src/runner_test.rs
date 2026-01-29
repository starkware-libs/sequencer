use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::state::contract_class_manager::ContractClassManager;
use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::GasPrice;
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::execution_resources::GasAmount;
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::transaction::fields::{AllResourceBounds, ResourceBounds, ValidResourceBounds};
use starknet_api::transaction::{InvokeTransaction, Transaction, TransactionHash};
use starknet_api::{calldata, felt, invoke_tx_args};
use url::Url;

use crate::runner::{RpcRunnerFactory, RunnerConfig};
use crate::storage_proofs::StorageProofConfig;
use crate::test_utils::{
    fetch_sepolia_block_number,
    get_sepolia_rpc_url,
    DUMMY_ACCOUNT_ADDRESS,
    STRK_TOKEN_ADDRESS_SEPOLIA,
};

/// Creates an invoke transaction that calls `balanceOf` on the STRK token.
///
/// Uses the dummy account which requires no signature validation.
/// The dummy account's `__execute__` format is: (contract_address, selector, calldata).
fn strk_balance_of_invoke() -> (InvokeTransaction, TransactionHash) {
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

/// Integration test for the full Runner flow with a balance_of transaction.
///
/// Uses a dummy account on Sepolia that requires no signature validation.
/// This test verifies that the runner can successfully execute transactions and run the OS
/// with the committer enabled (computing actual state root changes).
///
/// # Running
///
/// ```bash
/// SEPOLIA_NODE_URL=https://your-rpc-node cargo test -p starknet_os_runner test_run_os_with_balance_of_transaction -- --ignored
/// ```
#[rstest]
#[case(true)] // With committer
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Requires RPC access.
async fn test_run_os_with_balance_of_transaction(#[case] run_committer: bool) {
    // Create a custom factory with the specified run_committer setting.
    let rpc_url = get_sepolia_rpc_url();
    let rpc_url_parsed = Url::parse(&rpc_url).expect("Invalid Sepolia RPC URL");
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());
    let config = RunnerConfig { storage_proof_config: StorageProofConfig { run_committer } };
    let factory =
        RpcRunnerFactory::new(rpc_url_parsed, ChainId::Sepolia, contract_class_manager, config);

    let block_number = fetch_sepolia_block_number().await;
    let (tx, tx_hash) = strk_balance_of_invoke();
    let runner = factory.create_runner(block_number);

    // Verify execution succeeds.
    runner.run_virtual_os(vec![(tx, tx_hash)]).await.expect("run_virtual_os should succeed");
}
