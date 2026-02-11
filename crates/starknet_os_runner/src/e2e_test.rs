use apollo_transaction_converter::proof_verification::verify_proof;
use apollo_transaction_converter::transaction_converter::BOOTLOADER_PROGRAM_HASH;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::test_utils::invoke::rpc_invoke_tx;
use starknet_api::{calldata, felt, invoke_tx_args};

use crate::config::ProverConfig;
use crate::test_utils::{get_rpc_url, SENDER_ADDRESS, STRK_TOKEN_ADDRESS, TEST_BLOCK_NUMBER};
use crate::virtual_snos_prover::VirtualSnosProver;

/// Constructs a balanceOf RPC invoke transaction for testing.
///
/// This creates a transaction that calls `balanceOf` on the STRK token contract,
/// which is a simple read-only operation suitable for proving tests.
fn construct_balance_of_rpc_invoke() -> starknet_api::rpc_transaction::RpcTransaction {
    let strk_token = ContractAddress::try_from(STRK_TOKEN_ADDRESS).unwrap();
    let sender = ContractAddress::try_from(SENDER_ADDRESS).unwrap();
    let balance_of_selector = selector_from_name("balanceOf");

    // Calldata for account's __execute__ (Cairo 0 OZ account format):
    // [call_array_len, call_array..., calldata_len, calldata...]
    let calldata = calldata![
        felt!("1"),            // call_array_len
        *strk_token.0.key(),   // call_array[0].to
        balance_of_selector.0, // call_array[0].selector
        felt!("0"),            // call_array[0].data_offset
        felt!("1"),            // call_array[0].data_len
        felt!("1"),            // calldata_len
        *sender.0.key()        // calldata[0] - address to check balance of
    ];

    // Use a high nonce to satisfy the non-strict nonce check (nonce >= account_nonce).
    rpc_invoke_tx(invoke_tx_args! {
        sender_address: sender,
        calldata,
        nonce: Nonce(felt!("0x1000000")),
    })
}

/// End-to-end test: prove a transaction and verify the proof.
///
/// This test exercises the full proving pipeline:
/// 1. Constructs a balanceOf invoke transaction.
/// 2. Proves it using VirtualSnosProver (OS execution + stwo proving).
/// 3. Verifies the proof and proof facts using verify_proof (stwo verification,
///    proof facts consistency, proof version, and bootloader program hash).
///
/// # Requirements
///
/// - `NODE_URL` env var pointing to a Starknet mainnet RPC node.
/// - `stwo_run_and_prove` binary available in PATH.
///
/// # Running
///
/// ```bash
/// NODE_URL=https://your-rpc-node cargo test -p starknet_os_runner -- --ignored test_e2e
/// ```
#[tokio::test]
#[ignore]
async fn test_e2e_prove_and_verify_transaction() {
    let config = ProverConfig {
        rpc_node_url: get_rpc_url(),
        ..ProverConfig::default()
    };

    let prover = VirtualSnosProver::new(&config);
    let rpc_tx = construct_balance_of_rpc_invoke();
    let block_id = BlockId::Number(BlockNumber(TEST_BLOCK_NUMBER));

    // Prove the transaction.
    let output = prover
        .prove_transaction(block_id, rpc_tx)
        .await
        .expect("prove_transaction should succeed");
    let result = output.result;

    // Verify the proof and proof facts using the same verification logic as the gateway.
    verify_proof(result.proof_facts, result.proof, BOOTLOADER_PROGRAM_HASH)
        .expect("verify_proof should succeed");
}
