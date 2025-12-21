use blockifier_reexecution::errors::ReexecutionError;
use starknet_rust::providers::ProviderError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VirtualBlockExecutorError {
    #[error(transparent)]
    // Boxed to reduce the size of Result on the stack (ReexecutionError is >128 bytes).
    ReexecutionError(#[from] Box<ReexecutionError>),

    #[error("Transaction execution failed: {0}")]
    TransactionExecutionError(String),

    #[error("Block state unavailable after execution")]
    StateUnavailable,
}

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error(transparent)]
    ClassesProvider(#[from] ClassesProviderError),
    #[error(transparent)]
    ProofProvider(#[from] ProofProviderError),
    #[error(transparent)]
    VirtualBlockExecutor(#[from] VirtualBlockExecutorError),
    #[error("OS Input generation failed: {0}")]
    InputGenerationError(String),
}

#[derive(Debug, Error)]
pub enum ProofProviderError {
    #[error("RPC provider error: {0}")]
    Rpc(#[from] ProviderError),
}

#[derive(Debug, Error)]
pub enum ClassesProviderError {
    #[error("Failed to get classes: {0}")]
    GetClassesError(String),
}
