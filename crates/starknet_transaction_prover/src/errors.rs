use blockifier::state::errors::StateError;
use blockifier_reexecution::errors::ReexecutionError;
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

/// Errors that can occur during proving.
#[derive(Debug, Error)]
pub enum ProvingError {
    #[cfg(feature = "stwo_proving")]
    #[error("Prover execution failed: {0}")]
    ProverExecution(String),

    #[cfg(feature = "stwo_proving")]
    #[error("Proving task failed to join: {0}")]
    TaskJoin(#[source] tokio::task::JoinError),
}

/// Error type for the virtual SNOS prover.
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
