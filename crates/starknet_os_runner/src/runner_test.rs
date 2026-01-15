use std::env;
use std::path::Path;
use std::sync::Arc;

use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::state_api::StateReader;
use blockifier_reexecution::state_reader::rpc_state_reader::RpcStateReader;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::{BlockNumber, GasPrice};
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::execution_resources::GasAmount;
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::transaction::fields::{AllResourceBounds, ResourceBounds, TransactionSignature};
use starknet_api::transaction::{
    InvokeTransaction,
    Transaction,
    TransactionHash,
    TransactionVersion,
};
use starknet_api::{calldata, felt, invoke_tx_args};
use starknet_core::crypto::ecdsa_sign;
use starknet_rust::providers::Provider;
use starknet_types_core::felt::Felt;
use url::Url;

use crate::runner::RpcRunnerFactory;
use crate::storage_proofs::RpcStorageProofsProvider;
use crate::test_utils::{get_rpc_url, STRK_TOKEN_ADDRESS};

/// Account address on mainnet (Cairo 1 account).
/// Contract: https://starkscan.co/contract/0x07f2f71bebfd9021684fcbcb954a37450febef5f3649ac6228e0c76c4f8819c4
/// Class: https://starkscan.co/class/0x05b4b537eaa2399e3aa99c4e2e0208ebd6c71bc1467938cd52c798c601e43564
pub const ACCOUNT_ADDRESS: Felt =
    Felt::from_hex_unchecked("0x07f2f71bebfd9021684fcbcb954a37450febef5f3649ac6228e0c76c4f8819c4");

/// Constructs a balance_of invoke transaction for a Cairo 1 account.
/// Fetches the real nonce from the RPC state reader and signs the transaction.
///
/// # Arguments
///
/// * `rpc_state_reader` - RPC state reader to fetch nonce
/// * `private_key` - Private key for signing the transaction (as hex string)
fn construct_balance_of_invoke_cairo1(
    rpc_state_reader: &RpcStateReader,
    private_key: &str,
) -> (InvokeTransaction, TransactionHash) {
    let strk_token = ContractAddress::try_from(STRK_TOKEN_ADDRESS).unwrap();
    let account = ContractAddress::try_from(ACCOUNT_ADDRESS).unwrap();

    // Fetch the actual nonce for the account address from RPC.
    let nonce = rpc_state_reader.get_nonce_at(account).expect("Failed to fetch nonce from RPC");

    // Calldata for Cairo 1 account's __execute__:
    // The format is: [calls_len, call.to, call.selector, call.calldata_len, ...call.calldata, ...]
    // For a single call to balanceOf(address):
    let balance_of_selector = selector_from_name("balanceOf");
    let calldata = calldata![
        felt!("1"),            // calls_len - number of calls
        *strk_token.0.key(),   // call.to - contract to call
        balance_of_selector.0, // call.selector - function selector
        felt!("1"),            // call.calldata_len - length of inner calldata
        *account.0.key()       // call.calldata[0] - address to check balance of
    ];

    // Set up resource bounds with realistic values for V3 transaction.
    // Using values that are within the account balance but high enough for execution.
    // Account balance is ~1440 ETH, so we set bounds that result in max fee < 1000 ETH.
    // Actual L1 gas price is ~38.1 trillion wei, so we set max_price_per_unit higher.
    let resource_bounds =
        starknet_api::transaction::fields::ValidResourceBounds::AllResources(AllResourceBounds {
            l1_gas: ResourceBounds { max_amount: GasAmount(0), max_price_per_unit: GasPrice(0) },
            l2_gas: ResourceBounds {
                max_amount: GasAmount(10_000_000),
                max_price_per_unit: GasPrice(0),
            },
            l1_data_gas: ResourceBounds {
                max_amount: GasAmount(0),
                max_price_per_unit: GasPrice(0),
            },
        });

    // Create the transaction with a placeholder signature first (V3 transaction).
    let invoke_tx_unsigned = invoke_tx(invoke_tx_args! {
        sender_address: account,
        calldata: calldata.clone(),
        nonce,
        version: TransactionVersion::THREE,
        resource_bounds: resource_bounds.clone(),
    });

    // Calculate the transaction hash (signature is not part of hash for V3).
    let tx_hash = Transaction::Invoke(invoke_tx_unsigned.clone())
        .calculate_transaction_hash(&ChainId::Mainnet)
        .unwrap();

    // Sign the transaction with the private key.
    let private_key_felt = Felt::from_hex(private_key)
        .expect("Failed to parse private key. Expected hex string (e.g., '0x123...')");
    let signature = ecdsa_sign(&private_key_felt, &tx_hash.0).expect("Failed to sign transaction");

    // Create the final V3 transaction with the signature and resource bounds.
    let invoke_tx = invoke_tx(invoke_tx_args! {
        sender_address: account,
        calldata,
        nonce,
        version: TransactionVersion::THREE,
        signature: TransactionSignature(Arc::new(vec![signature.r, signature.s])),
        resource_bounds,
    });

    (invoke_tx, tx_hash)
}

