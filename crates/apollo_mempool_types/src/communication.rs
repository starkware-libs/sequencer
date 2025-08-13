use std::sync::Arc;

use apollo_infra::component_client::{ClientError, LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{
    ComponentClient,
    ComponentRequestAndResponseSender,
    PrioritizedRequest,
    RequestPriority,
};
use apollo_infra::{impl_debug_for_infra_requests_and_responses, impl_labeled_request};
use apollo_network_types::network_types::BroadcastedMessageMetadata;
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
#[cfg(any(feature = "testing", test))]
use mockall::automock;
use serde::{Deserialize, Serialize};
use starknet_api::block::GasPrice;
use starknet_api::core::ContractAddress;
use starknet_api::rpc_transaction::InternalRpcTransaction;
use strum::EnumVariantNames;
use strum_macros::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr};
use thiserror::Error;

use crate::errors::MempoolError;
use crate::mempool_types::{AddTransactionArgs, CommitBlockArgs, MempoolSnapshot};

pub type LocalMempoolClient = LocalComponentClient<MempoolRequest, MempoolResponse>;
pub type RemoteMempoolClient = RemoteComponentClient<MempoolRequest, MempoolResponse>;
pub type MempoolResult<T> = Result<T, MempoolError>;
pub type MempoolClientResult<T> = Result<T, MempoolClientError>;
pub type MempoolRequestAndResponseSender =
    ComponentRequestAndResponseSender<MempoolRequest, MempoolResponse>;
pub type SharedMempoolClient = Arc<dyn MempoolClient>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AddTransactionArgsWrapper {
    pub args: AddTransactionArgs,
    pub p2p_message_metadata: Option<BroadcastedMessageMetadata>,
}

/// Serves as the mempool's shared interface. Requires `Send + Sync` to allow transferring and
/// sharing resources (inputs, futures) across threads.
#[cfg_attr(any(feature = "testing", test), automock)]
#[async_trait]
pub trait MempoolClient: Send + Sync {
    // TODO(AlonH): Add Option<BroadcastedMessageMetadata> as an argument for add_transaction
    // TODO(AlonH): Rename tx to transaction
    async fn add_tx(&self, args: AddTransactionArgsWrapper) -> MempoolClientResult<()>;
    async fn commit_block(&self, args: CommitBlockArgs) -> MempoolClientResult<()>;
    async fn get_txs(&self, n_txs: usize) -> MempoolClientResult<Vec<InternalRpcTransaction>>;
    async fn account_tx_in_pool_or_recent_block(
        &self,
        contract_address: ContractAddress,
    ) -> MempoolClientResult<bool>;
    async fn update_gas_price(&self, gas_price: GasPrice) -> MempoolClientResult<()>;
    async fn get_mempool_snapshot(&self) -> MempoolClientResult<MempoolSnapshot>;
}

#[derive(Serialize, Deserialize, Clone, AsRefStr, EnumDiscriminants)]
#[strum_discriminants(
    name(MempoolRequestLabelValue),
    derive(IntoStaticStr, EnumIter, EnumVariantNames),
    strum(serialize_all = "snake_case")
)]
pub enum MempoolRequest {
    AddTransaction(AddTransactionArgsWrapper),
    CommitBlock(CommitBlockArgs),
    GetTransactions(usize),
    AccountTxInPoolOrRecentBlock(ContractAddress),
    // TODO(yair): Rename to `StartBlock` and add cleanup of staged txs.
    UpdateGasPrice(GasPrice),
    GetMempoolSnapshot(),
}
impl_debug_for_infra_requests_and_responses!(MempoolRequest);
impl_labeled_request!(MempoolRequest, MempoolRequestLabelValue);
impl PrioritizedRequest for MempoolRequest {
    fn priority(&self) -> RequestPriority {
        match self {
            MempoolRequest::CommitBlock(_) | MempoolRequest::GetTransactions(_) => {
                RequestPriority::High
            }
            MempoolRequest::AddTransaction(_)
            | MempoolRequest::AccountTxInPoolOrRecentBlock(_)
            | MempoolRequest::UpdateGasPrice(_)
            | MempoolRequest::GetMempoolSnapshot() => RequestPriority::Normal,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum MempoolResponse {
    AddTransaction(MempoolResult<()>),
    CommitBlock(MempoolResult<()>),
    GetTransactions(MempoolResult<Vec<InternalRpcTransaction>>),
    AccountTxInPoolOrRecentBlock(MempoolResult<bool>),
    UpdateGasPrice(MempoolResult<()>),
    GetMempoolSnapshot(MempoolResult<MempoolSnapshot>),
}
impl_debug_for_infra_requests_and_responses!(MempoolResponse);

#[derive(Clone, Debug, Error)]
pub enum MempoolClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    MempoolError(#[from] MempoolError),
}

#[async_trait]
impl<ComponentClientType> MempoolClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<MempoolRequest, MempoolResponse>,
{
    async fn add_tx(&self, args: AddTransactionArgsWrapper) -> MempoolClientResult<()> {
        let request = MempoolRequest::AddTransaction(args);
        handle_all_response_variants!(
            MempoolResponse,
            AddTransaction,
            MempoolClientError,
            MempoolError,
            Direct
        )
    }

    async fn commit_block(&self, args: CommitBlockArgs) -> MempoolClientResult<()> {
        let request = MempoolRequest::CommitBlock(args);
        handle_all_response_variants!(
            MempoolResponse,
            CommitBlock,
            MempoolClientError,
            MempoolError,
            Direct
        )
    }

    async fn get_txs(&self, n_txs: usize) -> MempoolClientResult<Vec<InternalRpcTransaction>> {
        let request = MempoolRequest::GetTransactions(n_txs);
        handle_all_response_variants!(
            MempoolResponse,
            GetTransactions,
            MempoolClientError,
            MempoolError,
            Direct
        )
    }

    async fn account_tx_in_pool_or_recent_block(
        &self,
        account_address: ContractAddress,
    ) -> MempoolClientResult<bool> {
        let request = MempoolRequest::AccountTxInPoolOrRecentBlock(account_address);
        handle_all_response_variants!(
            MempoolResponse,
            AccountTxInPoolOrRecentBlock,
            MempoolClientError,
            MempoolError,
            Direct
        )
    }

    async fn update_gas_price(&self, gas_price: GasPrice) -> MempoolClientResult<()> {
        let request = MempoolRequest::UpdateGasPrice(gas_price);
        handle_all_response_variants!(
            MempoolResponse,
            UpdateGasPrice,
            MempoolClientError,
            MempoolError,
            Direct
        )
    }

    async fn get_mempool_snapshot(&self) -> MempoolClientResult<MempoolSnapshot> {
        let request = MempoolRequest::GetMempoolSnapshot();
        handle_all_response_variants!(
            MempoolResponse,
            GetMempoolSnapshot,
            MempoolClientError,
            MempoolError,
            Direct
        )
    }
}
