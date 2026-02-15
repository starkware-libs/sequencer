use std::sync::Arc;

use apollo_class_manager_types::{ClassHashes, MockClassManagerClient};
use apollo_proof_manager_types::MockProofManagerClient;
use assert_matches::assert_matches;
use blockifier::context::ChainInfo;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use mempool_test_utils::starknet_api_test_utils::{
    declare_tx,
    invoke_tx,
    invoke_tx_client_side_proving,
};
use mockall::predicate::eq;
use proving_utils::proof_encoding::ProofBytes;
use rstest::{fixture, rstest};
use starknet_api::compiled_class_hash;
use starknet_api::consensus_transaction::ConsensusTransaction;
use starknet_api::executable_transaction::ValidateCompiledClassHashError;
use starknet_api::rpc_transaction::{RpcDeclareTransaction, RpcTransaction};
use starknet_api::test_utils::{path_in_resources, read_json_file};
use starknet_api::transaction::fields::{Proof, ProofFacts};

use crate::transaction_converter::{
    TransactionConverter,
    TransactionConverterError,
    TransactionConverterTrait,
    VerificationHandle,
};

/// Resource file names for testing.
const EXAMPLE_PROOF_FILE: &str = "example_proof.bz2";
const EXAMPLE_PROOF_FACTS_FILE: &str = "example_proof_facts.json";

/// Loads the example proof from the resources directory.
/// Uses `ProofBytes::from_file()` to load the bz2-compressed proof file.
#[fixture]
fn proof() -> Proof {
    let proof_path = path_in_resources(EXAMPLE_PROOF_FILE);
    let proof_bytes = ProofBytes::from_file(&proof_path)
        .expect("Failed to load example_proof.bz2 from resources directory");
    proof_bytes.into()
}

/// Loads the example proof facts from the resources directory.
#[fixture]
fn proof_facts() -> ProofFacts {
    read_json_file(EXAMPLE_PROOF_FACTS_FILE)
}

/// Creates a transaction converter with empty class manager mock.
fn create_transaction_converter(
    mock_proof_manager_client: MockProofManagerClient,
) -> TransactionConverter {
    TransactionConverter::new(
        Arc::new(MockClassManagerClient::new()),
        Arc::new(mock_proof_manager_client),
        ChainInfo::create_for_testing().chain_id,
    )
}

async fn await_verification_task(verification_handle: Option<VerificationHandle>) {
    if let Some(handle) = verification_handle {
        handle
            .verification_task
            .await
            .expect("verification task panicked")
            .expect("proof verification failed");
    }
}

#[rstest]
#[tokio::test]
async fn test_compiled_class_hash_mismatch() {
    let declare_tx = declare_tx();
    let declare_tx_inner = assert_matches!(declare_tx.clone(), RpcTransaction::Declare(RpcDeclareTransaction::V3(tx)) => tx);

    let other_compiled_class_hash = compiled_class_hash!(2_u8);
    assert_ne!(declare_tx_inner.compiled_class_hash, other_compiled_class_hash);

    let mut mock_class_manager_client = MockClassManagerClient::new();
    let mock_proof_manager_client = MockProofManagerClient::new();

    mock_class_manager_client
        .expect_add_class()
        .once()
        .with(eq(declare_tx_inner.contract_class.clone()))
        .return_once(move |_| {
            Ok(ClassHashes {
                class_hash: declare_tx_inner.contract_class.calculate_class_hash(),
                executable_class_hash_v2: other_compiled_class_hash,
            })
        });

    let transaction_converter = TransactionConverter::new(
        Arc::new(mock_class_manager_client),
        Arc::new(mock_proof_manager_client),
        ChainInfo::create_for_testing().chain_id,
    );

    let err =
        transaction_converter.convert_rpc_tx_to_internal_rpc_tx(declare_tx).await.unwrap_err();
    let expected_code = TransactionConverterError::ValidateCompiledClassHashError(
        ValidateCompiledClassHashError::CompiledClassHashMismatch {
            computed_class_hash: other_compiled_class_hash,
            supplied_class_hash: declare_tx_inner.compiled_class_hash,
        },
    );
    assert_eq!(err, expected_code);
}

