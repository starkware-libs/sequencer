//! Virtual SNOS prover for generating transaction proofs.
//!
//! This module contains the core proving logic, extracted from the HTTP layer
//! to enable better separation of concerns and testability.

use std::time::{Duration, Instant};

use apollo_transaction_converter::ProgramOutputError;
use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use serde::{Deserialize, Serialize};
use starknet_api::rpc_transaction::{RpcInvokeTransaction, RpcTransaction};
use starknet_api::transaction::fields::{Proof, ProofFacts, VIRTUAL_SNOS};
use starknet_api::transaction::{InvokeTransaction, MessageToL1};
use starknet_os::io::os_output::OsOutputError;
use tracing::{info, instrument};
use url::Url;

use crate::config::ProverConfig;
use crate::errors::{ProvingError, RunnerError};
use crate::proving::prover::prove;
use crate::runner::{RpcRunnerFactory, VirtualSnosRunner};

/// Error type for the virtual SNOS prover.
#[derive(Debug, thiserror::Error)]
pub enum VirtualSnosProverError {
    #[error("Invalid transaction type: {0}")]
    InvalidTransactionType(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error(transparent)]
    ProgramOutputError(#[from] ProgramOutputError),
    #[error(transparent)]
    // Boxed to reduce the size of Result on the stack (RunnerError is >128 bytes).
    RunnerError(#[from] Box<RunnerError>),
    #[error(transparent)]
    ProvingError(#[from] ProvingError),
    #[error(transparent)]
    OutputParseError(#[from] OsOutputError),
}

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

/// Output from a successful proving operation.
///
/// Contains the RPC-facing result plus internal metrics.
#[derive(Debug, Clone)]
pub struct VirtualSnosProverOutput {
    /// The proving result (proof, proof facts, and messages).
    pub result: ProveTransactionResult,
    /// Duration of OS execution.
    pub os_duration: Duration,
    /// Duration of proving.
    pub prove_duration: Duration,
    /// Total duration from start to finish.
    pub total_duration: Duration,
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
        let runner = RpcRunnerFactory::new(
            node_url,
            prover_config.chain_id.clone(),
            contract_class_manager,
            prover_config.runner_config.clone(),
        );
        Self { runner }
    }
}

impl<R: VirtualSnosRunner> VirtualSnosProver<R> {
    /// Creates a new VirtualSnosProver from a runner.
    ///
    /// This constructor allows using any runner implementation.
    #[allow(dead_code)]
    pub(crate) fn from_runner(runner: R) -> Self {
        Self { runner }
    }

    /// Proves a transaction on top of the specified block.
    ///
    /// This method:
    /// 1. Validates and extracts the invoke transaction.
    /// 2. Calculates the transaction hash.
    /// 3. Runs the Starknet OS.
    /// 4. Generates a proof.
    #[instrument(skip(self, transaction), fields(block_id = ?block_id, tx_hash))]
    pub async fn prove_transaction(
        &self,
        block_id: BlockId,
        transaction: RpcTransaction,
    ) -> Result<VirtualSnosProverOutput, VirtualSnosProverError> {
        let start_time = Instant::now();

        // Validate block_id is not pending.
        if matches!(block_id, BlockId::Pending) {
            return Err(VirtualSnosProverError::ValidationError(
                "Pending blocks are not supported; only finalized blocks can be proven."
                    .to_string(),
            ));
        }

        let invoke_tx = extract_invoke_tx(transaction)?;

        // Run OS and get output.
        let os_start = Instant::now();

        let txs = vec![invoke_tx];
        let runner_output = self
            .runner
            .run_virtual_os(block_id, txs)
            .await
            .map_err(|err| VirtualSnosProverError::RunnerError(Box::new(err)))?;

        let os_duration = os_start.elapsed();
        info!(
            os_duration_ms = %os_duration.as_millis(),
            "OS execution completed"
        );

        // Run the prover.
        let prove_start = Instant::now();
        let prover_output = prove(runner_output.cairo_pie).await?;
        let prove_duration = prove_start.elapsed();
        let total_duration = start_time.elapsed();

        info!(
            prove_duration_ms = %prove_duration.as_millis(),
            total_duration_ms = %total_duration.as_millis(),
            "Proving completed"
        );

        // Convert program output to proof facts using VIRTUAL_SNOS variant marker.
        let proof_facts = prover_output.program_output.try_into_proof_facts(VIRTUAL_SNOS)?;

        let result = ProveTransactionResult {
            proof: prover_output.proof,
            proof_facts,
            l2_to_l1_messages: runner_output.l2_to_l1_messages,
        };

        Ok(VirtualSnosProverOutput { result, os_duration, prove_duration, total_duration })
    }
}

/// Validates that the transaction is an Invoke transaction and extracts it.
pub fn extract_invoke_tx(tx: RpcTransaction) -> Result<InvokeTransaction, VirtualSnosProverError> {
    match tx {
        RpcTransaction::Invoke(RpcInvokeTransaction::V3(invoke_v3)) => {
            Ok(InvokeTransaction::V3(invoke_v3.into()))
        }
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
