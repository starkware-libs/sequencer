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

#[cfg(not(feature = "stwo_proving"))]
use crate::errors::VirtualSnosProverError;
use crate::proving::virtual_snos_prover::VirtualSnosProver;
use crate::server::metrics::{names as metric_names, outcomes};
use crate::server::test_recorder::{metric_value, shared_handle};
use crate::test_utils::{
    build_client_side_rpc_invoke,
    resolve_test_mode,
    runner_factory,
    DUMMY_ACCOUNT_ADDRESS,
    STRK_TOKEN_ADDRESS_SEPOLIA,
};

/// Sample line for the outcome counter at a given `outcome` label, and for the proving-duration
/// histogram's `_count`. Callers take a baseline before a request and assert the delta after,
/// because the Prometheus recorder is process-global (see `test_recorder`).
fn outcome_total_line(outcome: &str) -> String {
    format!("{}{{outcome=\"{}\"}}", metric_names::PROVE_TRANSACTION_OUTCOME_TOTAL, outcome)
}

fn duration_count_line() -> String {
    format!("{}_count", metric_names::PROVE_TRANSACTION_DURATION_SECONDS)
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

    // Baseline the outcome counter and duration histogram so we can assert this request's deltas.
    let handle = shared_handle();
    let success_line = outcome_total_line(outcomes::SUCCESS);
    let count_line = duration_count_line();
    let before_success = metric_value(&handle.render(), &success_line);
    let before_count = metric_value(&handle.render(), &count_line);

    // Run the full prover pipeline: OS execution → proof generation.
    let result = prover.prove_transaction(BlockId::Latest, rpc_tx).await;

    // Finalize recording before asserting so records are saved even on failure.
    test_mode.finalize();

    // Verify execution and proving succeeded.
    let output = result.expect("prove_transaction should succeed");

    // A successful prove records exactly one `success` outcome and one duration observation.
    let scrape = handle.render();
    assert_eq!(metric_value(&scrape, &success_line) - before_success, 1.0, "success outcome delta");
    assert_eq!(metric_value(&scrape, &count_line) - before_count, 1.0, "duration count delta");

    // Verify the proof against the proof facts.
    let proof_facts = output.proof_facts.clone();
    let proof = output.proof.clone();
    tokio::task::spawn_blocking(move || verify_proof(proof_facts, proof))
        .await
        .expect("proof verification task panicked")
        .expect("proof verification should succeed");
}

/// The proving-outcome counter and duration histogram are recorded for every request, including
/// failures. A pending block is rejected during input validation — before any runner or proving
/// work — so this asserts the failure-path recording without a live node or the `stwo_proving`
/// feature. Deleting either the outcome-counter or the duration-histogram emission fails this test.
#[cfg(not(feature = "stwo_proving"))]
#[tokio::test]
async fn prove_transaction_records_validation_failure_outcome_and_duration() {
    let handle = shared_handle();
    let outcome_line = outcome_total_line(outcomes::VALIDATION);
    let count_line = duration_count_line();
    let before_outcome = metric_value(&handle.render(), &outcome_line);
    let before_count = metric_value(&handle.render(), &count_line);

    let prover = VirtualSnosProver::from_runner(runner_factory("http://localhost:1"));
    let account = ContractAddress::try_from(DUMMY_ACCOUNT_ADDRESS).unwrap();
    let tx = build_client_side_rpc_invoke(account, create_calldata(account, "noop", &[]));
    let result = prover.prove_transaction(BlockId::Pending, tx).await;
    assert!(
        matches!(result, Err(VirtualSnosProverError::ValidationError(_))),
        "pending block should fail validation, got: {result:?}"
    );

    let scrape = handle.render();
    assert_eq!(
        metric_value(&scrape, &outcome_line) - before_outcome,
        1.0,
        "failure_validation outcome delta"
    );
    assert_eq!(metric_value(&scrape, &count_line) - before_count, 1.0, "duration count delta");
}
