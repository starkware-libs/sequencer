use async_trait::async_trait;
use starknet_api::consensus_transaction::{ConsensusTransaction, InternalConsensusTransaction};
use starknet_api::executable_transaction::AccountTransaction;
use starknet_api::rpc_transaction::{InternalRpcTransaction, RpcTransaction};
use starknet_api::{executable_transaction, transaction, StarknetApiError};
use thiserror::Error;

use crate::{ClassManagerClientError, SharedClassManagerClient};

#[derive(Error, Debug, Clone)]
pub enum TransactionConverterError {
    #[error(transparent)]
    ClassManagerClientError(#[from] ClassManagerClientError),
    #[error(transparent)]
    StarknetApiError(#[from] StarknetApiError),
}

pub type TransactionConverterResult<T> = Result<T, TransactionConverterError>;

#[async_trait]
pub trait TransactionConverterTrait {
    async fn convert_internal_consensus_tx_to_consensus_tx(
        &self,
        tx: InternalConsensusTransaction,
    ) -> TransactionConverterResult<ConsensusTransaction>;

    async fn convert_consensus_tx_to_internal_consensus_tx(
        &self,
        tx: ConsensusTransaction,
    ) -> TransactionConverterResult<InternalConsensusTransaction>;

    async fn convert_internal_rpc_tx_to_rpc_tx(
        &self,
        tx: InternalRpcTransaction,
    ) -> TransactionConverterResult<RpcTransaction>;

    async fn convert_rpc_tx_to_internal_rpc_tx(
        &self,
        tx: RpcTransaction,
    ) -> TransactionConverterResult<InternalRpcTransaction>;

    async fn convert_internal_rpc_tx_to_executable_tx(
        &self,
        tx: InternalRpcTransaction,
    ) -> TransactionConverterResult<AccountTransaction>;
}

#[derive(Clone)]
pub struct TransactionConverter {
    #[allow(dead_code)]
    class_manager_client: SharedClassManagerClient,
}

impl TransactionConverter {
    pub fn new(class_manager_client: SharedClassManagerClient) -> Self {
        Self { class_manager_client }
    }
}

#[async_trait]
impl TransactionConverterTrait for TransactionConverter {
    async fn convert_internal_consensus_tx_to_consensus_tx(
        &self,
        _tx: InternalConsensusTransaction,
    ) -> TransactionConverterResult<ConsensusTransaction> {
        todo!()
    }

    async fn convert_consensus_tx_to_internal_consensus_tx(
        &self,
        _tx: ConsensusTransaction,
    ) -> TransactionConverterResult<InternalConsensusTransaction> {
        todo!()
    }

    async fn convert_internal_rpc_tx_to_rpc_tx(
        &self,
        _tx: InternalRpcTransaction,
    ) -> TransactionConverterResult<RpcTransaction> {
        todo!()
    }

    async fn convert_rpc_tx_to_internal_rpc_tx(
        &self,
        _tx: RpcTransaction,
    ) -> TransactionConverterResult<InternalRpcTransaction> {
        todo!()
    }

    async fn convert_internal_rpc_tx_to_executable_tx(
        &self,
        _tx: InternalRpcTransaction,
    ) -> TransactionConverterResult<AccountTransaction> {
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
