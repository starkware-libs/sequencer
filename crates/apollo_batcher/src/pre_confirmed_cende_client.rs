use std::collections::BTreeMap;

use apollo_batcher_types::batcher_types::Round;
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use thiserror::Error;
use tracing::{debug, error, trace, warn};
use url::Url;

use crate::cende_client_types::CendePreconfirmedBlock;
use crate::metrics::PRECONFIRMED_BLOCK_WRITTEN;

#[derive(Debug, Error)]
pub enum PreconfirmedCendeClientError {
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    #[error(
        "Aerospike rejected an outdated proposal. A newer proposal was already written. round: \
         {0}, write_iteration: {1}."
    )]
    OutdatedProposalError(Round, u64),
    #[error("Cende recorder returned an error: {0}")]
    CendeRecorderError(String),
}

pub type PreconfirmedCendeClientResult<T> = Result<T, PreconfirmedCendeClientError>;

/// Interface for communicating pre-confirmed block data to the Cende recorder during block
/// proposal.
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
        let recorder_url = config.recorder_url;

        Self {
            write_pre_confirmed_block_url: recorder_url
                .join(RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH)
                .expect("Failed to construct URL"),
            client: Client::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreconfirmedCendeConfig {
    pub recorder_url: Url,
}

impl Default for PreconfirmedCendeConfig {
    fn default() -> Self {
        Self {
            recorder_url: "https://recorder_url"
                .parse()
                .expect("recorder_url must be a valid Recorder URL"),
        }
    }
}

impl SerializeConfig for PreconfirmedCendeConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from([ser_param(
            "recorder_url",
            &self.recorder_url,
            "The URL of the Pythonic cende_recorder",
            ParamPrivacyInput::Private,
        )])
    }
}

#[derive(Serialize)]
pub struct CendeWritePreconfirmedBlock {
    pub block_number: BlockNumber,
    pub round: Round,
    pub write_iteration: u64,
    pub pre_confirmed_block: CendePreconfirmedBlock,
}

#[async_trait]
impl PreconfirmedCendeClientTrait for PreconfirmedCendeClient {
    async fn write_pre_confirmed_block(
        &self,
        pre_confirmed_block: CendeWritePreconfirmedBlock,
    ) -> PreconfirmedCendeClientResult<()> {
        let block_number = pre_confirmed_block.block_number;
        let round = pre_confirmed_block.round;
        let write_iteration = pre_confirmed_block.write_iteration;

        let request_builder =
            self.client.post(self.write_pre_confirmed_block_url.clone()).json(&pre_confirmed_block);

        trace!(
            "Sending write_pre_confirmed_block request to Cende recorder. block_number: \
             {block_number}, round: {round}, write_iteration: {write_iteration}",
        );

        let response = request_builder.send().await?;

        let response_status = response.status();
        if response_status.is_success() {
            debug!(
                "write_pre_confirmed_block request succeeded. block_number: {block_number}, \
                 round: {round}, write_iteration: {write_iteration}.",
            );
            PRECONFIRMED_BLOCK_WRITTEN.increment(1);
            Ok(())
        } else {
            let error_msg = format!(
                "write_pre_confirmed_block request failed. block_number: {block_number}, round: \
                 {round}, write_iteration: {write_iteration}, response status: {response_status}."
            );

            warn!("{error_msg}");

            if response_status == StatusCode::BAD_REQUEST {
                return Err(PreconfirmedCendeClientError::OutdatedProposalError(
                    round,
                    write_iteration,
                ));
            } else {
                return Err(PreconfirmedCendeClientError::CendeRecorderError(error_msg));
            }
        }
    }
}
