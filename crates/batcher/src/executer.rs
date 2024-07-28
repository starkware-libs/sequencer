// TODO(yair): remove this once the executer is in use.
#![allow(dead_code)]

#[cfg(test)]
use mockall::automock;

// Helper module to import the necessary types from the blockifier crate. Every type that is from
// the blockifier will be re-exported here and can be used in the rest of the code with the
// `blockifier_imports::` prefix.
mod blockifier_imports {
    pub use blockifier::blockifier::transaction_executor::{
        TransactionExecutor as GenericBlockifierTransactionExecutor,
        TransactionExecutorError,
    };
    pub use blockifier::transaction::objects::TransactionExecutionInfo;
    pub use blockifier::transaction::transaction_execution::Transaction;
    use starknet_gateway::rpc_state_reader::RpcStateReader;

    pub type TransactionExecutor = GenericBlockifierTransactionExecutor<RpcStateReader>;
}

/// Wrapper for the blockifier's execution functionality.
#[cfg_attr(test, automock)]
pub trait BlockifierTransactionExecutorTrait {
    fn execute_txs(
        &mut self,
        txs: &[blockifier_imports::Transaction],
    ) -> Vec<
        Result<
            blockifier_imports::TransactionExecutionInfo,
            blockifier_imports::TransactionExecutorError,
        >,
    >;
}

/// The actual type for our executer.
pub type Executer = GenericExecuter<blockifier_imports::TransactionExecutor>;

/// A generic executer for dependency injection.
pub struct GenericExecuter<E: BlockifierTransactionExecutorTrait> {
    pub executor_impl: E,
}
