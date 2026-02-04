//! Integration tests for the Runner.

use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use blockifier_test_utils::calldata::create_calldata;
use rstest::rstest;
use starknet_api::core::ContractAddress;
use starknet_api::invoke_tx_args;
use starknet_api::test_utils::invoke::invoke_tx;

use crate::runner::VirtualSnosRunner;
use crate::test_utils::{
    default_resource_bounds_for_client_side_tx,
    sepolia_runner_factory,
    DUMMY_ACCOUNT_ADDRESS,
    STRK_TOKEN_ADDRESS_SEPOLIA,
};

/// Integration test for the full Runner flow with a balance_of transaction.

/// # Running
///
/// ```bash
/// SEPOLIA_NODE_URL=https://your-rpc-node cargo test -p starknet_os_runner test_run_os_with_balance_of_transaction -- --ignored
/// ```
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Requires RPC access.
async fn test_run_os_with_balance_of_transaction() {
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

    // Create a custom factory with the specified run_committer setting.
    let factory = sepolia_runner_factory();
    let block_id = BlockId::Latest;

    // Verify execution succeeds.
    factory
        .run_virtual_os(block_id, vec![(invoke_tx)])
        .await
        .expect("run_virtual_os should succeed");
}
