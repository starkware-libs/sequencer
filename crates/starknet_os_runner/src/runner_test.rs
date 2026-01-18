use std::path::PathBuf;

use rstest::rstest;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::GasPrice;
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::execution_resources::GasAmount;
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::test_utils::privacy_transaction::create_signed_invoke_v3;
use starknet_api::transaction::fields::{AllResourceBounds, ResourceBounds, ValidResourceBounds};
use starknet_api::transaction::{InvokeTransaction, Transaction, TransactionHash};
use starknet_api::{calldata, felt, invoke_tx_args};

use crate::runner::RpcRunnerFactory;
use crate::test_utils::{
    fetch_sepolia_block_number,
    sepolia_runner_factory,
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

    // Calldata matches dummy account's __execute__(contract_address, selector, calldata)
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
///
/// # Running
///
/// ```bash
/// SEPOLIA_NODE_URL=https://your-rpc-node cargo test -p starknet_os_runner test_run_os_with_balance_of_transaction -- --ignored
/// ```
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Requires RPC access
async fn test_run_os_with_balance_of_transaction(sepolia_runner_factory: RpcRunnerFactory) {
    let block_number = fetch_sepolia_block_number().await;
    let (tx, tx_hash) = strk_balance_of_invoke();
    let runner = sepolia_runner_factory.create_runner(block_number);

    // Verify execution succeeds.
    runner.run_os(vec![(tx, tx_hash)]).await.expect("run_os should succeed");
}

/// Integration test that runs a privacy pool transaction and saves the Cairo PIE.
///
/// Uses a pre-signed privacy transaction from `starknet_api::test_utils::privacy_transaction`.
///
/// # Running
///
/// ```bash
/// SEPOLIA_NODE_URL=https://your-rpc-node cargo test -p starknet_os_runner test_run_os_with_privacy_transaction -- --ignored
/// ```
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Requires RPC access
async fn test_run_os_with_privacy_transaction(sepolia_runner_factory: RpcRunnerFactory) {
    let block_number = fetch_sepolia_block_number().await;

    // Create privacy transaction from pre-signed constants.
    let invoke_v3 = create_signed_invoke_v3();
    let tx = InvokeTransaction::V3(invoke_v3);
    let tx_hash =
        Transaction::Invoke(tx.clone()).calculate_transaction_hash(&ChainId::Sepolia).unwrap();

    let runner = sepolia_runner_factory.create_runner(block_number);
    let output = runner.run_os(vec![(tx, tx_hash)]).await.expect("run_os should succeed");

    // Save Cairo PIE to resources folder.
    let pie_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/privacy_tx_cairo_pie.zip");
    output.cairo_pie.write_zip_file(&pie_path, true).expect("Failed to save Cairo PIE");
    println!("Cairo PIE saved to: {}", pie_path.display());
}
