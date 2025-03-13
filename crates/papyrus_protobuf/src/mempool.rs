use starknet_api::rpc_transaction::RpcTransaction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcTransactionBatch(pub Vec<RpcTransaction>);
