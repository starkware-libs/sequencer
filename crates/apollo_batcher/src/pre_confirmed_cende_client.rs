pub use apollo_batcher_config::config::PreconfirmedCendeConfig;
use apollo_batcher_types::batcher_types::Round;
use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use reqwest::Client;
use serde::Serialize;
use starknet_api::block::BlockNumber;
use thiserror::Error;
use tracing::{debug, trace, warn};
use url::Url;

use crate::cende_client_types::CendePreconfirmedBlock;
use crate::metrics::{
    record_preconfirmed_block_write_failure,
    PreconfirmedBlockWriteFailureReason,
    PRECONFIRMED_BLOCK_WRITTEN,
};

#[derive(Debug, Error)]
pub enum PreconfirmedCendeClientError {
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    #[error("Request failed with status code: {0}")]
    RequestFailed(reqwest::StatusCode),
}

pub type PreconfirmedCendeClientResult<T> = Result<T, PreconfirmedCendeClientError>;

/// Interface for communicating pre-confirmed block data to the Cende recorder during block
/// proposal.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait PreconfirmedCendeClientTrait: Send + Sync {
    /// Notifies the Cende recorder about a pre-confirmed block update.
    async fn write_pre_confirmed_block(
        &self,
        pre_confirmed_block: CendeWritePreconfirmedBlock,
    ) -> PreconfirmedCendeClientResult<()>;
}

pub struct PreconfirmedCendeClient {
    write_pre_confirmed_block_url: Url,
    client: Client,
}

// The endpoints for the Cende recorder.
pub const RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH: &str =
    "/cende_recorder/write_pre_confirmed_block";

impl PreconfirmedCendeClient {
    pub fn new(config: PreconfirmedCendeConfig) -> Self {
        let mut recorder_url = config.recorder_url;
        recorder_url = recorder_url
            .join(RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH)
            .expect("Failed to construct URL");
        Self { write_pre_confirmed_block_url: recorder_url, client: Client::new() }
    }
}

#[cfg_attr(test, derive(PartialEq))]
#[derive(Debug, Serialize)]
pub struct CendeWritePreconfirmedBlock {
    pub block_number: BlockNumber,
    pub round: Round,
    pub write_iteration: u64,
    pub pre_confirmed_block: CendePreconfirmedBlock,
}

#[async_trait]
impl PreconfirmedCendeClientTrait for PreconfirmedCendeClient {
    // We considered making this a best-effort method, since errors are already logged and recorded
    // in metrics here. However, we chose to propagate errors instead, as it makes success and
    // failure handling more explicit for the caller.
    async fn write_pre_confirmed_block(
        &self,
        pre_confirmed_block: CendeWritePreconfirmedBlock,
    ) -> PreconfirmedCendeClientResult<()> {
        let block_number = pre_confirmed_block.block_number;
        let round = pre_confirmed_block.round;
        let write_iteration = pre_confirmed_block.write_iteration;
        let number_of_txs = pre_confirmed_block.pre_confirmed_block.transactions.len();
        let number_of_preconfirmed_txs = pre_confirmed_block
            .pre_confirmed_block
            .transaction_receipts
            .iter()
            .filter(|opt| opt.is_some())
            .count();

        let request_builder =
            self.client.post(self.write_pre_confirmed_block_url.clone()).json(&pre_confirmed_block);

        trace!(
            "Sending write_pre_confirmed_block request to Cende recorder. \
             block_number={block_number}, round={round}, write_iteration={write_iteration}. The \
             block contains {number_of_txs} transactions and {number_of_preconfirmed_txs} \
             preconfirmed transactions.",
        );

        let response = request_builder.send().await.inspect_err(|err| {
            record_preconfirmed_block_write_failure(PreconfirmedBlockWriteFailureReason::SendError);
            warn!(
                "Failed to send write_pre_confirmed_block request to Cende recorder. \
                 block_number={block_number}, round={round}, write iteration={write_iteration}, \
                 error={err}"
            );
        })?;

        let response_status = response.status();
        if response_status.is_success() {
            debug!(
                "write_pre_confirmed_block request succeeded. block_number={block_number}, \
                 round={round}, write_iteration={write_iteration}, status={response_status}, \
                 n_txs={number_of_txs}, n_preconfirmed_txs={number_of_preconfirmed_txs}",
            );
            PRECONFIRMED_BLOCK_WRITTEN.increment(1);
            Ok(())
        } else {
            let reason =
                PreconfirmedBlockWriteFailureReason::from_response_status(&response_status);
            record_preconfirmed_block_write_failure(reason);
            warn!(
                "write_pre_confirmed_block request failed. block_number={block_number}, \
                 round={round}, write_iteration={write_iteration}, status={response_status}",
            );
            Err(PreconfirmedCendeClientError::RequestFailed(response_status))
        }
    }
}
