use std::collections::BTreeMap;

use apollo_batcher_types::batcher_types::Round;
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use async_trait::async_trait;
use blockifier::fee::receipt::TransactionReceipt;
use indexmap::IndexMap;
use reqwest::{Client, RequestBuilder};
use serde::Serialize;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;
use tracing::{error, info, warn};
use url::Url;

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

/// Interface for communicating pre-confirmed transaction states to the Cende recorder during block
/// proposal.
#[async_trait]
pub trait PreConfirmedCendeClientTrait: Send + Sync {
    /// Notifies the Cende recorder about the start of a new proposal round.
    async fn send_start_new_round(
        &self,
        start_new_round: AerospikeStartNewRound,
    ) -> PreConfirmedCendeClientResult<()>;

    /// Notifies the Cende recorder about transactions that are pending execution, providing their
    /// hashes.
    async fn write_pre_confirmed_txs(
        &self,
        pre_confirmed_txs: AerospikePreConfirmedTxs,
    ) -> PreConfirmedCendeClientResult<()>;

    /// Notifies the Cende recorder about transactions that were executed successfully, providing
    /// their hashes and receipts.
    async fn write_executed_txs(
        &self,
        executed_txs: AerospikePreConfirmedTxs,
    ) -> PreConfirmedCendeClientResult<()>;
}

pub struct PreConfirmedCendeClient {
    start_new_round_url: Url,
    _write_pre_confirmed_txs_url: Url,
    _write_executed_txs_url: Url,
    client: Client,
}

// The endpoints for the Cende recorder.
pub const RECORDER_START_NEW_ROUND_PATH: &str = "/cende_recorder/start_new_round";
pub const RECORDER_WRITE_PRE_CONFIRMED_TXS_PATH: &str = "/cende_recorder/write_pre_confirmed_txs";
pub const RECORDER_WRITE_EXECUTED_TXS_PATH: &str = "/cende_recorder/write_executed_txs";

impl PreConfirmedCendeClient {
    pub fn new(config: PreConfirmedCendeConfig) -> Result<Self, PreConfirmedCendeClientError> {
        let recorder_url = config.recorder_url;

        Ok(Self {
            start_new_round_url: Self::construct_endpoint_url(
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
            client: Client::new(),
        })
    }

    fn construct_endpoint_url(recorder_url: Url, endpoint: &str) -> Url {
        recorder_url.join(endpoint).expect("Failed to construct URL")
    }

    async fn send_request(
        &self,
        request_builder: RequestBuilder,
        block_number: BlockNumber,
        proposal_round: Round,
        request_name: &'static str,
        additional_log_info: &str,
    ) -> PreConfirmedCendeClientResult<()> {
        info!(
            "Sending {request_name} request to Cende recorder. block_number: {block_number}, \
             round: {proposal_round}{additional_log_info}",
        );

        let response = request_builder.send().await?;

        let response_status = response.status();
        if response_status.is_success() {
            info!(
                "{request_name} request succeeded. block_number: {block_number}, round: \
                 {proposal_round}{additional_log_info}"
            );
            Ok(())
        } else {
            let error_msg = format!(
                "{request_name} request failed. block_number: {block_number}, round: \
                 {proposal_round}{additional_log_info}, status: {response_status}"
            );
            warn!("{error_msg}");
            Err(PreConfirmedCendeClientError::CendeRecorderError(error_msg))
        }
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

#[derive(Serialize)]
pub struct AerospikeStartNewRound {
    block_number: BlockNumber,
    round: Round,
}

#[derive(Serialize)]
pub struct PreConfirmedTransactionData {
    block_number: BlockNumber,
    round: Round,
    transaction_receipt: Option<TransactionReceipt>,
}

/// Invariant: all PreConfirmedTransactionData entries have block_number and proposal_round values
/// that match the corresponding values on this struct.
#[derive(Serialize)]
pub struct AerospikePreConfirmedTxs {
    block_number: BlockNumber,
    round: Round,
    transactions: IndexMap<TransactionHash, PreConfirmedTransactionData>,
}

#[async_trait]
impl PreConfirmedCendeClientTrait for PreConfirmedCendeClient {
    async fn send_start_new_round(
        &self,
        start_new_round: AerospikeStartNewRound,
    ) -> PreConfirmedCendeClientResult<()> {
        let request_builder =
            self.client.post(self.start_new_round_url.clone()).json(&start_new_round);

        self.send_request(
            request_builder,
            start_new_round.block_number,
            start_new_round.proposal_round,
            "start_new_round",
            "",
        )
        .await
    }

    async fn write_pre_confirmed_txs(
        &self,
        _pre_confirmed_txs: AerospikePreConfirmedTxs,
    ) -> PreConfirmedCendeClientResult<()> {
        todo!()
    }

    async fn write_executed_txs(
        &self,
        _executed_txs: AerospikePreConfirmedTxs,
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
        _start_new_round: AerospikeStartNewRound,
    ) -> PreConfirmedCendeClientResult<()> {
        Ok(())
    }

    async fn write_pre_confirmed_txs(
        &self,
        _pre_confirmed_txs: AerospikePreConfirmedTxs,
    ) -> PreConfirmedCendeClientResult<()> {
        Ok(())
    }

    async fn write_executed_txs(
        &self,
        _executed_txs: AerospikePreConfirmedTxs,
    ) -> PreConfirmedCendeClientResult<()> {
        Ok(())
    }
}
