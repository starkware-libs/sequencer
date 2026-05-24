//! Tests for VirtualSnosProver: unit tests for input validation and integration tests for the
//! full prove_transaction flow.
//!
//! The integration tests exercise the complete prover pipeline: transaction extraction, OS
//! execution, and proof generation. They run against Sepolia and support three modes
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
//! # Running integration tests
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

use std::sync::Arc;

use assert_matches::assert_matches;
use async_trait::async_trait;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use blockifier_test_utils::calldata::create_calldata;
use rstest::rstest;
use starknet_api::block::GasPrice;
use starknet_api::core::ContractAddress;
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction,
    RpcDeclareTransactionV3,
    RpcDeployAccountTransaction,
    RpcDeployAccountTransactionV3,
    RpcInvokeTransaction,
    RpcInvokeTransactionV3,
    RpcTransaction,
};
use starknet_api::transaction::fields::{Proof, ProofFacts, ResourceBounds, Tip};
use starknet_api::transaction::InvokeTransaction;
use starknet_api::{contract_address, felt};
use starknet_proof_verifier::verify_proof;
use starknet_types_core::felt::Felt;

use crate::errors::{RunnerError, VirtualSnosProverError};
use crate::proving::virtual_snos_prover::VirtualSnosProver;
use crate::running::runner::{RunnerOutput, VirtualSnosRunner};
use crate::server::metrics::{names as metric_names, outcomes};
use crate::server::test_recorder::{metric_value, shared_handle};
use crate::test_utils::{
    build_client_side_rpc_invoke,
    resolve_test_mode,
    runner_factory,
    DUMMY_ACCOUNT_ADDRESS,
    STRK_TOKEN_ADDRESS_SEPOLIA,
};

fn dummy_account() -> ContractAddress {
    ContractAddress::try_from(DUMMY_ACCOUNT_ADDRESS).unwrap()
}

fn valid_invoke_tx() -> RpcTransaction {
    build_client_side_rpc_invoke(dummy_account(), Default::default())
}

fn invoke_v3_mut(tx: &mut RpcTransaction) -> &mut RpcInvokeTransactionV3 {
    match tx {
        RpcTransaction::Invoke(RpcInvokeTransaction::V3(inner)) => inner,
        _ => panic!("Expected InvokeV3"),
    }
}

/// Reaching this runner means validation failed to reject — used where the test asserts rejection.
#[derive(Clone)]
struct UnreachableRunner;

#[async_trait]
impl VirtualSnosRunner for UnreachableRunner {
    async fn run_virtual_os(
        &self,
        _block_id: BlockId,
        _txs: Vec<InvokeTransaction>,
    ) -> Result<RunnerOutput, RunnerError> {
        panic!("validation should have rejected the transaction before reaching the runner");
    }
}

/// Always errors — used where the test must observe that the runner *was* reached.
#[derive(Clone)]
struct FailingRunner;

#[async_trait]
impl VirtualSnosRunner for FailingRunner {
    async fn run_virtual_os(
        &self,
        _block_id: BlockId,
        _txs: Vec<InvokeTransaction>,
    ) -> Result<RunnerOutput, RunnerError> {
        Err(RunnerError::InputGenerationError("mock error".to_string()))
    }
}

#[tokio::test]
async fn test_pending_block_rejected() {
    let prover = VirtualSnosProver::from_runner_without_fee_validation(UnreachableRunner);
    let result = prover.prove_transaction(BlockId::Pending, valid_invoke_tx()).await;
    assert_matches!(
        result,
        Err(VirtualSnosProverError::ValidationError(msg)) if msg.contains("Pending")
    );
}

#[rstest]
#[case::deploy_account(
    RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(RpcDeployAccountTransactionV3 {
        signature: Default::default(),
        nonce: Default::default(),
        class_hash: Default::default(),
        contract_address_salt: Default::default(),
        constructor_calldata: Default::default(),
        resource_bounds: Default::default(),
        tip: Default::default(),
        paymaster_data: Default::default(),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
    })),
    "DeployAccount"
)]
#[case::declare(
    RpcTransaction::Declare(RpcDeclareTransaction::V3(RpcDeclareTransactionV3 {
        sender_address: Default::default(),
        compiled_class_hash: Default::default(),
        signature: Default::default(),
        nonce: Default::default(),
        contract_class: Default::default(),
        resource_bounds: Default::default(),
        tip: Default::default(),
        paymaster_data: Default::default(),
        account_deployment_data: Default::default(),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
    })),
    "Declare"
)]
#[tokio::test]
async fn test_non_invoke_transaction_type_rejected(
    #[case] tx: RpcTransaction,
    #[case] expected_message_substring: &str,
) {
    let prover = VirtualSnosProver::from_runner_without_fee_validation(UnreachableRunner);
    let result = prover.prove_transaction(BlockId::Latest, tx).await;
    assert_matches!(
        result,
        Err(VirtualSnosProverError::InvalidTransactionType(msg))
            if msg.contains(expected_message_substring)
    );
}

