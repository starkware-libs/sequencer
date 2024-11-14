use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerError;
use thiserror::Error;

use crate::ProviderState;

#[derive(Error, Debug)]
pub enum L1ProviderError {
    #[error(transparent)]
    BaseLayer(#[from] EthereumBaseLayerError),
    #[error(
        "`get_txs` called while in `Pending` state, likely due to a crash; restart block proposal"
    )]
    GetTransactionsInPendingState,
    #[error("`get_txs` while in validate state")]
    GetTransactionConsensusBug,
    #[error("Cannot transition from {from} to {to}")]
    UnexpectedProviderState { from: ProviderState, to: ProviderState },
    #[error("`validate_tx` called while in proposal state")]
    ValidateTransactionConsensusBug,
}