/// Integration test for the full Runner flow with a balance_of transaction.
///
/// This test:
/// 1. Constructs a balanceOf call on the STRK token contract using real nonce
/// 2. Signs the transaction with the provided private key
/// 3. Creates a Runner using RpcRunnerFactory
/// 4. Runs the OS with the transaction
/// 5. Saves the Cairo PIE to a file
///
/// # Environment Variables
///
/// - `NODE_URL`: Required. URL of a Starknet mainnet RPC node.
/// - `PRIVATE_KEY`: Required. Private key of the account (hex string, e.g., "0x123...").
///
/// # Running
///
/// ```bash
/// NODE_URL=https://your-rpc-node PRIVATE_KEY=0x... cargo test -p starknet_os_runner test_run_os_with_balance_of_transaction -- --ignored
/// ```
#[test]
#[ignore] // Requires RPC access
fn test_run_os_with_balance_of_transaction() {
    // Get private key from environment variable.
    let private_key = env::var("PRIVATE_KEY").expect(
        "PRIVATE_KEY environment variable is required. Provide the private key as a hex string \
         (e.g., '0x123...')",
    );

    // Get RPC URL and create providers.
    let rpc_url_str = get_rpc_url();
    let rpc_url = Url::parse(&rpc_url_str).expect("Invalid RPC URL");
    let rpc_provider = RpcStorageProofsProvider::new(rpc_url.clone());

    // Fetch the latest block number from RPC.
    // We need to do this before creating the main tokio runtime to avoid nested runtime issues.
    let latest_block = {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create temp runtime");
        rt.block_on(rpc_provider.0.block_number()).expect("Failed to fetch block number")
    };
    let block_number = BlockNumber(latest_block);

    // Create contract class manager.
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());

    // Create RpcRunnerFactory.
    let factory = RpcRunnerFactory::new(rpc_url, ChainId::Mainnet, contract_class_manager.clone());

    // Create state reader for fetching nonce.
    let rpc_state_reader = RpcStateReader::new_with_config_from_url(
        rpc_url_str.clone(),
        ChainId::Mainnet,
        block_number,
    );

    // Construct the transaction using real nonce from RPC and sign it.
    let (tx, tx_hash) = construct_balance_of_invoke_cairo1(&rpc_state_reader, &private_key);

    // Create runner from factory.
    let runner = factory.create_runner(block_number);

    // Create a multi-thread runtime. This is required because Runner::run_os uses
    // tokio::task::block_in_place internally, which only works on multi-thread runtime.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");

    // Run the OS with the transaction.
    let result = runtime.block_on(runner.run_os(vec![(tx, tx_hash)]));

    // Verify execution succeeded.
    let output = result.expect("run_os should succeed");

    // Save the Cairo PIE to a file.
    let cairo_pie_path = "cairo_pie.zip";
    let merge_extra_segments = true;
    output
        .cairo_pie
        .write_zip_file(Path::new(cairo_pie_path), merge_extra_segments)
        .expect("Failed to write Cairo PIE to file");

    println!("OS execution completed successfully!");
    println!("Cairo PIE saved to: {}", cairo_pie_path);
}
