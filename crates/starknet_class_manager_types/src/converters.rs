use blockifier::transaction::transaction_execution;
use starknet_api::internal_transaction::{InternalRpcTransaction, InternalTransaction};
use starknet_api::rpc_transaction::{
    ExternalTransaction,
    RpcDeclareTransaction,
    RpcDeployAccountTransaction,
    RpcInvokeTransaction,
    RpcTransaction,
};

use crate::SharedClassManagerClient;

pub fn internal_transaction_to_external_transaction(
    tx: InternalTransaction,
    class_manager_client: &SharedClassManagerClient,
) -> ExternalTransaction {
    match tx {
        InternalTransaction::RpcTransaction(internal_rpc_transaction) => {
            ExternalTransaction::RpcTransaction(internal_rpc_to_rpc(
                internal_rpc_transaction,
                class_manager_client,
            ))
        }
        InternalTransaction::L1Handler(l1_handler) => ExternalTransaction::L1Handler(l1_handler),
    }
}

pub fn external_transaction_to_internal_transaction(
    tx: ExternalTransaction,
) -> InternalTransaction {
    match tx {
        ExternalTransaction::RpcTransaction(rpc_transaction) => {
            InternalTransaction::RpcTransaction(rpc_to_internal_rpc(rpc_transaction))
        }
        ExternalTransaction::L1Handler(l1_handler) => InternalTransaction::L1Handler(l1_handler),
    }
}

// The transaction returned here implements the trait ExecutableTransaction (defined in the batcher)
pub fn internal_rpc_to_executable_transaction(
    _tx: InternalRpcTransaction,
    _class_manager_client: &SharedClassManagerClient,
) -> transaction_execution::Transaction {
    unimplemented!()
}

pub fn rpc_to_internal_rpc(tx: RpcTransaction) -> InternalRpcTransaction {
    match tx {
        RpcTransaction::Declare(RpcDeclareTransaction::V3(declare)) => {
            InternalRpcTransaction::Declare(declare.into())
        }
        RpcTransaction::Invoke(RpcInvokeTransaction::V3(invoke)) => {
            InternalRpcTransaction::Invoke(invoke.into())
        }
        RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(deploy_account)) => {
            InternalRpcTransaction::DeployAccount(deploy_account.into())
        }
    }
}

pub fn internal_rpc_to_rpc(
    _tx: InternalRpcTransaction,
    _class_manager_client: &SharedClassManagerClient,
) -> RpcTransaction {
    unimplemented!()
}