#[rstest]
#[tokio::test]
async fn test_proof_verification_called_for_invoke_v3_with_proof_facts(
    proof_facts: ProofFacts,
    proof: Proof,
) {
    let invoke_tx = invoke_tx_client_side_proving(
        CairoVersion::Cairo1(RunnableCairo1::Casm),
        proof_facts.clone(),
        proof.clone(),
    );

    let mut mock_proof_manager_client = MockProofManagerClient::new();
    // Expect contains proof to be called and return false (proof does not exist).
    mock_proof_manager_client
        .expect_contains_proof()
        .once()
        .with(eq(proof_facts.clone()))
        .return_once(|_| Ok(false));

    let transaction_converter = create_transaction_converter(mock_proof_manager_client);

    // Convert the RPC transaction to an internal RPC transaction.
    let (_internal_tx, verification_handle) =
        transaction_converter.convert_rpc_tx_to_internal_rpc_tx(invoke_tx).await.unwrap();

    // Await the verification task to ensure proof verification completes.
    await_verification_task(verification_handle).await;
}

#[rstest]
#[tokio::test]
async fn test_proof_verification_skipped_for_invoke_v3_without_proof_facts() {
    // Create an invoke transaction without proof_facts.
    let invoke_tx = invoke_tx(CairoVersion::Cairo1(RunnableCairo1::Casm));

    // Mock proof manager client expects NO calls to contains proof or set proof.
    let mock_proof_manager_client = MockProofManagerClient::new();
    let transaction_converter = create_transaction_converter(mock_proof_manager_client);

    // Convert the RPC transaction to an internal RPC transaction.
    // This should succeed without calling contains proof or set proof.
    let (_internal_tx, verification_handle) =
        transaction_converter.convert_rpc_tx_to_internal_rpc_tx(invoke_tx).await.unwrap();

    // Verify that no verification handle was returned (no proof to verify).
    assert!(verification_handle.is_none());
}

#[rstest]
#[tokio::test]
async fn test_consensus_tx_to_internal_with_proof_facts_verifies_and_sets_proof(
    proof_facts: ProofFacts,
    proof: Proof,
) {
    let invoke_tx = invoke_tx_client_side_proving(
        CairoVersion::Cairo1(RunnableCairo1::Casm),
        proof_facts.clone(),
        proof.clone(),
    );

    let consensus_tx = ConsensusTransaction::RpcTransaction(invoke_tx);

    let mut mock_proof_manager_client = MockProofManagerClient::new();

    // Expect contains proof to be called during conversion from rpc to internal rpc.
    mock_proof_manager_client
        .expect_contains_proof()
        .once()
        .with(eq(proof_facts.clone()))
        .return_once(|_| Ok(false));

    // Expect set proof to be called after the conversion succeeds.
    // This is specific to conversion from consensus to internal consensus.
    mock_proof_manager_client
        .expect_set_proof()
        .once()
        .with(eq(proof_facts), eq(proof))
        .return_once(|_, _| Ok(()));

    let transaction_converter = create_transaction_converter(mock_proof_manager_client);

    // Convert the consensus transaction to an internal consensus transaction.
    // This should call contains proof and set proof.
    let (_internal_tx, verification_handle) = transaction_converter
        .convert_consensus_tx_to_internal_consensus_tx(consensus_tx)
        .await
        .unwrap();

    // Await the verification task to ensure proof verification completes.
    await_verification_task(verification_handle).await;
}

/// Tests round-trip conversion: RPC → Internal → RPC preserves all transaction data.
#[rstest]
#[tokio::test]
async fn test_convert_internal_rpc_tx_to_rpc_tx_with_proof(proof_facts: ProofFacts, proof: Proof) {
    let rpc_tx =
        invoke_tx_client_side_proving(CairoVersion::default(), proof_facts.clone(), proof.clone());

    let mut mock_proof_manager_client = MockProofManagerClient::new();

    // Step 1 (RPC → Internal): Converter checks if proof exists.
    mock_proof_manager_client
        .expect_contains_proof()
        .once()
        .with(eq(proof_facts.clone()))
        .return_once(|_| Ok(false));

    // Step 2 (Internal → RPC): Converter retrieves the proof to reconstruct the RPC tx.
    mock_proof_manager_client
        .expect_get_proof()
        .once()
        .with(eq(proof_facts))
        .return_once(move |_| Ok(Some(proof)));

    let transaction_converter = create_transaction_converter(mock_proof_manager_client);

    // Execute round-trip conversion.
    let (internal_tx, verification_handle) =
        transaction_converter.convert_rpc_tx_to_internal_rpc_tx(rpc_tx.clone()).await.unwrap();

    // Await the verification task to ensure proof verification completes.
    await_verification_task(verification_handle).await;

    let rpc_tx_from_internal =
        transaction_converter.convert_internal_rpc_tx_to_rpc_tx(internal_tx).await.unwrap();

    // Verify: no data lost in round-trip.
    assert_eq!(rpc_tx, rpc_tx_from_internal);
}
