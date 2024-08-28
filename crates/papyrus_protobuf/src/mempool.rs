#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcTransactionWrapper<RpcTransaction>(pub RpcTransaction);
