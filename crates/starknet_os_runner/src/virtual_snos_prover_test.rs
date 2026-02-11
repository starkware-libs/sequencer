use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use rstest::rstest;
use starknet_api::block::BlockNumber;
use starknet_api::core::ChainId;
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::rpc_transaction::{
    RpcDeclareTransaction, RpcDeclareTransactionV3, RpcDeployAccountTransaction,
    RpcDeployAccountTransactionV3, RpcInvokeTransaction, RpcInvokeTransactionV3, RpcTransaction,
};
use starknet_api::state::SierraContractClass;
use starknet_api::transaction::fields::{
    AccountDeploymentData, AllResourceBounds, Calldata, ContractAddressSalt, PaymasterData, Tip,
    TransactionSignature,
};
use starknet_api::transaction::{InvokeTransaction, TransactionHash};
use starknet_api::{class_hash, compiled_class_hash, contract_address, nonce};

use crate::errors::{
    ClassesProviderError, ProofProviderError, RunnerError, VirtualBlockExecutorError,
};
use crate::runner::{RunnerOutput, VirtualSnosRunner};
use crate::virtual_snos_prover::{
    VirtualSnosProver, VirtualSnosProverError, calculate_tx_hash, extract_invoke_tx,
};

fn create_invoke_v3_tx() -> RpcTransaction {
    RpcTransaction::Invoke(RpcInvokeTransaction::V3(RpcInvokeTransactionV3 {
        sender_address: contract_address!("0x123"),
        calldata: Calldata::default(),
        signature: TransactionSignature::default(),
        nonce: nonce!(0_u64),
        resource_bounds: AllResourceBounds::default(),
        tip: Tip::default(),
        paymaster_data: PaymasterData::default(),
        account_deployment_data: AccountDeploymentData::default(),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
        proof_facts: Default::default(),
        proof: Default::default(),
    }))
}

fn create_declare_v3_tx() -> RpcTransaction {
    RpcTransaction::Declare(RpcDeclareTransaction::V3(RpcDeclareTransactionV3 {
        sender_address: contract_address!("0x123"),
        compiled_class_hash: compiled_class_hash!(0x56_u8),
        signature: TransactionSignature::default(),
        nonce: nonce!(0_u64),
        contract_class: SierraContractClass::default(),
        resource_bounds: AllResourceBounds::default(),
        tip: Tip::default(),
        paymaster_data: PaymasterData::default(),
        account_deployment_data: AccountDeploymentData::default(),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
    }))
}

fn create_deploy_account_v3_tx() -> RpcTransaction {
    RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(RpcDeployAccountTransactionV3 {
        signature: TransactionSignature::default(),
        nonce: nonce!(0_u64),
        class_hash: class_hash!("0x789"),
        contract_address_salt: ContractAddressSalt::default(),
        constructor_calldata: Calldata::default(),
        resource_bounds: AllResourceBounds::default(),
        tip: Tip::default(),
        paymaster_data: PaymasterData::default(),
        nonce_data_availability_mode: DataAvailabilityMode::L1,
        fee_data_availability_mode: DataAvailabilityMode::L1,
    }))
}

#[rstest]
#[case::invoke(create_invoke_v3_tx(), true)]
#[case::declare(create_declare_v3_tx(), false)]
#[case::deploy_account(create_deploy_account_v3_tx(), false)]
fn extract_invoke_tx_accepts_only_invoke_transactions(
    #[case] tx: RpcTransaction,
    #[case] should_succeed: bool,
) {
    assert_eq!(extract_invoke_tx(tx).is_ok(), should_succeed);
}

#[test]
fn calculate_tx_hash_is_deterministic_and_chain_specific() {
    let invoke_tx = extract_invoke_tx(create_invoke_v3_tx()).expect("Invoke transaction is valid");

    let mainnet_hash_1 =
        calculate_tx_hash(&invoke_tx, &ChainId::Mainnet).expect("Hash should be calculated");
    let mainnet_hash_2 =
        calculate_tx_hash(&invoke_tx, &ChainId::Mainnet).expect("Hash should be calculated");
    let sepolia_hash =
        calculate_tx_hash(&invoke_tx, &ChainId::Sepolia).expect("Hash should be calculated");

    assert_eq!(mainnet_hash_1, mainnet_hash_2);
    assert_ne!(mainnet_hash_1, sepolia_hash);
}

struct MockRunnerCall {
    block_id: BlockId,
    txs: Vec<(InvokeTransaction, TransactionHash)>,
}

