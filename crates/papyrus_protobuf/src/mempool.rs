use starknet_api::rpc_transaction::RpcTransaction;

// TODO(alonl): remove this struct once we switch to RpcTransactionBatch in mempool p2p
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcTransactionWrapper(pub RpcTransaction);
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcTransactionBatch(pub Vec<RpcTransaction>);
