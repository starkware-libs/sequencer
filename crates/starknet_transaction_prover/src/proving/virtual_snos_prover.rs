//! Virtual SNOS prover for generating transaction proofs.
//!
//! This module contains the core proving logic, extracted from the HTTP layer
//! to enable better separation of concerns and testability.

use std::time::Instant;

use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use blockifier_reexecution::utils::get_chain_info;
use serde::{Deserialize, Serialize};
use starknet_api::rpc_transaction::{RpcInvokeTransaction, RpcInvokeTransactionV3, RpcTransaction};
use starknet_api::transaction::fields::{Fee, Proof, ProofFacts, ValidResourceBounds};
use starknet_api::transaction::{InvokeTransaction, MessageToL1};
use tracing::{info, instrument};
use url::Url;

use crate::config::ProverConfig;
use crate::errors::VirtualSnosProverError;
use crate::running::runner::{RpcRunnerFactory, RunnerOutput, VirtualSnosRunner};

/// Result of a successful prove transaction operation.
///
/// This struct is used both as the RPC response and as part of the internal prover output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProveTransactionResult {
    /// The generated proof.
    pub proof: Proof,
    /// The proof facts.
    pub proof_facts: ProofFacts,
    /// Messages sent from L2 to L1 during execution.
    pub l2_to_l1_messages: Vec<MessageToL1>,
}

/// Virtual SNOS prover for Starknet transactions.
///
/// Encapsulates all proving logic, including OS execution and proof generation.
/// This prover is independent of the HTTP layer and can be used directly for testing.
///
/// The prover is generic over the runner, allowing different implementations
/// (RPC-based, mock, etc.) to be used interchangeably.
#[derive(Clone)]
pub(crate) struct VirtualSnosProver<R: VirtualSnosRunner> {
    /// Runner for executing the virtual OS.
    runner: R,
    /// Whether to validate that fee-related fields (resource bounds, tip) are zero.
    validate_zero_fee_fields: bool,
}

/// Type alias for the RPC-based virtual SNOS prover.
pub(crate) type RpcVirtualSnosProver = VirtualSnosProver<RpcRunnerFactory>;

impl VirtualSnosProver<RpcRunnerFactory> {
    /// Creates a new VirtualSnosProver from configuration.
    ///
    /// This constructor creates an RPC-based prover using the configuration values.
    pub fn new(prover_config: &ProverConfig) -> Self {
        let contract_class_manager =
            ContractClassManager::start(prover_config.contract_class_manager_config.clone());
        let node_url =
            Url::parse(&prover_config.rpc_node_url).expect("Invalid RPC node URL in config");
        let chain_info =
            get_chain_info(&prover_config.chain_id, prover_config.strk_fee_token_address);
        let runner = RpcRunnerFactory::new(
            node_url,
            chain_info,
            contract_class_manager,
            prover_config.runner_config.clone(),
        );
        Self { runner, validate_zero_fee_fields: prover_config.validate_zero_fee_fields }
    }
}

impl<R: VirtualSnosRunner> VirtualSnosProver<R> {
    /// Creates a new VirtualSnosProver from a runner.
    ///
    /// This constructor allows using any runner implementation.
    #[allow(dead_code)]
    pub(crate) fn from_runner(runner: R) -> Self {
        Self { runner, validate_zero_fee_fields: true }
    }

    /// Disables fee-field validation (resource bounds, tip).
    #[allow(dead_code)]
    pub(crate) fn disable_fee_validation(mut self) -> Self {
        self.validate_zero_fee_fields = false;
        self
    }

