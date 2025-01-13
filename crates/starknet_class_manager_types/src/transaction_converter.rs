use std::future::Future;

use blockifier::transaction::transaction_execution::{self};
use starknet_api::consensus_transaction::{ConsensusTransaction, InternalConsensusTransaction};
use starknet_api::rpc_transaction::{InternalRpcTransaction, RpcTransaction};
use starknet_api::{executable_transaction, transaction};

use crate::SharedClassManagerClient;

pub trait TransactionConverterTrait {
    fn convert_internal_tx_to_consensus_transaction(
        tx: InternalConsensusTransaction,
    ) -> ConsensusTransaction;

    fn convert_consensus_tx_to_internal_tx(
        tx: ConsensusTransaction,
    ) -> impl Future<Output = InternalConsensusTransaction> + Send;

    fn convert_internal_rpc_to_rpc(tx: InternalRpcTransaction) -> RpcTransaction;

    fn convert_rpc_to_internal_rpc(
        tx: RpcTransaction,
    ) -> impl Future<Output = InternalRpcTransaction> + Send;

    fn convert_internal_l1_handler_to_consensus_l1_handler(
        tx: executable_transaction::L1HandlerTransaction,
    ) -> transaction::L1HandlerTransaction;

    fn convert_consensus_l1_handler_to_internal_l1_handler(
        tx: transaction::L1HandlerTransaction,
    ) -> executable_transaction::L1HandlerTransaction;

    fn convert_internal_rpc_to_executable_tx(
        tx: InternalRpcTransaction,
    ) -> transaction_execution::Transaction;
}

pub struct TransactionConverter {
    _class_manager_client: SharedClassManagerClient,
}

impl TransactionConverterTrait for TransactionConverter {
    fn convert_internal_tx_to_consensus_transaction(
        _tx: InternalConsensusTransaction,
    ) -> ConsensusTransaction {
        todo!()
    }

    async fn convert_consensus_tx_to_internal_tx(
        _tx: ConsensusTransaction,
    ) -> InternalConsensusTransaction {
        todo!()
    }

    fn convert_internal_rpc_to_rpc(_tx: InternalRpcTransaction) -> RpcTransaction {
        todo!()
    }

    async fn convert_rpc_to_internal_rpc(_tx: RpcTransaction) -> InternalRpcTransaction {
        todo!()
    }

    fn convert_internal_l1_handler_to_consensus_l1_handler(
        _tx: executable_transaction::L1HandlerTransaction,
    ) -> transaction::L1HandlerTransaction {
        todo!()
    }

    fn convert_consensus_l1_handler_to_internal_l1_handler(
        _tx: transaction::L1HandlerTransaction,
    ) -> executable_transaction::L1HandlerTransaction {
        todo!()
    }

    fn convert_internal_rpc_to_executable_tx(
        _tx: InternalRpcTransaction,
    ) -> transaction_execution::Transaction {
        todo!()
    }
}
