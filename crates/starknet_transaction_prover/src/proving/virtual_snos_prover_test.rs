//! Integration tests for the VirtualSnosProver (full prove_transaction flow).
//!
//! These tests exercise the complete prover pipeline: transaction extraction, OS execution,
//! and proof generation.  They run against Sepolia and support three modes
//! (see [`crate::running::rpc_records`] and [`crate::test_utils::resolve_test_mode`]):
//!
//! - **Live mode** (default): runs against a real node (requires `NODE_URL`).
//! - **Recording mode** (`RECORD_RPC_RECORDS=1`): runs against a real node through a recording
//!   proxy and saves all RPC interactions to a records file.
//! - **Offline mode** (records file present): replays pre-recorded interactions from a mock server.
//!
//! # Environment variables
//!
//! - `NODE_URL`: RPC endpoint URL (required for live/recording modes).
//! - `CHAIN_ID`: Override the chain ID (defaults to `Sepolia`).
//! - `STRK_FEE_TOKEN_ADDRESS`: Override the STRK fee token contract address.
//!
//! # Running
//!
//! ```bash
//! # Live mode:
//! NODE_URL=http://localhost:9545/rpc/v0_10 cargo test -p starknet_transaction_prover virtual_snos_prover_test -- --ignored
//!
//! # Recording mode (saves records files under resources/rpc_records/):
//! RECORD_RPC_RECORDS=1 NODE_URL=http://localhost:9545/rpc/v0_10 cargo test -p starknet_transaction_prover virtual_snos_prover_test -- --ignored
//!
//! # Offline mode (uses saved records files):
//! cargo test -p starknet_transaction_prover virtual_snos_prover_test -- --ignored
//! ```

use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use blockifier_test_utils::calldata::create_calldata;
use rstest::rstest;
use starknet_api::core::ContractAddress;
use starknet_api::{contract_address, felt};
use starknet_proof_verifier::verify_proof;

use crate::proving::virtual_snos_prover::VirtualSnosProver;
use crate::test_utils::{
    build_client_side_rpc_invoke,
    resolve_test_mode,
    runner_factory,
    DUMMY_ACCOUNT_ADDRESS,
    STRK_TOKEN_ADDRESS_SEPOLIA,
};

/// Integration test for the full prover pipeline with a `balanceOf` transaction.
/// Runs on a Sepolia environment; in live/recording mode requires a Sepolia RPC node via
/// `NODE_URL`.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn test_prove_balance_of_transaction() {
    let test_mode = resolve_test_mode("test_prove_balance_of_transaction").await;

    // Creates an RPC invoke transaction that calls `balanceOf` on the STRK token.
    let strk_token = ContractAddress::try_from(STRK_TOKEN_ADDRESS_SEPOLIA).unwrap();
    let account = ContractAddress::try_from(DUMMY_ACCOUNT_ADDRESS).unwrap();

    // Calldata matches dummy account's __execute__(contract_address, selector, calldata).
    let calldata = create_calldata(strk_token, "balanceOf", &[account.into()]);
    let rpc_tx = build_client_side_rpc_invoke(account, calldata);

    let factory = runner_factory(&test_mode.rpc_url());
    let prover = VirtualSnosProver::from_runner(factory);

    // Run the full prover pipeline: OS execution → proof generation.
    let result = prover.prove_transaction(BlockId::Latest, rpc_tx).await;

    // Finalize recording before asserting so records are saved even on failure.
    test_mode.finalize();

    // Verify execution and proving succeeded.
    let output = result.expect("prove_transaction should succeed");

    // Verify the proof against the proof facts.
    let proof_facts = output.proof_facts.clone();
    let proof = output.proof.clone();
    tokio::task::spawn_blocking(move || verify_proof(proof_facts, proof))
        .await
        .expect("proof verification task panicked")
        .expect("proof verification should succeed");
}

/// Integration test for the full prover pipeline with a STRK `transfer` transaction.
/// Runs on a Sepolia environment; in live/recording mode requires a Sepolia RPC node via
/// `NODE_URL`.
#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[ignore] // Run with --ignored; supports live, recording, and offline modes.
async fn test_prove_transfer_transaction() {
    let test_mode = resolve_test_mode("test_prove_transfer_transaction").await;

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
    let rpc_tx = build_client_side_rpc_invoke(account, calldata);

    let factory = runner_factory(&test_mode.rpc_url());
    let prover = VirtualSnosProver::from_runner(factory);

    // Run the full prover pipeline: OS execution → proof generation.
    let result = prover.prove_transaction(BlockId::Latest, rpc_tx).await;

    // Finalize recording before asserting so records are saved even on failure.
    test_mode.finalize();

    // Verify execution and proving succeeded.
    let output = result.expect("prove_transaction should succeed");

    // Verify the proof against the proof facts.
    let proof_facts = output.proof_facts.clone();
    let proof = output.proof.clone();
    tokio::task::spawn_blocking(move || verify_proof(proof_facts, proof))
        .await
        .expect("proof verification task panicked")
        .expect("proof verification should succeed");
}