    /// Proves a transaction on top of the specified block.
    ///
    /// This method:
    /// 1. Validates and extracts the invoke transaction.
    /// 2. Runs the Starknet OS.
    /// 3. Generates a proof via `prove_virtual_snos_run`.
    #[instrument(skip(self, transaction), fields(block_id = ?block_id, tx_hash))]
    pub async fn prove_transaction(
        &self,
        block_id: BlockId,
        transaction: RpcTransaction,
    ) -> Result<ProveTransactionResult, VirtualSnosProverError> {
        let start_time = Instant::now();

        // Validate block_id is not pending.
        if matches!(block_id, BlockId::Pending) {
            return Err(VirtualSnosProverError::ValidationError(
                "Pending blocks are not supported; only finalized blocks can be proven."
                    .to_string(),
            ));
        }

        let invoke_v3 = extract_rpc_invoke_tx(transaction)?;
        validate_transaction_input(&invoke_v3, self.validate_zero_fee_fields)?;
        let invoke_tx = InvokeTransaction::V3(invoke_v3.into());

        // Run OS and get output.
        let os_start = Instant::now();

        let txs = vec![invoke_tx];
        let runner_output = self
            .runner
            .run_virtual_os(block_id, txs)
            .await
            .map_err(|err| VirtualSnosProverError::RunnerError(Box::new(err)))?;

        info!(
            os_duration_ms = %os_start.elapsed().as_millis(),
            "OS execution completed"
        );

        // Run the prover.
        let prove_start = Instant::now();
        let result = prove_virtual_snos_run(runner_output).await?;

        info!(
            prove_duration_ms = %prove_start.elapsed().as_millis(),
            "Proving completed"
        );

        info!(total_duration_ms = %start_time.elapsed().as_millis(), "prove_transaction completed");
        Ok(result)
    }
}

/// Proves a Virtual Starknet OS run from its output.
///
/// Generates a proof from the given [`RunnerOutput`] and converts the program output into proof
/// facts.
pub async fn prove_virtual_snos_run(
    runner_output: RunnerOutput,
) -> Result<ProveTransactionResult, VirtualSnosProverError> {
    #[cfg(not(feature = "stwo_proving"))]
    {
        let _ = runner_output;
        unimplemented!(
            "In-memory proving requires the `stwo_proving` feature flag and a nightly Rust \
             toolchain."
        );
    }

    #[cfg(feature = "stwo_proving")]
    {
        use starknet_api::transaction::fields::VIRTUAL_SNOS;

        use crate::proving::prover::prove;

        let prover_output = prove(runner_output.cairo_pie).await?;
        // Convert program output to proof facts using VIRTUAL_SNOS variant marker.
        let proof_facts = prover_output.program_output.try_into_proof_facts(VIRTUAL_SNOS)?;

        Ok(ProveTransactionResult {
            proof: prover_output.proof,
            proof_facts,
            l2_to_l1_messages: runner_output.l2_to_l1_messages,
        })
    }
}

/// Extracts the RPC Invoke V3 transaction, rejecting other transaction types.
fn extract_rpc_invoke_tx(
    tx: RpcTransaction,
) -> Result<RpcInvokeTransactionV3, VirtualSnosProverError> {
    match tx {
        RpcTransaction::Invoke(RpcInvokeTransaction::V3(invoke_v3)) => Ok(invoke_v3),
        RpcTransaction::Declare(_) => Err(VirtualSnosProverError::InvalidTransactionType(
            "Declare transactions are not supported; only Invoke transactions are allowed"
                .to_string(),
        )),
        RpcTransaction::DeployAccount(_) => Err(VirtualSnosProverError::InvalidTransactionType(
            "DeployAccount transactions are not supported; only Invoke transactions are allowed"
                .to_string(),
        )),
    }
}

/// Validates that the transaction input fields are acceptable for proving.
///
/// Rejects transactions where:
/// - `proof` or `proof_facts` are non-empty (these are output-only fields).
/// - Max possible fee is non-zero (when `validate_zero_fee_fields` is enabled).
fn validate_transaction_input(
    tx: &RpcInvokeTransactionV3,
    validate_zero_fee_fields: bool,
) -> Result<(), VirtualSnosProverError> {
    if !tx.proof.is_empty() {
        return Err(VirtualSnosProverError::InvalidTransactionInput(
            "The proof field must be empty on input.".to_string(),
        ));
    }
    if !tx.proof_facts.is_empty() {
        return Err(VirtualSnosProverError::InvalidTransactionInput(
            "The proof_facts field must be empty on input.".to_string(),
        ));
    }
    if validate_zero_fee_fields {
        let resource_bounds = ValidResourceBounds::AllResources(tx.resource_bounds);
        let max_fee = resource_bounds.max_possible_fee(tx.tip);
        if max_fee != Fee(0) {
            return Err(VirtualSnosProverError::InvalidTransactionInput(format!(
                "Max possible fee must be zero, got: {max_fee}."
            )));
        }
    }
    Ok(())
}
