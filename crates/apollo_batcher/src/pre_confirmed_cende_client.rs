use std::collections::BTreeMap;

use apollo_batcher_types::batcher_types::Round;
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use starknet_api::block::BlockNumber;
use thiserror::Error;
use url::Url;

use crate::cende_client_types::CendePreConfirmedBlock;

// TODO(noamsp): rename PreConfirmed.. to Preconfirmed.. throughout the codebase.
#[derive(Debug, Error)]
// TODO(noamsp): add block number/round mismatch and handle it in the client implementation.
pub enum PreConfirmedCendeClientError {
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    #[error("CendeRecorder returned an error: {0}")]
    CendeRecorderError(String),
}

pub type PreConfirmedCendeClientResult<T> = Result<T, PreConfirmedCendeClientError>;

/// Interface for communicating pre-confirmed block data to the Cende recorder during block
/// proposal.
#[async_trait]
pub trait PreConfirmedCendeClientTrait: Send + Sync {
    /// Notifies the Cende recorder about a pre-confirmed block update.
    async fn write_pre_confirmed_block(
        &self,
        pre_confirmed_block: CendeWritePreConfirmedBlock,
    ) -> PreConfirmedCendeClientResult<()>;
}

pub struct PreConfirmedCendeClient {
    _write_pre_confirmed_block_url: Url,
    _client: Client,
}

// The endpoints for the Cende recorder.
pub const RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH: &str =
    "/cende_recorder/write_pre_confirmed_block";

impl PreConfirmedCendeClient {
    pub fn new(config: PreConfirmedCendeConfig) -> Self {
        let recorder_url = config.recorder_url;

        Self {
            _write_pre_confirmed_block_url: recorder_url
                .join(RECORDER_WRITE_PRE_CONFIRMED_BLOCK_PATH)
                .expect("Failed to construct URL"),
            _client: Client::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreConfirmedCendeConfig {
    pub recorder_url: Url,
}

impl Default for PreConfirmedCendeConfig {
    fn default() -> Self {
        Self {
            recorder_url: "https://recorder_url"
                .parse()
                .expect("recorder_url must be a valid Recorder URL"),
        }
    }
}

impl SerializeConfig for PreConfirmedCendeConfig {
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
pub struct CendeWritePreConfirmedBlock {
    pub block_number: BlockNumber,
    pub round: Round,
    pub write_iteration: u64,
    pub pre_confirmed_block: CendePreConfirmedBlock,
}

#[async_trait]
impl PreConfirmedCendeClientTrait for PreConfirmedCendeClient {
    async fn write_pre_confirmed_block(
        &self,
        _pre_confirmed_block: CendeWritePreConfirmedBlock,
    ) -> PreConfirmedCendeClientResult<()> {
        // TODO(noamsp): implement this.
        Ok(())
    }
}
