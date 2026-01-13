use std::path::PathBuf;
use std::sync::Arc;

use apollo_class_manager_types::{ClassHashes, MockClassManagerClient};
use apollo_infra_utils::path::resolve_project_relative_path;
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
use rstest::rstest;
use starknet_api::compiled_class_hash;
use starknet_api::consensus_transaction::ConsensusTransaction;
use starknet_api::executable_transaction::ValidateCompiledClassHashError;
use starknet_api::rpc_transaction::{RpcDeclareTransaction, RpcTransaction};
use starknet_api::transaction::fields::{Proof, ProofFacts};
use starknet_types_core::felt::Felt;

use crate::transaction_converter::{
    TransactionConverter,
    TransactionConverterError,
    TransactionConverterTrait,
};

/// Resource file names for testing.
const EXAMPLE_PROOF_FILE: &str = "example_proof.bz2";
const EXAMPLE_PROOF_FACTS_FILE: &str = "example_proof_facts.json";

/// Returns the absolute path to a resource file in this crate's resources directory.
fn resolve_resource_path(file_name: &str) -> PathBuf {
    let path = ["crates", "apollo_transaction_converter", "resources", file_name]
        .iter()
        .collect::<PathBuf>();
    resolve_project_relative_path(&path.to_string_lossy())
        .unwrap_or_else(|_| panic!("Failed to resolve resource path for {}", file_name))
}

/// Loads the example proof from the resources directory.
/// Uses `ProofBytes::from_file()` to load the bz2-compressed proof file.
pub fn get_proof_for_testing() -> Proof {
    let proof_path = resolve_resource_path(EXAMPLE_PROOF_FILE);
    let proof_bytes = ProofBytes::from_file(&proof_path)
        .expect("Failed to load example_proof.bz2 from resources directory");
    proof_bytes.into()
}

/// Loads the example proof facts from the resources directory.
/// Parses the JSON file containing an array of hex-encoded felt values.
pub fn get_proof_facts_for_testing() -> ProofFacts {
    let proof_facts_path = resolve_resource_path(EXAMPLE_PROOF_FACTS_FILE);
    let proof_facts_str = std::fs::read_to_string(&proof_facts_path)
        .expect("Failed to read example_proof_facts.json from resources directory");
    serde_json::from_str(&proof_facts_str).expect("Failed to parse example_proof_facts.json")
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
    // Load proof and proof facts from resources directory.
    let proof_facts = get_proof_facts_for_testing();
    let proof = get_proof_for_testing();
    let invoke_tx = invoke_tx_client_side_proving(
        CairoVersion::Cairo1(RunnableCairo1::Casm),
        proof_facts.clone(),
        proof.clone(),
    );

    let mut mock_proof_manager_client = MockProofManagerClient::new();
    // Expect contains proof to be called and return false (proof does not exists).
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
    // Load proof and proof facts from resources directory.
    let proof_facts = get_proof_facts_for_testing();
    let proof = get_proof_for_testing();
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
