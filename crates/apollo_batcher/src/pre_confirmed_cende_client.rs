use std::collections::BTreeMap;

use apollo_batcher_types::batcher_types::Round;
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use async_trait::async_trait;
use blockifier::fee::receipt::TransactionReceipt;
use reqwest::Client;
use serde::Serialize;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;
use tracing::{error, info, warn};
use url::Url;

#[derive(Debug, Error)]
// TODO(noamsp): add block number/round mismatch and handle it in the client implementation.
pub enum PreConfirmedCendeClientError {
    #[error(transparent)]
    RequestError(#[from] reqwest::Error),
    #[error("CendeRecorder returned an error: {0}")]
    CendeRecorderError(String),
}

pub type PreConfirmedCendeClientResult<T> = Result<T, PreConfirmedCendeClientError>;

/// Interface for communicating pre-confirmed transaction states to the Cende recorder during block
/// proposal.
#[async_trait]
pub trait PreConfirmedCendeClientTrait: Send + Sync {
    /// Notifies the Cende recorder about the start of a new proposal round.
    async fn send_start_new_round(
        &self,
        block_number: BlockNumber,
        proposal_round: Round,
    ) -> PreConfirmedCendeClientResult<()>;

    /// Notifies the Cende recorder about transactions that are pending execution, providing their
    /// hashes.
    async fn send_pre_confirmed_txs(
        &self,
        block_number: BlockNumber,
        proposal_round: Round,
        pre_confirmed_txs: Vec<TransactionHash>,
    ) -> PreConfirmedCendeClientResult<()>;

    /// Notifies the Cende recorder about transactions that were executed successfully, providing
    /// their hashes and receipts.
    async fn send_executed_txs(
        &self,
        block_number: BlockNumber,
        proposal_round: Round,
        executed_txs: Vec<(TransactionHash, TransactionReceipt)>,
    ) -> PreConfirmedCendeClientResult<()>;
}

pub struct PreConfirmedCendeClient {
    // We store the URLs as strings to avoid unnecessary cloning of the Url object.
    _start_new_round_url: String,
    _write_pre_confirmed_txs_url: String,
    _write_executed_txs_url: String,
    _client: Client,
}

// The endpoints for the Cende recorder.
pub const RECORDER_START_NEW_ROUND_PATH: &str = "/cende_recorder/start_new_round";
pub const RECORDER_WRITE_PRE_CONFIRMED_TXS_PATH: &str = "/cende_recorder/write_pre_confirmed_txs";
pub const RECORDER_WRITE_EXECUTED_TXS_PATH: &str = "/cende_recorder/write_executed_txs";

impl PreConfirmedCendeClient {
    pub fn new(config: PreConfirmedCendeConfig) -> Result<Self, PreConfirmedCendeClientError> {
        Ok(Self {
            _start_new_round_url: Self::construct_endpoint_url(
                config.clone(),
                RECORDER_START_NEW_ROUND_PATH,
            ),
            _write_pre_confirmed_txs_url: Self::construct_endpoint_url(
                config.clone(),
                RECORDER_WRITE_PRE_CONFIRMED_TXS_PATH,
            ),
            _write_executed_txs_url: Self::construct_endpoint_url(
                config,
                RECORDER_WRITE_EXECUTED_TXS_PATH,
            ),
            _client: Client::new(),
        })
    }

    fn construct_endpoint_url(config: PreConfirmedCendeConfig, endpoint: &str) -> String {
        config.recorder_url.join(endpoint).expect("Failed to construct URL").to_string()
    }

    // TODO(noamsp): remove this allow once
    #[allow(dead_code)]
    async fn send_request<T: Serialize>(
        &self,
        request: &'static str,
        block_number: BlockNumber,
        proposal_round: Round,
        num_txs: Option<usize>,
        data: &T,
        url: &str,
    ) -> PreConfirmedCendeClientResult<()> {
        let num_txs_str = num_txs.map_or(String::new(), |n| format!(", num_txs: {n}"));

        info!(
            "Sending {request} request to Cende recorder. block_number: {block_number}, round: \
             {proposal_round}{num_txs_str}",
        );

        let response = self._client.post(url).json(data).send().await?;

        if response.status().is_success() {
            info!(
                "{request} request succeeded. block_number: {block_number}, round: \
                 {proposal_round}{num_txs_str}"
            );
            Ok(())
        } else {
            let error_msg = format!(
                "{request} request failed. block_number: {block_number}, round: \
                 {proposal_round}{num_txs_str}, status: {}",
                response.status(),
            );
            warn!("{error_msg}");
            Err(PreConfirmedCendeClientError::CendeRecorderError(error_msg))
        }
    }
}

#[derive(Clone)]
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

#[async_trait]
impl PreConfirmedCendeClientTrait for PreConfirmedCendeClient {
    async fn send_start_new_round(
        &self,
        _block_number: BlockNumber,
        _proposal_round: Round,
    ) -> PreConfirmedCendeClientResult<()> {
        todo!()
    }

    async fn send_pre_confirmed_txs(
        &self,
        _block_number: BlockNumber,
        _proposal_round: Round,
        _pre_confirmed_txs: Vec<TransactionHash>,
    ) -> PreConfirmedCendeClientResult<()> {
        todo!()
    }

    async fn send_executed_txs(
        &self,
        _block_number: BlockNumber,
        _proposal_round: Round,
        _executed_txs: Vec<(TransactionHash, TransactionReceipt)>,
    ) -> PreConfirmedCendeClientResult<()> {
        todo!()
    }
}

// TODO(noamsp): Remove this empty client once the Cende client is implemented.
pub struct EmptyPreConfirmedCendeClient;

#[async_trait]
impl PreConfirmedCendeClientTrait for EmptyPreConfirmedCendeClient {
    async fn send_start_new_round(
        &self,
        _block_number: BlockNumber,
        _proposal_round: Round,
    ) -> PreConfirmedCendeClientResult<()> {
        Ok(())
    }

    async fn send_pre_confirmed_txs(
        &self,
        _block_number: BlockNumber,
        _proposal_round: Round,
        _pre_confirmed_txs: Vec<TransactionHash>,
    ) -> PreConfirmedCendeClientResult<()> {
        Ok(())
    }

    async fn send_executed_txs(
        &self,
        _block_number: BlockNumber,
        _proposal_round: Round,
        _executed_txs: Vec<(TransactionHash, TransactionReceipt)>,
    ) -> PreConfirmedCendeClientResult<()> {
        Ok(())
    }
}