#[rstest]
#[case::non_empty_proof({
    let mut tx = valid_invoke_tx();
    invoke_v3_mut(&mut tx).proof = Proof(Arc::new(vec![0u8]));
    tx
})]
#[case::non_empty_proof_facts({
    let mut tx = valid_invoke_tx();
    invoke_v3_mut(&mut tx).proof_facts = ProofFacts(Arc::new(vec![Felt::ZERO]));
    tx
})]
#[tokio::test]
async fn test_non_empty_proof_fields_rejected(#[case] tx: RpcTransaction) {
    let prover = VirtualSnosProver::from_runner_without_fee_validation(UnreachableRunner);
    let result = prover.prove_transaction(BlockId::Latest, tx).await;
    assert_matches!(result, Err(VirtualSnosProverError::InvalidTransactionInput(_)));
}

#[rstest]
#[case::non_zero_l1_gas_price(
    {
        let mut tx = valid_invoke_tx();
        invoke_v3_mut(&mut tx).resource_bounds.l1_gas.max_price_per_unit = GasPrice(1);
        tx
    },
    "l1_gas.max_price_per_unit"
)]
#[case::non_zero_l2_gas_price(
    {
        let mut tx = valid_invoke_tx();
        invoke_v3_mut(&mut tx).resource_bounds.l2_gas.max_price_per_unit = GasPrice(1);
        tx
    },
    "l2_gas.max_price_per_unit"
)]
#[case::non_zero_l1_data_gas_price(
    {
        let mut tx = valid_invoke_tx();
        invoke_v3_mut(&mut tx).resource_bounds.l1_data_gas.max_price_per_unit = GasPrice(1);
        tx
    },
    "l1_data_gas.max_price_per_unit"
)]
#[case::non_zero_tip(
    {
        let mut tx = valid_invoke_tx();
        invoke_v3_mut(&mut tx).tip = Tip(1);
        tx
    },
    "tip ="
)]
#[case::zero_l2_gas_max_amount(
    {
        let mut tx = valid_invoke_tx();
        invoke_v3_mut(&mut tx).resource_bounds.l2_gas.max_amount = GasAmount(0);
        tx
    },
    "l2_gas.max_amount"
)]
#[tokio::test]
async fn test_fee_fields_rejected_when_validation_enabled(
    #[case] tx: RpcTransaction,
    #[case] expected_message_substring: &str,
) {
    let prover = VirtualSnosProver::from_runner(UnreachableRunner);
    let result = prover.prove_transaction(BlockId::Latest, tx).await;
    assert_matches!(
        result,
        Err(VirtualSnosProverError::InvalidTransactionInput(msg))
            if msg.contains(expected_message_substring)
    );
}

#[tokio::test]
async fn test_non_zero_resource_bounds_accepted_when_validation_disabled() {
    let mut tx = valid_invoke_tx();
    invoke_v3_mut(&mut tx).resource_bounds.l1_gas =
        ResourceBounds { max_amount: GasAmount(1), max_price_per_unit: GasPrice(1) };

    let prover = VirtualSnosProver::from_runner_without_fee_validation(FailingRunner);
    let result = prover.prove_transaction(BlockId::Latest, tx).await;
    assert_matches!(result, Err(VirtualSnosProverError::RunnerError(_)));
}

/// A default-valid invoke (empty proof fields, zero fees, Latest block) must pass every validation
/// gate and be handed to the runner. Asserting on `RunnerError::InputGenerationError("mock error")`
/// proves the runner was reached; any rejection earlier would surface as a different variant.
#[tokio::test]
async fn test_valid_invoke_reaches_runner() {
    let prover = VirtualSnosProver::from_runner(FailingRunner);
    let result = prover.prove_transaction(BlockId::Latest, valid_invoke_tx()).await;
    assert_matches!(
        result,
        Err(VirtualSnosProverError::RunnerError(inner)) if matches!(
            *inner,
            RunnerError::InputGenerationError(ref msg) if msg == "mock error"
        )
    );
}

/// `l1_gas.max_amount` and `l1_data_gas.max_amount` do not affect OS execution and may be
/// non-zero even with fee-field validation enabled; only `max_price_per_unit` fields and `tip`
/// are required to be zero. Asserting on `RunnerError::InputGenerationError("mock error")` proves
/// the runner was reached.
#[tokio::test]
async fn test_non_fee_max_amount_fields_accepted_when_validation_enabled() {
    let mut tx = valid_invoke_tx();
    let bounds = &mut invoke_v3_mut(&mut tx).resource_bounds;
    bounds.l1_gas.max_amount = GasAmount(1);
    bounds.l1_data_gas.max_amount = GasAmount(1);

    let prover = VirtualSnosProver::from_runner(FailingRunner);
    let result = prover.prove_transaction(BlockId::Latest, tx).await;
    assert_matches!(
        result,
        Err(VirtualSnosProverError::RunnerError(inner)) if matches!(
            *inner,
            RunnerError::InputGenerationError(ref msg) if msg == "mock error"
        )
    );
}

/// Prometheus sample line for the outcome counter at a given `outcome` label. Callers baseline
/// it before a request and assert the delta after, because the recorder is process-global
/// (see `test_recorder`).
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
    let account = dummy_account();
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
