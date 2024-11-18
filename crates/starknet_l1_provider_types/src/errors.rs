use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Error, Serialize)]
// TODO(Mohammad): Add `EthereumBaseLayerError` error, and solve the serialization issue.
pub enum L1ProviderError {
    #[error(
        "`get_txs` called while in `Pending` state, likely due to a crash; restart block proposal"
    )]
    GetTransactionsInPendingState,
    #[error("`get_txs` while in validate state")]
    GetTransactionConsensusBug,
    #[error("Cannot transition from {from} to {to}")]
    UnexpectedProviderStateTransition { from: &'static str, to: &'static str },
    #[error("`validate_tx` called while in proposal state")]
    ValidateTransactionConsensusBug,
}
