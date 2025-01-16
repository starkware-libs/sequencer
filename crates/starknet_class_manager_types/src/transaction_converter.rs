use std::future::Future;

use starknet_api::consensus_transaction::{ConsensusTransaction, InternalConsensusTransaction};
use starknet_api::rpc_transaction::{InternalRpcTransaction, RpcTransaction};
use starknet_api::{executable_transaction, transaction};

use crate::SharedClassManagerClient;

pub trait TransactionConverterTrait {
    fn convert_internal_tx_to_consensus_tx(
        tx: InternalConsensusTransaction,
    ) -> ConsensusTransaction;

    fn convert_consensus_tx_to_internal_tx(
        tx: ConsensusTransaction,
    ) -> impl Future<Output = InternalConsensusTransaction> + Send;

    fn convert_internal_rpc_tx_to_rpc_tx(tx: InternalRpcTransaction) -> RpcTransaction;

    fn convert_rpc_tx_to_internal_rpc_tx(
        tx: RpcTransaction,
    ) -> impl Future<Output = InternalRpcTransaction> + Send;

    fn convert_internal_rpc_tx_to_executable_tx(
        tx: InternalRpcTransaction,
    ) -> executable_transaction::Transaction;
}

pub struct TransactionConverter {
    _class_manager_client: SharedClassManagerClient,
}

impl TransactionConverterTrait for TransactionConverter {
    fn convert_internal_tx_to_consensus_tx(
        _tx: InternalConsensusTransaction,
    ) -> ConsensusTransaction {
        todo!()
    }

    async fn convert_consensus_tx_to_internal_tx(
        _tx: ConsensusTransaction,
    ) -> InternalConsensusTransaction {
        todo!()
    }

    fn convert_internal_rpc_tx_to_rpc_tx(_tx: InternalRpcTransaction) -> RpcTransaction {
        todo!()
    }

    async fn convert_rpc_tx_to_internal_rpc_tx(_tx: RpcTransaction) -> InternalRpcTransaction {
        todo!()
    }

    fn convert_internal_rpc_tx_to_executable_tx(
        _tx: InternalRpcTransaction,
    ) -> executable_transaction::Transaction {
        todo!()
    }
}

// TODO(alonl): remove this once the conversion functions are implemented.
#[allow(dead_code)]
fn convert_consensus_l1_handler_to_internal_l1_handler(
    _tx: transaction::L1HandlerTransaction,
) -> executable_transaction::L1HandlerTransaction {
    todo!()
}

#[allow(dead_code)]
fn convert_internal_l1_handler_to_consensus_l1_handler(
    _tx: executable_transaction::L1HandlerTransaction,
) -> transaction::L1HandlerTransaction {
    todo!()
}
