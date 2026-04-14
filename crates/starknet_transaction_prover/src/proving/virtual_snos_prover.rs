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
use starknet_api::block::GasPrice;
use starknet_api::execution_resources::GasAmount;
use starknet_api::rpc_transaction::{RpcInvokeTransaction, RpcInvokeTransactionV3, RpcTransaction};
use starknet_api::transaction::fields::{Proof, ProofFacts, Tip};
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
        #[cfg(feature = "stwo_proving")]
        let precomputes = prepare_recursive_prover_precomputes()
            .expect("Failed to prepare recursive prover precomputes");
        Self {
            runner,
            validate_zero_fee_fields: prover_config.validate_zero_fee_fields,
            #[cfg(feature = "stwo_proving")]
            precomputes,
        }
    }
}

impl<R: VirtualSnosRunner> VirtualSnosProver<R> {
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
        let result = self.prove_virtual_snos_run(runner_output).await?;

        info!(
            prove_duration_ms = %prove_start.elapsed().as_millis(),
            "Proving completed"
        );

        info!(total_duration_ms = %start_time.elapsed().as_millis(), "prove_transaction completed");
        Ok(result)
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
        RpcTransaction::Declare(_) => Err(VirtualSnosProverError::InvalidTransactionInput(
            "Declare transactions are not supported; only Invoke transactions are allowed"
                .to_string(),
        )),
        RpcTransaction::DeployAccount(_) => Err(VirtualSnosProverError::InvalidTransactionInput(
            "DeployAccount transactions are not supported; only Invoke transactions are allowed"
                .to_string(),
        )),
    }
}

/// Validates that the transaction input fields are acceptable for proving.
///
/// Rejects transactions where:
/// - `proof` or `proof_facts` are non-empty (these are output-only fields).
/// - Any `max_price_per_unit` is non-zero or `tip` is non-zero (when `validate_zero_fee_fields` is
///   enabled). Proving is client-side so no fees are charged.
/// - `l2_gas.max_amount` is zero — this is the gas limit enforced by the OS.
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
        validate_zero_fee_resource_bounds(tx)?;
    }
    Ok(())
}

/// Validates resource bounds for proving, collecting all violations into a single error.
///
/// Since proving is client-side, no fees are charged. All `max_price_per_unit` fields and `tip`
/// must be zero. The `max_amount` fields have different semantics:
/// - `l2_gas.max_amount`: determines the gas limit the OS enforces on the transaction. Must be
///   non-zero. Set this to the value returned by `starknet_estimateFee`, or use a safe upper bound
///   like 100,000,000 (sufficient for ~1 million Cairo steps).
/// - `l1_gas.max_amount` and `l1_data_gas.max_amount`: do not affect OS execution and can be any
///   value.
fn validate_zero_fee_resource_bounds(
    tx: &RpcInvokeTransactionV3,
) -> Result<(), VirtualSnosProverError> {
    let bounds = &tx.resource_bounds;
    let mut violations = Vec::new();

    if bounds.l1_gas.max_price_per_unit != GasPrice(0) {
        violations
            .push(format!("l1_gas.max_price_per_unit = {}", bounds.l1_gas.max_price_per_unit.0));
    }
    if bounds.l2_gas.max_price_per_unit != GasPrice(0) {
        violations
            .push(format!("l2_gas.max_price_per_unit = {}", bounds.l2_gas.max_price_per_unit.0));
    }
    if bounds.l1_data_gas.max_price_per_unit != GasPrice(0) {
        violations.push(format!(
            "l1_data_gas.max_price_per_unit = {}",
            bounds.l1_data_gas.max_price_per_unit.0
        ));
    }
    if tx.tip != Tip(0) {
        violations.push(format!("tip = {}", tx.tip.0));
    }

    if !violations.is_empty() {
        return Err(VirtualSnosProverError::InvalidTransactionInput(format!(
            "Proving is client-side — no fees are charged. The following fields must be zero but \
             were not: [{}]. Set all max_price_per_unit fields and tip to 0x0. Note: max_amount \
             fields are fine to set — l2_gas.max_amount controls the gas limit enforced by the OS \
             (use the value from starknet_estimateFee, or 100000000 as a safe upper bound). \
             l1_gas.max_amount and l1_data_gas.max_amount do not affect OS execution.",
            violations.join(", ")
        )));
    }

    if bounds.l2_gas.max_amount == GasAmount(0) {
        return Err(VirtualSnosProverError::InvalidTransactionInput(
            "l2_gas.max_amount must be non-zero — it is the gas limit enforced by the OS on the \
             transaction. Set this to the value returned by starknet_estimateFee, or use \
             100000000 (0x5f5e100) as a safe upper bound (sufficient for ~1 million Cairo steps)."
                .to_string(),
        ));
    }

    Ok(())
}
