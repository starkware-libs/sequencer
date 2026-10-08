use blockifier::state::errors::StateError;
use blockifier_reexecution::errors::ReexecutionError;
#[cfg(feature = "stwo_proving")]
use cairo_vm::types::builtin_name::BuiltinName;
use cairo_vm::types::errors::program_errors::ProgramError;
use starknet_api::core::ClassHash;
use starknet_api::transaction::TransactionHash;
use starknet_os::errors::StarknetOsError;
use starknet_os::io::os_output::OsOutputError;
use starknet_patricia_storage::errors::SerializationError;
use starknet_proof_verifier::ProgramOutputError;
use starknet_rust::providers::jsonrpc::{HttpTransportError, JsonRpcClientError};
use starknet_rust::providers::ProviderError;
use thiserror::Error;

use crate::server::metrics::outcomes;

#[cfg(test)]
#[path = "errors_test.rs"]
mod errors_test;

#[derive(Debug, Error)]
pub enum VirtualBlockExecutorError {
    #[error(transparent)]
    // Boxed to reduce the size of Result on the stack (ReexecutionError is >128 bytes).
    ReexecutionError(#[from] Box<ReexecutionError>),
    #[error("Transaction execution failed: {0}")]
    TransactionExecutionError(String),
    #[error("Reverted transactions are not supported; hash: {0:?}, revert reason: {1}")]
    TransactionReverted(TransactionHash, String),
    #[error("Block state unavailable after execution")]
    StateUnavailable,
    #[error("Failed to acquire bouncer lock: {0}")]
    BouncerLockError(String),
}

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error(transparent)]
    ClassesProvider(#[from] ClassesProviderError),
    #[error(transparent)]
    ProofProvider(#[from] ProofProviderError),
    #[error(transparent)]
    VirtualBlockExecutor(#[from] VirtualBlockExecutorError),
    #[error(transparent)]
    OsExecution(#[from] StarknetOsError),
    #[error("OS Input generation failed: {0}")]
    InputGenerationError(String),
    #[error("Failed to calculate transaction hash: {0}")]
    TransactionHashError(String),
    #[error(transparent)]
    TaskJoin(#[from] tokio::task::JoinError),
}

impl RunnerError {
    /// Returns the fixed log reason naming the runner step that failed. Unlike the error text, it
    /// carries no transaction data or node URL, so it is safe to log. Proof provider failures also
    /// name the provider error, so a failed RPC call reads differently from a malformed proof
    /// response.
    pub fn log_reason(&self) -> &'static str {
        match self {
            RunnerError::ClassesProvider(_) => "classes_provider",
            RunnerError::ProofProvider(proof_provider_error) => match proof_provider_error {
                ProofProviderError::InvalidStateDiff(_) => "proof_provider_invalid_state_diff",
                ProofProviderError::Rpc(_) => "proof_provider_rpc",
                ProofProviderError::UpstreamRpcError { .. } => "proof_provider_upstream_rpc",
                ProofProviderError::SerializationError(_) => "proof_provider_serialization",
                ProofProviderError::InvalidProofResponse(_) => {
                    "proof_provider_invalid_proof_response"
                }
                ProofProviderError::BlockCommitmentError(_) => "proof_provider_block_commitment",
            },
            RunnerError::VirtualBlockExecutor(_) => "virtual_block_executor",
            RunnerError::OsExecution(_) => "os_execution",
            RunnerError::InputGenerationError(_) => "input_generation",
            RunnerError::TransactionHashError(_) => "transaction_hash",
            RunnerError::TaskJoin(_) => "task_join",
        }
    }
}

#[derive(Debug, Error)]
pub enum ProofProviderError {
    #[error("Invalid state diff: {0}")]
    InvalidStateDiff(String),
    #[error("RPC provider error: {0}")]
    Rpc(ProviderError),
    #[error("Upstream JSON-RPC error (code {code}): {message}")]
    UpstreamRpcError { code: i64, message: String, data: Option<serde_json::Value> },
    #[error(transparent)]
    SerializationError(#[from] SerializationError),
    #[error("Invalid RPC proof response: {0}")]
    InvalidProofResponse(String),
    #[error("Block commitment error: {0}")]
    BlockCommitmentError(String),
}

/// If the provider error wraps a raw JSON-RPC error (unrecognized by starknet-rs), surface it as
/// [`ProofProviderError::UpstreamRpcError`] so the original code and message are forwarded.
/// Otherwise fall back to [`ProofProviderError::Rpc`].
impl From<ProviderError> for ProofProviderError {
    fn from(err: ProviderError) -> Self {
        if let ProviderError::Other(ref inner) = err {
            if let Some(JsonRpcClientError::JsonRpcError(rpc_err)) =
                inner.as_any().downcast_ref::<JsonRpcClientError<HttpTransportError>>()
            {
                return ProofProviderError::UpstreamRpcError {
                    code: rpc_err.code,
                    message: rpc_err.message.clone(),
                    data: rpc_err.data.clone(),
                };
            }
        }
        ProofProviderError::Rpc(err)
    }
}

#[derive(Debug, Error)]
pub enum ClassesProviderError {
    #[error("Failed to get classes: {0}")]
    GetClassesError(String),
    #[error(
        "Starknet os does not support deprecated contract classes, class hash: {0} is deprecated"
    )]
    DeprecatedContractError(ClassHash),
    #[error("Unexpected error: bytecode of a class contained a non-integer value")]
    InvalidBytecodeElement,
    #[error(transparent)]
    StateError(#[from] StateError),
    #[error(transparent)]
    HintsConversionError(#[from] ProgramError),
}

/// Renders builtin usage for [`ProvingError::UnsupportedBuiltins`], e.g.
/// `add_mod (2 instances), mul_mod (4 instances)`.
#[cfg(feature = "stwo_proving")]
fn format_builtin_usage(builtin_usage: &[(BuiltinName, usize)]) -> String {
    builtin_usage
        .iter()
        .map(|(builtin_name, n_instances)| {
            format!("{} ({n_instances} instances)", builtin_name.to_str())
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Errors that can occur during proving.
#[derive(Debug, Error)]
pub enum ProvingError {
    #[cfg(feature = "stwo_proving")]
    #[error("Prover execution failed: {0}")]
    ProverExecution(String),

    #[cfg(feature = "stwo_proving")]
    #[error("Proving task failed to join: {0}")]
    TaskJoin(#[source] tokio::task::JoinError),

    /// The Cairo PIE uses builtins that the prover cannot prove, listed with their positive
    /// instance counts.
    #[cfg(feature = "stwo_proving")]
    #[error(
        "Transaction uses builtins the prover does not support: {}",
        format_builtin_usage(unsupported_builtins)
    )]
    UnsupportedBuiltins { unsupported_builtins: Vec<(BuiltinName, usize)> },
}

/// Error type for the virtual SNOS prover.
///
/// Never log an error's `Display` text. These failures reach the operator's log aggregator, and
/// every payload-carrying variant can embed data derived from the client's transaction:
/// `InvalidTransactionInput` quotes the fee inputs, a reverted transaction carries its hash and
/// revert reason, runner, output-parse and proving errors can quote transaction-derived program
/// output, and a transport error renders the node URL with any credentials in it. Log
/// [`Self::metric_outcome`] instead, which is a bounded label set.
#[derive(Debug, Error)]
pub enum VirtualSnosProverError {
    #[error("Invalid transaction type: {0}")]
    InvalidTransactionType(String),
    #[error("Invalid transaction input: {0}")]
    InvalidTransactionInput(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error(transparent)]
    ProgramOutputError(#[from] ProgramOutputError),
    #[error(transparent)]
    // Boxed to reduce the size of Result on the stack (RunnerError is >128 bytes).
    RunnerError(#[from] Box<RunnerError>),
    #[cfg(feature = "stwo_proving")]
    #[error(transparent)]
    ProvingError(#[from] ProvingError),
    #[error(transparent)]
    OutputParseError(#[from] OsOutputError),
    #[error("Transaction blocked by external check")]
    TransactionBlocked,
    /// The blocking check timed out or was inconclusive, and the fail-close policy rejected the
    /// transaction.
    #[error("Blocking check unavailable")]
    BlockingCheckUnavailable,
}

impl VirtualSnosProverError {
    /// Returns the fixed metric label for this error category.
    pub fn metric_outcome(&self) -> &'static str {
        match self {
            VirtualSnosProverError::InvalidTransactionType(_)
            | VirtualSnosProverError::InvalidTransactionInput(_)
            | VirtualSnosProverError::ValidationError(_) => outcomes::FAILURE_VALIDATION,
            VirtualSnosProverError::TransactionBlocked => outcomes::FAILURE_BLOCKED,
            VirtualSnosProverError::BlockingCheckUnavailable => {
                outcomes::FAILURE_BLOCKING_CHECK_UNAVAILABLE
            }
            VirtualSnosProverError::RunnerError(_) => outcomes::FAILURE_RUNNER,
            VirtualSnosProverError::OutputParseError(_)
            | VirtualSnosProverError::ProgramOutputError(_) => outcomes::FAILURE_OUTPUT_PARSE,
            // Names every ProvingError variant, so adding one fails to compile until it is given
            // an outcome label. A client-caused proving error must not count as a server-side
            // proving failure.
            #[cfg(feature = "stwo_proving")]
            VirtualSnosProverError::ProvingError(
                ProvingError::ProverExecution(_) | ProvingError::TaskJoin(_),
            ) => outcomes::FAILURE_PROVING,
            // The client receives error 1002, because the transaction uses a builtin the prover
            // cannot prove.
            #[cfg(feature = "stwo_proving")]
            VirtualSnosProverError::ProvingError(ProvingError::UnsupportedBuiltins { .. }) => {
                outcomes::FAILURE_UNSUPPORTED_BUILTIN
            }
        }
    }
}

/// Errors that can occur during configuration.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Configuration file error: {0}")]
    ConfigFileError(String),
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    #[error("Missing required field: {0}")]
    MissingRequiredField(String),
    #[error("Incomplete TLS configuration: {0}")]
    IncompleteTlsConfig(String),
}
