//! Virtual SNOS prover for generating transaction proofs.
//!
//! This module contains the core proving logic, extracted from the HTTP layer
//! to enable better separation of concerns and testability.

#[cfg(feature = "stwo_proving")]
use std::sync::Arc;
use std::time::Instant;

use blockifier::state::contract_class_manager::ContractClassManager;
use blockifier_reexecution::state_reader::rpc_objects::BlockId;
use blockifier_reexecution::utils::get_chain_info;
#[cfg(feature = "stwo_proving")]
use privacy_prove::{prepare_recursive_prover_precomputes, RecursiveProverPrecomputes};
use serde::{Deserialize, Serialize};
use starknet_api::rpc_transaction::{RpcInvokeTransaction, RpcInvokeTransactionV3, RpcTransaction};
use starknet_api::transaction::fields::{Fee, Proof, ProofFacts, ValidResourceBounds};
use starknet_api::transaction::{InvokeTransaction, MessageToL1};
use tracing::{info, instrument};
use url::Url;

use crate::blocking_check::{BlockingCheckClient, BlockingCheckResult};
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
    /// Optional client for the external blocking check service.
    blocking_check_client: Option<BlockingCheckClient>,
    /// Precomputed data for the recursive prover, prepared once at startup.
    #[cfg(feature = "stwo_proving")]
    precomputes: Arc<RecursiveProverPrecomputes>,
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
        let blocking_check_client = prover_config.blocking_check_url.as_ref().map(|url_str| {
            let url = Url::parse(url_str).expect("Invalid blocking_check_url in config");
            BlockingCheckClient::new(
                url,
                prover_config.blocking_check_timeout_millis,
                prover_config.blocking_check_fail_open,
            )
        });
        #[cfg(feature = "stwo_proving")]
        let precomputes = prepare_recursive_prover_precomputes()
            .expect("Failed to prepare recursive prover precomputes");
        Self {
            runner,
            validate_zero_fee_fields: prover_config.validate_zero_fee_fields,
            blocking_check_client,
            #[cfg(feature = "stwo_proving")]
            precomputes,
        }
    }
}

