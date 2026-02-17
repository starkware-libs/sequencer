//! Integration tests for the Runner.
//!
//! These tests run against Sepolia and support three modes (see [`crate::running::rpc_records`]
//! and [`crate::running::test_utils::resolve_test_mode`]):
//!
//! - **Live mode** (default): runs against a real node (requires `NODE_URL`).
//! - **Recording mode** (`RECORD_RPC_RECORDS=1`): runs against a real node through a recording
//!   proxy and saves all RPC interactions to a records file.
//! - **Offline mode** (records file present): replays pre-recorded interactions from a mock server.
//!
//! # Running
//!
//! ```bash
//! # Live mode:
//! NODE_URL=http://localhost:9545/rpc/v0_10 cargo test -p starknet_os_runner runner_test -- --ignored
//!
//! # Recording mode (saves records files under resources/fixtures/):
//! RECORD_RPC_RECORDS=1 NODE_URL=http://localhost:9545/rpc/v0_10 cargo test -p starknet_os_runner runner_test -- --ignored
//!
//! # Offline mode (uses saved records files):
//! cargo test -p starknet_os_runner runner_test -- --ignored
//! ```

use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use blockifier_test_utils::calldata::create_calldata;
use rstest::rstest;
use starknet_api::core::ContractAddress;
use starknet_api::test_utils::invoke::invoke_tx;
use starknet_api::{contract_address, felt, invoke_tx_args};

use crate::running::runner::VirtualSnosRunner;
use crate::running::test_utils::{
    default_resource_bounds_for_client_side_tx,
    resolve_test_mode,
    runner_factory,
    DUMMY_ACCOUNT_ADDRESS,
    STRK_TOKEN_ADDRESS_SEPOLIA,
};

/// Integration test for the full Runner flow with a balance_of transaction.
/// Runs on a Sepolia environment; in live/recording mode requires a Sepolia RPC node via
/// `NODE_URL`.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn test_run_os_with_balance_of_transaction() {
    let test_mode = resolve_test_mode("test_run_os_with_balance_of_transaction").await;

    // Creates an invoke transaction that calls `balanceOf` on the STRK token.
    let strk_token = ContractAddress::try_from(STRK_TOKEN_ADDRESS_SEPOLIA).unwrap();
    let account = ContractAddress::try_from(DUMMY_ACCOUNT_ADDRESS).unwrap();

    // Calldata matches dummy account's __execute__(contract_address, selector, calldata).
    let calldata = create_calldata(strk_token, "balanceOf", &[account.into()]);
    let resource_bounds = default_resource_bounds_for_client_side_tx();

    let invoke_tx = invoke_tx(invoke_tx_args! {
        sender_address: account,
        calldata,
        resource_bounds,
    });

    let factory = runner_factory(&test_mode.rpc_url());
    let block_id = BlockId::Latest;

    // Verify execution succeeds.
    factory
        .run_virtual_os(block_id, vec![(invoke_tx)])
        .await
        .expect("run_virtual_os should succeed");

    test_mode.finalize();
}

/// Integration test for the full Runner flow with a STRK transfer transaction.
/// Runs on a Sepolia environment; in live/recording mode requires a Sepolia RPC node via
/// `NODE_URL`.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Run with --ignored; supports live, recording, and offline modes.
async fn test_run_os_with_transfer_transaction() {
    let test_mode = resolve_test_mode("test_run_os_with_transfer_transaction").await;

    let strk_token = ContractAddress::try_from(STRK_TOKEN_ADDRESS_SEPOLIA).unwrap();
    let account = ContractAddress::try_from(DUMMY_ACCOUNT_ADDRESS).unwrap();
    let recipient = contract_address!("0x123");

    // Transfer amount: 1 wei (u256 = low + high * 2^128).
    let amount_low = felt!("1");
    let amount_high = felt!("0");

    // Calldata matches dummy account's __execute__(contract_address, selector, calldata).
    // transfer(recipient, amount) where amount is u256 (low, high).
    let calldata =
        create_calldata(strk_token, "transfer", &[recipient.into(), amount_low, amount_high]);

    let resource_bounds = default_resource_bounds_for_client_side_tx();

    let invoke_tx = invoke_tx(invoke_tx_args! {
        sender_address: account,
        calldata,
        resource_bounds,
    });

    let factory = runner_factory(&test_mode.rpc_url());
    let block_id = BlockId::Latest;

    // Verify execution succeeds.
    factory
        .run_virtual_os(block_id, vec![(invoke_tx)])
        .await
        .expect("run_virtual_os should succeed");

    test_mode.finalize();
}
