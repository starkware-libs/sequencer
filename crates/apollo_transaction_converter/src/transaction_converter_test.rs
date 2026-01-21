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
use rstest::rstest;
use starknet_api::compiled_class_hash;
use starknet_api::consensus_transaction::ConsensusTransaction;
use starknet_api::executable_transaction::ValidateCompiledClassHashError;
use starknet_api::rpc_transaction::{RpcDeclareTransaction, RpcTransaction};
use starknet_api::transaction::fields::{Proof, ProofFacts};

use crate::transaction_converter::{
    TransactionConverter,
    TransactionConverterError,
    TransactionConverterTrait,
};

fn test_proof_facts() -> ProofFacts {
    ProofFacts::snos_proof_facts_for_testing()
}

fn test_proof() -> Proof {
    Proof::proof_for_testing()
}

fn invoke_tx_with_proof() -> RpcTransaction {
    invoke_tx_client_side_proving(CairoVersion::default(), test_proof_facts(), test_proof())
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
async fn test_proof_verification_called_for_invoke_v3_with_proof_facts() {
    let invoke_tx = invoke_tx_with_proof();

    let mut mock_proof_manager_client = MockProofManagerClient::new();
    // Expect contains proof to be called and return false (proof does not exist).
    mock_proof_manager_client
        .expect_contains_proof()
        .once()
        .with(eq(test_proof_facts()))
        .return_once(|_| Ok(false));

    let transaction_converter = create_transaction_converter(mock_proof_manager_client);

    // Convert the RPC transaction to an internal RPC transaction.
    transaction_converter.convert_rpc_tx_to_internal_rpc_tx(invoke_tx).await.unwrap();
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
    transaction_converter.convert_rpc_tx_to_internal_rpc_tx(invoke_tx).await.unwrap();
}

#[rstest]
#[tokio::test]
async fn test_consensus_tx_to_internal_with_proof_facts_verifies_and_sets_proof() {
    let proof_facts = test_proof_facts();
    let proof = test_proof();
    let invoke_tx = invoke_tx_with_proof();

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
    transaction_converter
        .convert_consensus_tx_to_internal_consensus_tx(consensus_tx)
        .await
        .unwrap();
}

/// Tests round-trip conversion: RPC → Internal → RPC preserves all transaction data.
///
/// The mock bypasses actual storage - it just returns pre-configured responses.
#[rstest]
#[tokio::test]
async fn test_convert_internal_rpc_tx_to_rpc_tx_with_proof() {
    let proof_facts = test_proof_facts();
    let proof = test_proof();
    let rpc_tx = invoke_tx_with_proof();

    // Configure mock: define what methods will be called and what they should return.
    // Note: The mock doesn't have real storage - it just returns pre-programmed responses.
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
    let internal_tx =
        transaction_converter.convert_rpc_tx_to_internal_rpc_tx(rpc_tx.clone()).await.unwrap();
    let rpc_tx_from_internal =
        transaction_converter.convert_internal_rpc_tx_to_rpc_tx(internal_tx).await.unwrap();

    // Verify: no data lost in round-trip.
    assert_eq!(rpc_tx, rpc_tx_from_internal);
}
