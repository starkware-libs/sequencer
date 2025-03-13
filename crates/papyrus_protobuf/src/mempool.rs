use starknet_api::rpc_transaction::RpcTransaction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcTransactionWrapper(pub RpcTransaction);
pub struct RpcTransactionBatch(pub Vec<RpcTransaction>);
