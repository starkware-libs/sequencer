use std::fmt::Debug;

use serde::{Deserialize, Serialize};
use starknet_sequencer_infra::component_client::ClientError;
use thiserror::Error;

#[derive(Clone, Debug, Error, PartialEq, Eq, Serialize, Deserialize)]
pub enum L1ProviderError {
    #[error(
        "`get_txs` called while in `Pending` state, likely due to a crash; restart block proposal"
    )]
    GetTransactionsInPendingState,
    #[error("`get_txs` while in validate state")]
    GetTransactionConsensusBug,
    #[error("Cannot transition from {from} to {to}")]
    UnexpectedProviderStateTransition { from: String, to: String },
    #[error(
        "`validate` called while in `Pending` state, likely due to a crash; restart block proposal"
    )]
    ValidateInPendingState,
    #[error("`validate` called while in `Propose`")]
    ValidateTransactionConsensusBug,
}

impl L1ProviderError {
    pub fn unexpected_transition(from: impl ToString, to: impl ToString) -> Self {
        Self::UnexpectedProviderStateTransition { from: from.to_string(), to: to.to_string() }
    }
}

#[derive(Clone, Debug, Error)]
pub enum L1ProviderClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    L1ProviderError(#[from] L1ProviderError),
}
