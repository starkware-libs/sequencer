use std::sync::Arc;

use blockifier::blockifier::config::ContractClassManagerConfig;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier::state::state_api::StateReader;
use blockifier::state::state_reader_and_contract_manager::StateReaderAndContractManager;
use blockifier_reexecution::state_reader::rpc_state_reader::RpcStateReader;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ChainId, ContractAddress};
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::transaction::{InvokeTransaction, Transaction, TransactionHash};
use starknet_api::{calldata, felt, invoke_tx_args};
use starknet_rust::providers::Provider;
use starknet_types_core::felt::Felt;
use url::Url;

use crate::runner::Runner;
use crate::storage_proofs::RpcStorageProofsProvider;
use crate::test_utils::{get_rpc_url, STRK_TOKEN_ADDRESS};
use crate::virtual_block_executor::RpcVirtualBlockExecutor;

/// Binance address on mainnet (Cairo 1 account).
pub const BINANCE_ADDRESS: Felt =
    Felt::from_hex_unchecked("0x0213c67ed78bc280887234fe5ed5e77272465317978ae86c25a71531d9332a2d");

/// Constructs a balance_of invoke transaction for a Cairo 1 account (Binance).
/// Fetches the real nonce from the RPC state reader.
fn construct_balance_of_invoke_cairo1(
    rpc_state_reader: &RpcStateReader,
) -> (InvokeTransaction, TransactionHash) {
    let strk_token = ContractAddress::try_from(STRK_TOKEN_ADDRESS).unwrap();
    let binance = ContractAddress::try_from(BINANCE_ADDRESS).unwrap();

    // Fetch the actual nonce for the Binance address from RPC.
    let nonce = rpc_state_reader.get_nonce_at(binance).expect("Failed to fetch nonce from RPC");

    // Calldata for Cairo 1 account's __execute__:
    // The format is: [calls_len, call.to, call.selector, call.calldata_len, ...call.calldata, ...]
    // For a single call to balanceOf(address):
    let balance_of_selector = selector_from_name("balanceOf");
    let calldata = calldata![
        felt!("1"),            // calls_len - number of calls
        *strk_token.0.key(),   // call.to - contract to call
        balance_of_selector.0, // call.selector - function selector
        felt!("1"),            // call.calldata_len - length of inner calldata
        *binance.0.key()       // call.calldata[0] - address to check balance of
    ];

    // Use the actual nonce fetched from RPC.
    let invoke_tx = invoke_tx(invoke_tx_args! {
        sender_address: binance,
        calldata,
        nonce,
    });

    let tx_hash = Transaction::Invoke(invoke_tx.clone())
        .calculate_transaction_hash(&ChainId::Mainnet)
        .unwrap();

    (invoke_tx, tx_hash)
}

/// Integration test for the full Runner flow with a balance_of transaction.
///
/// This test:
/// 1. Constructs a balanceOf call on the STRK token contract using real nonce
/// 2. Creates a Runner with RPC-based providers
/// 3. Runs the OS with the transaction
///
/// # Environment Variables
///
/// - `NODE_URL`: Required. URL of a Starknet mainnet RPC node.
///
/// # Running
///
/// ```bash
/// NODE_URL=https://your-rpc-node cargo test -p starknet_os_runner test_run_os_with_balance_of_transaction -- --ignored
/// ```
#[test]
#[ignore] // Requires RPC access
fn test_run_os_with_balance_of_transaction() {
    // Get RPC URL and create providers.
    let rpc_url_str = get_rpc_url();
    let rpc_url = Url::parse(&rpc_url_str).expect("Invalid RPC URL");
    let rpc_provider = RpcStorageProofsProvider::new(rpc_url);

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

    // Create state reader for the test block.
    let rpc_state_reader = RpcStateReader::new_with_config_from_url(
        rpc_url_str.clone(),
        ChainId::Mainnet,
        block_number,
    );

    // Create contract class manager.
    let contract_class_manager = ContractClassManager::start(ContractClassManagerConfig::default());

    // Construct the transaction using real nonce from RPC.
    let (tx, tx_hash) = construct_balance_of_invoke_cairo1(&rpc_state_reader);

    // Create the virtual block executor for the test block.
    // Disable validation since we don't have a valid signature.
    let rpc_virtual_block_executor = RpcVirtualBlockExecutor {
        rpc_state_reader: RpcStateReader::new_with_config_from_url(
            rpc_url_str.clone(),
            ChainId::Mainnet,
            block_number,
        ),
        validate_txs: false,
    };

    // Create the classes provider using a state reader and contract manager.
    let classes_state_reader =
        RpcStateReader::new_with_config_from_url(rpc_url_str, ChainId::Mainnet, block_number);
    let classes_provider = Arc::new(StateReaderAndContractManager::new(
        classes_state_reader,
        contract_class_manager.clone(),
        None,
    ));

    // Create the runner with all providers.
    let runner = Runner::new(classes_provider, rpc_provider, rpc_virtual_block_executor);

    // Create a multi-thread runtime. This is required because Runner::run_os uses
    // tokio::task::block_in_place internally, which only works on multi-thread runtime.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime");

    let result =
        runtime.block_on(runner.run_os(block_number, contract_class_manager, vec![(tx, tx_hash)]));

    // Verify execution succeeded.
    assert!(result.is_ok(), "run_os should succeed, got error: {:?}", result.err());

    println!("OS execution completed successfully!");
}
