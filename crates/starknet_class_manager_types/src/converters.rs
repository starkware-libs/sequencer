use blockifier::transaction::transaction_execution::{self};
use starknet_api::core::ClassHash;
use starknet_api::rpc_transaction::{
    InternalRpcTransaction,
    InternalRpcTransactionWithoutHash,
    RpcDeclareTransaction,
    RpcDeployAccountTransaction,
    RpcInvokeTransaction,
    RpcTransaction,
};
use starknet_api::transaction::TransactionHash;
use starknet_api::transaction_v3::{ExternalTransactionV3, InternalTransactionV3};

use crate::SharedClassManagerClient;

pub fn convert_internal_transaction_to_external_transaction(
    tx: InternalTransactionV3,
    class_manager_client: &SharedClassManagerClient,
) -> ExternalTransactionV3 {
    match tx {
        InternalTransactionV3::RpcTransaction(internal_rpc_transaction) => {
            ExternalTransactionV3::RpcTransaction(convert_internal_rpc_to_rpc(
                internal_rpc_transaction,
                class_manager_client,
            ))
        }
        InternalTransactionV3::L1Handler(l1_handler) => {
            ExternalTransactionV3::L1Handler(l1_handler)
        }
    }
}

pub async fn convert_external_transaction_to_internal_transaction(
    tx: ExternalTransactionV3,
    class_manager_client: &SharedClassManagerClient,
) -> InternalTransactionV3 {
    match tx {
        ExternalTransactionV3::RpcTransaction(rpc_transaction) => {
            InternalTransactionV3::RpcTransaction(
                convert_rpc_to_internal_rpc(rpc_transaction, class_manager_client).await,
            )
        }
        ExternalTransactionV3::L1Handler(l1_handler) => {
            InternalTransactionV3::L1Handler(l1_handler)
        }
    }
}

// The transaction returned here implements the trait ExecutableTransaction (defined in the batcher)
pub fn convert_internal_rpc_to_executable_transaction(
    _tx: InternalRpcTransaction,
    _class_manager_client: &SharedClassManagerClient,
) -> transaction_execution::Transaction {
    unimplemented!()
}

pub async fn convert_rpc_to_internal_rpc(
    tx: RpcTransaction,
    class_manager_client: &SharedClassManagerClient,
) -> InternalRpcTransaction {
    let internal_rpc_tx_without_hash = match tx {
        RpcTransaction::Declare(RpcDeclareTransaction::V3(declare)) => {
            class_manager_client
                .add_class(ClassHash(declare.compiled_class_hash.0), declare.contract_class.clone())
                .await
                .expect("Failed to add class");
            InternalRpcTransactionWithoutHash::Declare(declare.into())
        }
        RpcTransaction::Invoke(RpcInvokeTransaction::V3(invoke)) => {
            InternalRpcTransactionWithoutHash::Invoke(invoke.into())
        }
        RpcTransaction::DeployAccount(RpcDeployAccountTransaction::V3(deploy_account)) => {
            InternalRpcTransactionWithoutHash::DeployAccount(deploy_account.into())
        }
    };
    // TODO: calculate the hash
    InternalRpcTransaction { tx: internal_rpc_tx_without_hash, tx_hash: TransactionHash::default() }
}

pub fn convert_internal_rpc_to_rpc(
    _tx: InternalRpcTransaction,
    _class_manager_client: &SharedClassManagerClient,
) -> RpcTransaction {
    unimplemented!()
}
