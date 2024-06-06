use async_trait::async_trait;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::transaction::{Tip, TransactionHash};
use starknet_mempool_infra::component_client::ComponentClient;
use starknet_mempool_infra::component_server::ComponentRequestAndResponseSender;
use thiserror::Error;

use crate::errors::MempoolError;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ThinTransaction {
    pub sender_address: ContractAddress,
    pub tx_hash: TransactionHash,
    pub tip: Tip,
    pub nonce: Nonce,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AccountState {
    pub nonce: Nonce,
    // TODO: add balance field when needed.
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Account {
    // TODO(Ayelet): Consider removing this field as it is duplicated in ThinTransaction.
    pub sender_address: ContractAddress,
    pub state: AccountState,
}

#[derive(Debug, Default)]
pub struct MempoolInput {
    pub tx: ThinTransaction,
    pub account: Account,
}

pub type MempoolResult<T> = Result<T, MempoolError>;

#[derive(Debug, Error)]
pub enum MempoolClientError {
    #[error(transparent)]
    MempoolError(#[from] MempoolError),
    #[error(transparent)]
    ClientError(#[from] ClientError),
}
pub type MempoolClientResult<T> = Result<T, MempoolClientError>;

// TODO(Tsabary, 1/6/2024): Move communication-related definitions to a separate file.
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Got an unexpected response type.")]
    UnexpectedResponse,
}

/// Serves as the mempool's shared interface. Requires `Send + Sync` to allow transferring and
/// sharing resources (inputs, futures) across threads.
#[async_trait]
pub trait MempoolClient: Send + Sync {
    async fn add_tx(&self, mempool_input: MempoolInput) -> MempoolClientResult<()>;
    async fn get_txs(&self, n_txs: usize) -> MempoolClientResult<Vec<ThinTransaction>>;
}

#[derive(Debug)]
pub enum MempoolRequest {
    AddTransaction(MempoolInput),
    GetTransactions(usize),
}

#[derive(Debug)]
pub enum MempoolResponse {
    AddTransaction(MempoolResult<()>),
    GetTransactions(MempoolResult<Vec<ThinTransaction>>),
}

pub type MempoolClientImpl = ComponentClient<MempoolRequest, MempoolResponse>;
pub type MempoolRequestAndResponseSender =
    ComponentRequestAndResponseSender<MempoolRequest, MempoolResponse>;

#[async_trait]
impl MempoolClient for MempoolClientImpl {
    async fn add_tx(&self, mempool_input: MempoolInput) -> MempoolClientResult<()> {
        let request = MempoolRequest::AddTransaction(mempool_input);
        let res = self.send(request).await;
        match res {
            MempoolResponse::AddTransaction(Ok(res)) => Ok(res),
            MempoolResponse::AddTransaction(Err(res)) => Err(MempoolClientError::MempoolError(res)),
            _ => Err(MempoolClientError::ClientError(ClientError::UnexpectedResponse)),
        }
    }

    async fn get_txs(&self, n_txs: usize) -> MempoolClientResult<Vec<ThinTransaction>> {
        let request = MempoolRequest::GetTransactions(n_txs);
        let res = self.send(request).await;
        match res {
            MempoolResponse::GetTransactions(Ok(res)) => Ok(res),
            MempoolResponse::GetTransactions(Err(res)) => {
                Err(MempoolClientError::MempoolError(res))
            }
            _ => Err(MempoolClientError::ClientError(ClientError::UnexpectedResponse)),
        }
    }
}
