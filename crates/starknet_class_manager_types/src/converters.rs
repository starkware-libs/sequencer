use blockifier::transaction::transaction_execution::{self};
use starknet_api::rpc_transaction::{InternalRpcTransaction, RpcTransaction};
use starknet_api::transaction_v3::{ExternalTransactionV3, InternalTransactionV3};
use starknet_api::{executable_transaction, transaction};

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
        InternalTransactionV3::L1Handler(l1_handler) => ExternalTransactionV3::L1Handler(
            convert_internal_l1_handler_to_external_l1_handler(l1_handler),
        ),
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
        ExternalTransactionV3::L1Handler(l1_handler) => InternalTransactionV3::L1Handler(
            convert_external_l1_handler_to_internal_l1_handler(l1_handler),
        ),
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
    _tx: RpcTransaction,
    _class_manager_client: &SharedClassManagerClient,
) -> InternalRpcTransaction {
    unimplemented!()
}

pub fn convert_internal_rpc_to_rpc(
    _tx: InternalRpcTransaction,
    _class_manager_client: &SharedClassManagerClient,
) -> RpcTransaction {
    unimplemented!()
}

fn convert_external_l1_handler_to_internal_l1_handler(
    _tx: transaction::L1HandlerTransaction,
) -> executable_transaction::L1HandlerTransaction {
    unimplemented!()
}

fn convert_internal_l1_handler_to_external_l1_handler(
    _tx: executable_transaction::L1HandlerTransaction,
) -> transaction::L1HandlerTransaction {
    unimplemented!()
}