impl<R: VirtualSnosRunner + 'static> VirtualSnosProver<R> {
    /// Creates a new VirtualSnosProver from a runner.
    ///
    /// This constructor allows using any runner implementation.
    #[allow(dead_code)]
    pub(crate) fn from_runner(runner: R) -> Self {
        #[cfg(feature = "stwo_proving")]
        let precomputes = prepare_recursive_prover_precomputes()
            .expect("Failed to prepare recursive prover precomputes");
        Self {
            runner,
            validate_zero_fee_fields: true,
            blocking_check_client: None,
            #[cfg(feature = "stwo_proving")]
            precomputes,
        }
    }

    /// Creates a new VirtualSnosProver from a runner with an optional blocking check client.
    #[allow(dead_code)]
    pub(crate) fn from_runner_with_blocking_check(
        runner: R,
        blocking_check_client: Option<BlockingCheckClient>,
    ) -> Self {
        #[cfg(feature = "stwo_proving")]
        let precomputes = prepare_recursive_prover_precomputes()
            .expect("Failed to prepare recursive prover precomputes");
        Self {
            runner,
            validate_zero_fee_fields: true,
            blocking_check_client,
            #[cfg(feature = "stwo_proving")]
            precomputes,
        }
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
    /// 2. Optionally awaits an external blocking check (with timeout) before proving.
    /// 3. Runs the Starknet OS and generates a proof via `prove_virtual_snos_run`.
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

        let invoke_v3 = extract_rpc_invoke_tx(transaction.clone())?;
        validate_transaction_input(&invoke_v3, self.validate_zero_fee_fields)?;
        let invoke_tx = InvokeTransaction::V3(invoke_v3.into());

        let result = match &self.blocking_check_client {
            None => self.run_and_prove(block_id, vec![invoke_tx]).await?,
            Some(client) => {
                self.prove_with_blocking_check(client, block_id, transaction, invoke_tx).await?
            }
        };

        info!(total_duration_ms = %start_time.elapsed().as_millis(), "prove_transaction completed");
        Ok(result)
    }

    /// Runs the OS and generates a proof. This is the core proving pipeline.
    async fn run_and_prove(
        &self,
        block_id: BlockId,
        txs: Vec<InvokeTransaction>,
    ) -> Result<ProveTransactionResult, VirtualSnosProverError> {
        let os_start = Instant::now();
        let runner_output = self
            .runner
            .run_virtual_os(block_id, txs)
            .await
            .map_err(|err| VirtualSnosProverError::RunnerError(Box::new(err)))?;

        info!(
            os_duration_ms = %os_start.elapsed().as_millis(),
            "OS execution completed"
        );

        let prove_start = Instant::now();
        let result = self.prove_virtual_snos_run(runner_output).await?;

        info!(
            prove_duration_ms = %prove_start.elapsed().as_millis(),
            "Proving completed"
        );

        Ok(result)
    }

    /// Starts proving in the background, then awaits the external blocking check (bounded by
    /// `client.timeout_millis`) and returns the proof only if the check allows it.
    ///
    /// Proving is spawned *before* the check request is sent so that the two run in parallel.
    ///
    /// Behavior:
    /// - `Blocked`: abort the background proof and return `TransactionBlocked`.
    /// - `Allowed`: await the background proof and return its result.
    /// - `Inconclusive` or timeout: apply the configured fail-open/fail-close policy.
    ///
    /// Trade-off: when the blocking check service is slow (e.g. elliptic is slow, network
    /// issues, or the elliptic proxy is still starting up), this flow may deny transactions
    /// that would otherwise be allowed — the check result might have arrived during the time
    /// it takes to compute the proof, but we decide solely on the check timeout.
    async fn prove_with_blocking_check(
        &self,
        client: &BlockingCheckClient,
        block_id: BlockId,
        transaction: RpcTransaction,
        invoke_tx: InvokeTransaction,
    ) -> Result<ProveTransactionResult, VirtualSnosProverError> {
        // Kick off proving in parallel with the check. Clone is cheap: inner fields are
        // Arcs or small configs.
        let prover = self.clone();
        let prove_handle =
            tokio::spawn(async move { prover.run_and_prove(block_id, vec![invoke_tx]).await });

        let timeout_duration = std::time::Duration::from_millis(client.timeout_millis);
        let check_outcome =
            tokio::time::timeout(timeout_duration, client.check_transaction(block_id, transaction))
                .await;

        let allow = match check_outcome {
            Ok(BlockingCheckResult::Blocked) => {
                info!("Transaction blocked by external check");
                false
            }
            Ok(BlockingCheckResult::Allowed) => {
                info!("Transaction allowed by external check");
                true
            }
            Ok(BlockingCheckResult::Inconclusive) => {
                info!(fail_open = client.fail_open, "Blocking check inconclusive");
                client.fail_open
            }
            Err(_) => {
                info!(fail_open = client.fail_open, "Blocking check timed out");
                client.fail_open
            }
        };

        if !allow {
            prove_handle.abort();
            return Err(VirtualSnosProverError::TransactionBlocked);
        }

        match prove_handle.await {
            Ok(result) => result,
            Err(err) if err.is_panic() => std::panic::resume_unwind(err.into_panic()),
            Err(err) => unreachable!("prove task cancelled unexpectedly: {err}"),
        }
    }

    /// Proves a Virtual Starknet OS run from its output.
    ///
    /// Generates a proof from the given [`RunnerOutput`] and converts the program output into
    /// proof facts.
    #[cfg(not(feature = "stwo_proving"))]
    async fn prove_virtual_snos_run(
        &self,
        _runner_output: RunnerOutput,
    ) -> Result<ProveTransactionResult, VirtualSnosProverError> {
        unimplemented!(
            "In-memory proving requires the `stwo_proving` feature flag and a nightly Rust \
             toolchain."
        );
    }

    /// Proves a Virtual Starknet OS run from its output.
    ///
    /// Generates a proof from the given [`RunnerOutput`] and converts the program output into
    /// proof facts.
    #[cfg(feature = "stwo_proving")]
    async fn prove_virtual_snos_run(
        &self,
        runner_output: RunnerOutput,
    ) -> Result<ProveTransactionResult, VirtualSnosProverError> {
        use starknet_api::transaction::fields::VIRTUAL_SNOS;

        use crate::proving::prover::prove;

        let prover_output = prove(runner_output.cairo_pie, self.precomputes.clone()).await?;
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
