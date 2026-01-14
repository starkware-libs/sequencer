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
use starknet_api::consensus_transaction::ConsensusTransaction;
use starknet_api::executable_transaction::ValidateCompiledClassHashError;
use starknet_api::rpc_transaction::{RpcDeclareTransaction, RpcTransaction};
use starknet_api::transaction::fields::Proof;
use starknet_api::{compiled_class_hash, felt, proof_facts};

use crate::transaction_converter::{
    TransactionConverter,
    TransactionConverterError,
    TransactionConverterTrait,
};

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
    // Create an invoke transaction with proof facts and proof.
    let proof_facts = proof_facts![felt!("0x1"), felt!("0x2"), felt!("0x3")];
    let proof = Proof::from(vec![1u32, 2u32, 3u32]);
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

    let mock_class_manager_client = MockClassManagerClient::new();

    let transaction_converter = TransactionConverter::new(
        Arc::new(mock_class_manager_client),
        Arc::new(mock_proof_manager_client),
        ChainInfo::create_for_testing().chain_id,
    );

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
    let mock_class_manager_client = MockClassManagerClient::new();

    let transaction_converter = TransactionConverter::new(
        Arc::new(mock_class_manager_client),
        Arc::new(mock_proof_manager_client),
        ChainInfo::create_for_testing().chain_id,
    );

    // Convert the RPC transaction to an internal RPC transaction.
    // This should succeed without calling contains proof or set proof.
    transaction_converter.convert_rpc_tx_to_internal_rpc_tx(invoke_tx).await.unwrap();
}

#[rstest]
#[tokio::test]
async fn test_consensus_tx_to_internal_with_proof_facts_verifies_and_sets_proof() {
    let proof_facts = proof_facts![felt!("0x1"), felt!("0x2"), felt!("0x3")];
    let proof = Proof::from(vec![1u32, 2u32, 3u32]);
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

    let mock_class_manager_client = MockClassManagerClient::new();

    let transaction_converter = TransactionConverter::new(
        Arc::new(mock_class_manager_client),
        Arc::new(mock_proof_manager_client),
        ChainInfo::create_for_testing().chain_id,
    );

    // Convert the consensus transaction to an internal consensus transaction.
    // This should call contains proof and set proof.
    transaction_converter
        .convert_consensus_tx_to_internal_consensus_tx(consensus_tx)
        .await
        .unwrap();
}
