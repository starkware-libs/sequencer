use std::collections::BTreeMap;

use apollo_batcher_types::batcher_types::Round;
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use async_trait::async_trait;
use blockifier::fee::receipt::TransactionReceipt;
use reqwest::Client;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;
use url::Url;

// TODO(noamsp): rename PreConfirmed.. to Preconfirmed.. throughout the codebase.
#[derive(Clone, Debug, Error)]
pub enum PreConfirmedCendeClientError {}

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
    _start_new_round_url: Url,
    _write_pre_confirmed_txs_url: Url,
    _write_executed_txs_url: Url,
    _client: Client,
}

// The endpoints for the Cende recorder.
pub const RECORDER_START_NEW_ROUND_PATH: &str = "/cende_recorder/start_new_round";
pub const RECORDER_WRITE_PRE_CONFIRMED_TXS_PATH: &str = "/cende_recorder/write_pre_confirmed_txs";
pub const RECORDER_WRITE_EXECUTED_TXS_PATH: &str = "/cende_recorder/write_executed_txs";

impl PreConfirmedCendeClient {
    pub fn new(config: PreConfirmedCendeConfig) -> Result<Self, PreConfirmedCendeClientError> {
        let recorder_url = config.recorder_url;

        Ok(Self {
            _start_new_round_url: Self::construct_endpoint_url(
                recorder_url.clone(),
                RECORDER_START_NEW_ROUND_PATH,
            ),
            _write_pre_confirmed_txs_url: Self::construct_endpoint_url(
                recorder_url.clone(),
                RECORDER_WRITE_PRE_CONFIRMED_TXS_PATH,
            ),
            _write_executed_txs_url: Self::construct_endpoint_url(
                recorder_url,
                RECORDER_WRITE_EXECUTED_TXS_PATH,
            ),
            _client: Client::new(),
        })
    }

    fn construct_endpoint_url(url: Url, endpoint: &str) -> Url {
        url.join(endpoint).expect("Failed to construct URL")
    }
}

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
