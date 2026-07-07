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
use rstest::{fixture, rstest};
use starknet_api::consensus_transaction::ConsensusTransaction;
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::executable_transaction::{AccountTransaction, ValidateCompiledClassHashError};
use starknet_api::rpc_transaction::{RpcDeclareTransaction, RpcTransaction};
use starknet_api::state::SierraContractClass;
use starknet_api::test_utils::declare::{default_compiled_contract_class, internal_rpc_declare_tx};
use starknet_api::test_utils::{path_in_resources, read_json_file};
use starknet_api::transaction::fields::{Proof, ProofFacts};
use starknet_api::{compiled_class_hash, declare_tx_args};

use crate::transaction_converter::{
    TransactionConverter,
    TransactionConverterError,
    TransactionConverterTrait,
    VerificationHandle,
    VerifyAndStoreProofTask,
};

/// Resource file names for testing.
const EXAMPLE_PROOF_FILE: &str = "example_proof.bin";
const EXAMPLE_PROOF_FACTS_FILE: &str = "example_proof_facts.json";

/// Loads the example proof from the resources directory as raw binary bytes.
#[fixture]
fn proof() -> Proof {
    let proof_path = path_in_resources(EXAMPLE_PROOF_FILE);
    let raw_bytes =
        std::fs::read(&proof_path).expect("Failed to read example_proof.bin from resources");
    Proof::from(raw_bytes)
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

async fn await_verification_handle(verification_handle: Option<VerificationHandle>) {
    if let Some(handle) = verification_handle {
        handle
            .verification_task
            .await
            .expect("verification task panicked")
            .expect("proof verification failed");
    }
}

async fn await_verify_and_store_proof_task(task: Option<VerifyAndStoreProofTask>) {
    if let Some(task) = task {
        task.await
            .expect("verify and store proof task panicked")
            .expect("verify and store proof task failed");
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
    mock_proof_manager_client
        .expect_contains_proof()
        .once()
        .with(eq(proof_facts.clone()))
        .return_once(|_| Ok(false));

    let transaction_converter = create_transaction_converter(mock_proof_manager_client);

    let (_internal_tx, verification_handle) =
        transaction_converter.convert_rpc_tx_to_internal_rpc_tx(invoke_tx).await.unwrap();

    await_verification_handle(verification_handle).await;
}

#[rstest]
#[tokio::test]
async fn test_proof_verification_skipped_for_invoke_v3_without_proof_facts() {
    let invoke_tx = invoke_tx(CairoVersion::Cairo1(RunnableCairo1::Casm));

    // No expectations set — mock asserts that neither contains_proof nor set_proof are called.
    let mock_proof_manager_client = MockProofManagerClient::new();
    let transaction_converter = create_transaction_converter(mock_proof_manager_client);

    let (_internal_tx, verification_handle) =
        transaction_converter.convert_rpc_tx_to_internal_rpc_tx(invoke_tx).await.unwrap();

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

    mock_proof_manager_client
        .expect_contains_proof()
        .once()
        .with(eq(proof_facts.clone()))
        .return_once(|_| Ok(false));

    // set_proof should be called only after successful verification.
    mock_proof_manager_client
        .expect_set_proof()
        .once()
        .with(eq(proof_facts.clone()), eq(proof.clone()))
        .return_once(|_, _| Ok(()));

    let transaction_converter = create_transaction_converter(mock_proof_manager_client);

    let (_internal_tx, verify_and_store_proof_task) = transaction_converter
        .convert_consensus_tx_to_internal_consensus_tx(consensus_tx)
        .await
        .unwrap();

    await_verify_and_store_proof_task(verify_and_store_proof_task).await;
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

    let (internal_tx, verification_handle) =
        transaction_converter.convert_rpc_tx_to_internal_rpc_tx(rpc_tx.clone()).await.unwrap();

    await_verification_handle(verification_handle).await;

    let rpc_tx_from_internal =
        transaction_converter.convert_internal_rpc_tx_to_rpc_tx(internal_tx).await.unwrap();

    assert_eq!(rpc_tx, rpc_tx_from_internal);
}

/// Sets up a mock class manager returning `sierra`/`contract_class` for `class_hash`, and
/// converts an internal declare tx (with the given `class_hash`/`compiled_class_hash`) to an
/// executable tx.
async fn convert_declare_tx_to_executable(
    class_hash: ClassHash,
    compiled_class_hash: CompiledClassHash,
    sierra: SierraContractClass,
    contract_class: ContractClass,
) -> Result<AccountTransaction, TransactionConverterError> {
    let internal_tx =
        internal_rpc_declare_tx(declare_tx_args!(class_hash: class_hash, compiled_class_hash));

    let mut mock_class_manager_client = MockClassManagerClient::new();
    mock_class_manager_client
        .expect_get_sierra()
        .with(eq(class_hash))
        .return_once(move |_| Ok(Some(sierra)));
    mock_class_manager_client
        .expect_get_executable()
        .with(eq(class_hash))
        .return_once(move |_| Ok(Some(contract_class)));

    let transaction_converter = TransactionConverter::new(
        Arc::new(mock_class_manager_client),
        Arc::new(MockProofManagerClient::new()),
        ChainInfo::create_for_testing().chain_id,
    );

    transaction_converter.convert_internal_rpc_tx_to_executable_tx(internal_tx).await
}

/// A consistent Sierra/executable pair (both keyed to the same class/compiled-class hash)
/// converts successfully.
#[rstest]
#[tokio::test]
async fn test_convert_internal_rpc_tx_to_executable_tx_declare_consistent_classes() {
    let sierra = SierraContractClass::default();
    let class_hash = sierra.calculate_class_hash();
    let contract_class = default_compiled_contract_class();
    let compiled_class_hash = contract_class.compiled_class_hash();

    let account_tx = convert_declare_tx_to_executable(
        class_hash,
        compiled_class_hash,
        sierra.clone(),
        contract_class.clone(),
    )
    .await
    .unwrap();

    let class_info = assert_matches!(account_tx, AccountTransaction::Declare(tx) => tx.class_info);
    assert_eq!(class_info.contract_class, contract_class);
    assert_eq!(class_info.sierra_program_length, sierra.sierra_program.len());
    assert_eq!(class_info.abi_length, sierra.abi.len());
}

/// If the class manager returns a Sierra that doesn't hash to the requested class hash (e.g. a
/// stale cache entry for a different class), the conversion fails instead of silently deriving
/// billing metadata from the wrong Sierra.
#[rstest]
#[tokio::test]
async fn test_convert_internal_rpc_tx_to_executable_tx_declare_sierra_class_hash_mismatch() {
    let sierra = SierraContractClass::default();
    let computed_class_hash = sierra.calculate_class_hash();
    let class_hash = ClassHash::default();
    assert_ne!(class_hash, computed_class_hash);
    let contract_class = default_compiled_contract_class();
    let compiled_class_hash = contract_class.compiled_class_hash();

    let err =
        convert_declare_tx_to_executable(class_hash, compiled_class_hash, sierra, contract_class)
            .await
            .unwrap_err();

    assert_eq!(
        err,
        TransactionConverterError::SierraClassHashMismatch { class_hash, computed_class_hash }
    );
}

/// If the class manager returns an executable whose compiled class hash doesn't match the
/// declare tx's `compiled_class_hash` (e.g. a stale/incorrect compilation for the same class),
/// the conversion fails instead of pairing it with the fetched Sierra.
#[rstest]
#[tokio::test]
async fn test_convert_internal_rpc_tx_to_executable_tx_declare_compiled_class_hash_mismatch() {
    let sierra = SierraContractClass::default();
    let class_hash = sierra.calculate_class_hash();
    let contract_class = default_compiled_contract_class();
    let computed_compiled_class_hash = contract_class.compiled_class_hash();
    let compiled_class_hash = compiled_class_hash!(999_u16);
    assert_ne!(compiled_class_hash, computed_compiled_class_hash);

    let err =
        convert_declare_tx_to_executable(class_hash, compiled_class_hash, sierra, contract_class)
            .await
            .unwrap_err();

    assert_eq!(
        err,
        TransactionConverterError::ValidateCompiledClassHashError(
            ValidateCompiledClassHashError::CompiledClassHashMismatch {
                computed_class_hash: computed_compiled_class_hash,
                supplied_class_hash: compiled_class_hash,
            }
        )
    );
}
