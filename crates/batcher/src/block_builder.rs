use std::sync::Arc;

use async_trait::async_trait;
use blockifier::blockifier::transaction_executor::{
    TransactionExecutorError as BlockifierTransactionExecutorError,
    VisitedSegmentsMapping,
};
use blockifier::bouncer::BouncerWeights;
use blockifier::state::cached_state::CommitmentStateDiff;
use blockifier::state::errors::StateError;
use blockifier::transaction::errors::TransactionExecutionError as BlockifierTransactionExecutionError;
use blockifier::transaction::objects::TransactionExecutionInfo;
use indexmap::IndexMap;
#[cfg(test)]
use mockall::automock;
use starknet_api::executable_transaction::Transaction;
use starknet_api::transaction::TransactionHash;
use starknet_batcher_types::batcher_types::GetProposalContent;
use thiserror::Error;

use crate::proposal_manager::InputTxStream;

#[derive(Debug, Default, PartialEq)]
pub struct BlockExecutionArtifacts {
    pub execution_infos: IndexMap<TransactionHash, TransactionExecutionInfo>,
    pub commitment_state_diff: CommitmentStateDiff,
    pub visited_segments_mapping: VisitedSegmentsMapping,
    pub bouncer_weights: BouncerWeights,
}

#[derive(Clone, Debug, Error)]
pub enum BlockBuilderError {
    #[error(transparent)]
    BadTimestamp(#[from] std::num::TryFromIntError),
    #[error(transparent)]
    BlockifierStateError(#[from] Arc<StateError>),
    #[error(transparent)]
    ExecutionError(#[from] Arc<BlockifierTransactionExecutorError>),
    #[error(transparent)]
    TransactionExecutionError(#[from] Arc<BlockifierTransactionExecutionError>),
    #[error(transparent)]
    StreamTransactionsError(#[from] tokio::sync::mpsc::error::SendError<Transaction>),
}

#[allow(dead_code)]
pub type BlockBuilderResult<T> = Result<T, BlockBuilderError>;

#[allow(dead_code)]
#[cfg_attr(test, automock)]
#[async_trait]
pub trait BlockBuilderTrait: Send {
    async fn build_block(
        &mut self,
        deadline: tokio::time::Instant,
        tx_stream: InputTxStream,
        output_content_sender: tokio::sync::mpsc::Sender<GetProposalContent>,
    ) -> BlockBuilderResult<BlockExecutionArtifacts>;

    fn abort_build(&self) -> BlockBuilderResult<()>;
}