struct MockRunnerState {
    calls: Vec<MockRunnerCall>,
    next_result: Option<Result<RunnerOutput, RunnerError>>,
}

#[derive(Clone)]
struct MockVirtualSnosRunner {
    state: Arc<Mutex<MockRunnerState>>,
}

impl MockVirtualSnosRunner {
    fn with_error(error: RunnerError) -> Self {
        Self {
            state: Arc::new(Mutex::new(MockRunnerState {
                calls: Vec::new(),
                next_result: Some(Err(error)),
            })),
        }
    }

    fn take_calls(&self) -> Vec<MockRunnerCall> {
        std::mem::take(&mut self.state.lock().expect("Lock should not be poisoned").calls)
    }
}

#[async_trait]
impl VirtualSnosRunner for MockVirtualSnosRunner {
    async fn run_virtual_os(
        &self,
        block_id: BlockId,
        txs: Vec<(InvokeTransaction, TransactionHash)>,
    ) -> Result<RunnerOutput, RunnerError> {
        let mut state = self.state.lock().expect("Lock should not be poisoned");
        state.calls.push(MockRunnerCall { block_id, txs });
        state.next_result.take().expect("Mock runner should be called at most once")
    }
}

#[tokio::test]
async fn prove_transaction_rejects_pending_blocks_without_calling_runner() {
    let mock_runner =
        MockVirtualSnosRunner::with_error(RunnerError::InputGenerationError("unused".to_string()));
    let prover = VirtualSnosProver::from_runner(mock_runner.clone(), ChainId::Mainnet);

    let error = prover
        .prove_transaction(BlockId::Pending, create_invoke_v3_tx())
        .await
        .expect_err("Pending block should fail validation");

    assert!(matches!(
        error,
        VirtualSnosProverError::ValidationError(msg) if msg.contains("Pending")
    ));
    assert!(mock_runner.take_calls().is_empty());
}

#[tokio::test]
async fn prove_transaction_passes_block_and_tx_hash_to_runner() {
    let mock_runner = MockVirtualSnosRunner::with_error(RunnerError::InputGenerationError(
        "runner failure".to_string(),
    ));
    let prover = VirtualSnosProver::from_runner(mock_runner.clone(), ChainId::Mainnet);

    let tx = create_invoke_v3_tx();
    let expected_hash = calculate_tx_hash(
        &extract_invoke_tx(tx.clone()).expect("Transaction should be valid invoke"),
        &ChainId::Mainnet,
    )
    .expect("Hash should be calculated");
    let block_id = BlockId::Number(BlockNumber(42));

    let error =
        prover.prove_transaction(block_id, tx).await.expect_err("Runner error should propagate");
    assert!(matches!(error, VirtualSnosProverError::RunnerError(_)));

    let calls = mock_runner.take_calls();
    assert_eq!(calls.len(), 1);

    let call = calls.into_iter().next().expect("Call should exist");
    assert!(matches!(
        call.block_id,
        BlockId::Number(block_number) if block_number == BlockNumber(42)
    ));
    assert_eq!(call.txs.len(), 1);
    assert_eq!(call.txs[0].1, expected_hash);
}

#[rstest]
#[case::classes_error(
    RunnerError::ClassesProvider(ClassesProviderError::GetClassesError("classes failure".to_string())),
    "classes failure"
)]
#[case::proofs_error(
    RunnerError::ProofProvider(ProofProviderError::InvalidProofResponse("proof failure".to_string())),
    "proof failure"
)]
#[case::executor_error(
    RunnerError::VirtualBlockExecutor(VirtualBlockExecutorError::StateUnavailable),
    "Block state unavailable after execution"
)]
#[case::input_generation_error(
    RunnerError::InputGenerationError("input failure".to_string()),
    "input failure"
)]
#[tokio::test]
async fn prove_transaction_wraps_runner_errors_as_runner_error(
    #[case] runner_error: RunnerError,
    #[case] expected_message_fragment: &str,
) {
    let mock_runner = MockVirtualSnosRunner::with_error(runner_error);
    let prover = VirtualSnosProver::from_runner(mock_runner, ChainId::Mainnet);

    let error = prover
        .prove_transaction(BlockId::Latest, create_invoke_v3_tx())
        .await
        .expect_err("Runner error should propagate");

    match error {
        VirtualSnosProverError::RunnerError(inner) => {
            assert!(inner.to_string().contains(expected_message_fragment));
        }
        other => panic!("Expected RunnerError, got {other:?}"),
    }
}
