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

/// Generates proof fixture files for the `integration_test_proof_flow` integration test.
///
/// Runs a `balanceOf` transaction against a local Starknet node, generates a proof, and writes
/// `proof.bin` and `proof_facts.json` to the `apollo_integration_tests/resources/proof_flow/`
/// directory.
///
/// # Environment variables
///
/// - `LOCAL_NODE_URL`: RPC endpoint URL (default: `http://localhost:6060`).
/// - `CHAIN_ID`: Override the chain ID (defaults to `Sepolia`).
/// - `STRK_FEE_TOKEN_ADDRESS`: Override the STRK fee token contract address.
///
/// # Running
///
/// ```bash
/// LOCAL_NODE_URL=http://localhost:6060 \
/// cargo test --features stwo_proving -p starknet_transaction_prover \
///     generate_proof_flow_fixtures -- --ignored --nocapture
/// ```
#[cfg(feature = "stwo_proving")]
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn generate_proof_flow_fixtures() {
    let node_url =
        std::env::var("LOCAL_NODE_URL").unwrap_or_else(|_| "http://localhost:6060".to_string());

    let strk_token = ContractAddress::try_from(STRK_TOKEN_ADDRESS_SEPOLIA).unwrap();
    let account = ContractAddress::try_from(DUMMY_ACCOUNT_ADDRESS).unwrap();

    let calldata = create_calldata(strk_token, "balanceOf", &[account.into()]);
    let rpc_tx = build_client_side_rpc_invoke(account, calldata);

    let factory = runner_factory(&node_url);
    let prover = VirtualSnosProver::from_runner(factory);

    let output = prover
        .prove_transaction(BlockId::Latest, rpc_tx)
        .await
        .expect("prove_transaction should succeed");

    let resources_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("apollo_integration_tests/resources/proof_flow");

    let proof_facts_path = resources_dir.join("proof_facts.json");
    let proof_path = resources_dir.join("proof.bin");

    let proof_facts_json =
        serde_json::to_string_pretty(&output.proof_facts).expect("Failed to serialize proof_facts");
    std::fs::write(&proof_facts_path, proof_facts_json).expect("Failed to write proof_facts.json");

    let proof_bytes: Vec<u8> = output.proof.0.iter().flat_map(|word| word.to_be_bytes()).collect();
    std::fs::write(&proof_path, proof_bytes).expect("Failed to write proof.bin");

    println!("Wrote proof_facts.json to {}", proof_facts_path.display());
    println!("Wrote proof.bin to {}", proof_path.display());
}

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

/// Generates proof fixture files for `integration_test_proof_flow` using an RPC node.
///
/// **Alternative (preferred)**: Use the in-memory approach in `starknet_os_flow_tests`:
/// ```bash
/// cargo +nightly-2025-07-14 test -p starknet_os_flow_tests --features stwo_proving \
///     generate_proof_flow_fixtures -- --ignored --nocapture
/// ```
///
/// This RPC-based test requires a local Starknet node; use the above instead when possible.
/// Connects to `LOCAL_NODE_URL` (default: `http://localhost:6060`), runs a `balanceOf`
/// transaction, generates a proof, and writes `proof.bin` and `proof_facts.json` to
/// `apollo_integration_tests/resources/proof_flow/`.
///
/// # Running
///
/// ```bash
/// LOCAL_NODE_URL=http://localhost:6060 \
/// cargo test --features stwo_proving -p starknet_transaction_prover \
///     generate_proof_flow_fixtures -- --ignored --nocapture
/// ```
#[cfg(feature = "stwo_proving")]
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn generate_proof_flow_fixtures() {
    use std::path::PathBuf;

    let rpc_url =
        std::env::var("LOCAL_NODE_URL").unwrap_or_else(|_| "http://localhost:6060".to_string());

    let strk_token = ContractAddress::try_from(STRK_TOKEN_ADDRESS_SEPOLIA).unwrap();
    let account = ContractAddress::try_from(DUMMY_ACCOUNT_ADDRESS).unwrap();
    let calldata = create_calldata(strk_token, "balanceOf", &[account.into()]);
    let rpc_tx = build_client_side_rpc_invoke(account, calldata);

    let factory = runner_factory(&rpc_url);
    let prover = VirtualSnosProver::from_runner(factory);

    let result = prover
        .prove_transaction(BlockId::Latest, rpc_tx)
        .await
        .expect("prove_transaction failed — ensure local node is running at LOCAL_NODE_URL");

    let resources_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("apollo_integration_tests/resources/proof_flow");
    std::fs::create_dir_all(&resources_dir).expect("Failed to create resources dir");

    let proof_bytes: Vec<u8> = result.proof.0.iter().flat_map(|n| n.to_be_bytes()).collect();
    std::fs::write(resources_dir.join("proof.bin"), &proof_bytes)
        .expect("Failed to write proof.bin");

    let proof_facts_json =
        serde_json::to_string_pretty(&result.proof_facts).expect("Failed to serialize proof_facts");
    std::fs::write(resources_dir.join("proof_facts.json"), proof_facts_json)
        .expect("Failed to write proof_facts.json");

    println!("Fixtures written to {:?}", resources_dir);
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
