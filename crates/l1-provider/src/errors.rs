use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum L1ProviderError {
    #[error(transparent)]
    BaseLayer(#[from] EthereumBaseLayerError),
    #[error(
        "`get_txs` called while in `Pending` state, likely due to a crash; restart block proposal"
    )]
    GetTxsInPendingState,
    #[error("`get_txs` while in validate state")]
    GetTxsConsensusBug,
    #[error("Can not set state: {0}")]
    UnexpectedState(String),
    #[error("`validate_tx` called while in proposal state")]
    ValidateTxConsensusBug,
}
