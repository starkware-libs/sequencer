use async_trait::async_trait;
use starknet_api::consensus_transaction::{ConsensusTransaction, InternalConsensusTransaction};
use starknet_api::rpc_transaction::{InternalRpcTransaction, RpcTransaction};
use starknet_api::{executable_transaction, transaction};

use crate::{ClassManagerClientResult, SharedClassManagerClient};

#[async_trait]
pub trait TransactionConverterTrait {
    async fn convert_internal_tx_to_consensus_tx(
        &self,
        tx: InternalConsensusTransaction,
    ) -> ClassManagerClientResult<ConsensusTransaction>;

    async fn convert_consensus_tx_to_internal_tx(
        &self,
        tx: ConsensusTransaction,
    ) -> ClassManagerClientResult<InternalConsensusTransaction>;

    async fn convert_internal_rpc_tx_to_rpc_tx(
        &self,
        tx: InternalRpcTransaction,
    ) -> ClassManagerClientResult<RpcTransaction>;

    async fn convert_rpc_tx_to_internal_rpc_tx(
        &self,
        tx: RpcTransaction,
    ) -> ClassManagerClientResult<InternalRpcTransaction>;

    async fn convert_internal_rpc_tx_to_executable_tx(
        &self,
        tx: InternalRpcTransaction,
    ) -> ClassManagerClientResult<executable_transaction::Transaction>;
}

pub struct TransactionConverter {
    _class_manager_client: SharedClassManagerClient,
}

#[async_trait]
impl TransactionConverterTrait for TransactionConverter {
    async fn convert_internal_tx_to_consensus_tx(
        &self,
        _tx: InternalConsensusTransaction,
    ) -> ClassManagerClientResult<ConsensusTransaction> {
        todo!()
    }

    async fn convert_consensus_tx_to_internal_tx(
        &self,
        _tx: ConsensusTransaction,
    ) -> ClassManagerClientResult<InternalConsensusTransaction> {
        todo!()
    }

    async fn convert_internal_rpc_tx_to_rpc_tx(
        &self,
        _tx: InternalRpcTransaction,
    ) -> ClassManagerClientResult<RpcTransaction> {
        todo!()
    }

    async fn convert_rpc_tx_to_internal_rpc_tx(
        &self,
        _tx: RpcTransaction,
    ) -> ClassManagerClientResult<InternalRpcTransaction> {
        todo!()
    }

    async fn convert_internal_rpc_tx_to_executable_tx(
        &self,
        _tx: InternalRpcTransaction,
    ) -> ClassManagerClientResult<executable_transaction::Transaction> {
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
