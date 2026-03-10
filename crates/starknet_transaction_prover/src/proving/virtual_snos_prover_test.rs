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
    RpcDeployAccountTransaction, RpcDeployAccountTransactionV3, RpcInvokeTransaction,
    RpcInvokeTransactionV3, RpcTransaction,
};
use starknet_api::transaction::InvokeTransaction;
use starknet_api::transaction::fields::{
    AllResourceBounds, Proof, ProofFacts, ResourceBounds, Tip,
};
use starknet_api::{contract_address, felt};
use starknet_proof_verifier::verify_proof;
use starknet_types_core::felt::Felt;

use crate::errors::{RunnerError, VirtualSnosProverError};
use crate::proving::virtual_snos_prover::VirtualSnosProver;
use crate::running::runner::{RunnerOutput, VirtualSnosRunner};
use crate::test_utils::{
    DUMMY_ACCOUNT_ADDRESS, STRK_TOKEN_ADDRESS_SEPOLIA, build_client_side_rpc_invoke,
    resolve_test_mode, runner_factory,
};

// --- Test helpers ---

fn build_valid_invoke() -> RpcTransaction {
    let account = ContractAddress::try_from(DUMMY_ACCOUNT_ADDRESS).unwrap();
    build_client_side_rpc_invoke(account, Default::default())
}

fn extract_invoke_v3_mut(tx: &mut RpcTransaction) -> &mut RpcInvokeTransactionV3 {
    match tx {
        RpcTransaction::Invoke(RpcInvokeTransaction::V3(inner)) => inner,
        _ => panic!("Expected InvokeV3"),
    }
}

/// A runner that panics if called. Use this for tests where validation should reject
/// the transaction before the runner is invoked.
#[derive(Clone)]
struct UnreachableRunner;

#[async_trait]
impl VirtualSnosRunner for UnreachableRunner {
    async fn run_virtual_os(
        &self,
        _block_id: BlockId,
        _txs: Vec<InvokeTransaction>,
    ) -> Result<RunnerOutput, RunnerError> {
        panic!("UnreachableRunner was called — validation should have rejected the transaction");
    }
}

/// A runner that always returns an error. Use this for tests that need to verify
/// the runner is reached (i.e., validation passed) without requiring a real node.
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

// --- Unit tests: input validation ---

#[tokio::test]
async fn test_pending_block_rejected() {
    let prover = VirtualSnosProver::from_runner(UnreachableRunner).disable_fee_validation();
    let result = prover.prove_transaction(BlockId::Pending, build_valid_invoke()).await;
    assert_matches!(result, Err(VirtualSnosProverError::ValidationError(msg)) if msg.contains("Pending"));
}

#[tokio::test]
async fn test_deploy_account_transaction_rejected() {
    let deploy_account_tx = RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(
        RpcDeployAccountTransactionV3 {
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
        },
    ));

    let prover = VirtualSnosProver::from_runner(UnreachableRunner).disable_fee_validation();
    let result = prover.prove_transaction(BlockId::Latest, deploy_account_tx).await;
    assert_matches!(
        result,
        Err(VirtualSnosProverError::InvalidTransactionType(msg)) if msg.contains("DeployAccount")
    );
}

#[rstest]
#[case("non-empty proof field", {
    let mut tx = build_valid_invoke();
    extract_invoke_v3_mut(&mut tx).proof = Proof(Arc::new(vec![0u32]));
    tx
})]
#[case("non-empty proof_facts field", {
    let mut tx = build_valid_invoke();
    extract_invoke_v3_mut(&mut tx).proof_facts = ProofFacts(Arc::new(vec![Felt::ZERO]));
    tx
})]
#[tokio::test]
async fn test_non_empty_proof_fields_rejected(
    #[case] _description: &str,
    #[case] tx: RpcTransaction,
) {
    let prover = VirtualSnosProver::from_runner(UnreachableRunner).disable_fee_validation();
    let result = prover.prove_transaction(BlockId::Latest, tx).await;
    assert_matches!(result, Err(VirtualSnosProverError::InvalidTransactionInput(_)));
}

#[rstest]
#[case("non-zero l1_gas resource bounds", {
    let mut tx = build_valid_invoke();
    extract_invoke_v3_mut(&mut tx).resource_bounds = AllResourceBounds {
        l1_gas: ResourceBounds { max_amount: GasAmount(1), max_price_per_unit: GasPrice(1) },
        ..Default::default()
    };
    tx
})]
#[case("non-zero tip", {
    let mut tx = build_valid_invoke();
    extract_invoke_v3_mut(&mut tx).tip = Tip(1);
    tx
})]
#[tokio::test]
async fn test_fee_fields_rejected_when_validation_enabled(
    #[case] _description: &str,
    #[case] tx: RpcTransaction,
) {
    let prover = VirtualSnosProver::from_runner(UnreachableRunner);
    let result = prover.prove_transaction(BlockId::Latest, tx).await;
    assert_matches!(result, Err(VirtualSnosProverError::InvalidTransactionInput(_)));
}

#[tokio::test]
async fn test_non_zero_resource_bounds_accepted_when_validation_disabled() {
    let mut tx = build_valid_invoke();
    extract_invoke_v3_mut(&mut tx).resource_bounds = AllResourceBounds {
        l1_gas: ResourceBounds { max_amount: GasAmount(1), max_price_per_unit: GasPrice(1) },
        ..Default::default()
    };

    // Validation is explicitly disabled, so the runner should be reached.
    let prover = VirtualSnosProver::from_runner(FailingRunner).disable_fee_validation();
    let result = prover.prove_transaction(BlockId::Latest, tx).await;
    assert_matches!(result, Err(VirtualSnosProverError::RunnerError(_)));
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
    let prover = VirtualSnosProver::from_runner(factory).disable_fee_validation();

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
    let prover = VirtualSnosProver::from_runner(factory).disable_fee_validation();

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
