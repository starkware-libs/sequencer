use std::sync::Arc;

use apollo_batcher_types::batcher_types::Round;
use async_trait::async_trait;
use blockifier::fee::receipt::TransactionReceipt;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum PreConfirmedCendeWriterError {}
pub type PreConfirmedCendeWriterResult<T> = Result<T, PreConfirmedCendeWriterError>;
#[async_trait]
pub trait PreConfirmedCendeWriterClientTrait: Send + Sync {
    async fn send_start_new_round(
        &self,
        block_number: BlockNumber,
        proposal_round: Round,
    ) -> PreConfirmedCendeWriterResult<()>;
    async fn send_pre_confirmed_txs(
        &self,
        block_number: BlockNumber,
        proposal_round: Round,
        pre_confirmed_txs: Vec<TransactionHash>,
    ) -> PreConfirmedCendeWriterResult<()>;
    async fn send_executed_txs(
        &self,
        block_number: BlockNumber,
        proposal_round: Round,
        executed_txs: Vec<(TransactionHash, TransactionReceipt)>,
    ) -> PreConfirmedCendeWriterResult<()>;
}

// TODO(noamsp): Remove this empty client once the Cende writer client is implemented.
pub struct EmptyPreConfirmedCendeWriterClient;
#[async_trait]
impl PreConfirmedCendeWriterClientTrait for EmptyPreConfirmedCendeWriterClient {
    async fn send_start_new_round(
        &self,
        _block_number: BlockNumber,
        _proposal_round: Round,
    ) -> PreConfirmedCendeWriterResult<()> {
        Ok(())
    }
    async fn send_pre_confirmed_txs(
        &self,
        _block_number: BlockNumber,
        _proposal_round: Round,
        _pre_confirmed_txs: Vec<TransactionHash>,
    ) -> PreConfirmedCendeWriterResult<()> {
        Ok(())
    }
    async fn send_executed_txs(
        &self,
        _block_number: BlockNumber,
        _proposal_round: Round,
        _executed_txs: Vec<(TransactionHash, TransactionReceipt)>,
    ) -> PreConfirmedCendeWriterResult<()> {
        Ok(())
    }
}

pub struct PreConfirmedCendeWriter {
    pub block_number: BlockNumber,
    pub proposal_round: Round,
    pub pre_confirmed_cende_writer_client: Arc<dyn PreConfirmedCendeWriterClientTrait>,
}

#[async_trait]
pub trait PreConfirmedCendeWriterTrait: Send + Sync {
    async fn send_start_new_round(&self) -> PreConfirmedCendeWriterResult<()>;
    async fn send_pre_confirmed_txs(
        &self,
        pre_confirmed_txs: Vec<TransactionHash>,
    ) -> PreConfirmedCendeWriterResult<()>;
    async fn send_executed_txs(
        &self,
        executed_txs: Vec<(TransactionHash, TransactionReceipt)>,
    ) -> PreConfirmedCendeWriterResult<()>;
}

impl PreConfirmedCendeWriter {
    pub fn new(
        block_number: BlockNumber,
        proposal_round: Round,
        pre_confirmed_cende_writer_client: Arc<dyn PreConfirmedCendeWriterClientTrait>,
    ) -> Self {
        Self { block_number, proposal_round, pre_confirmed_cende_writer_client }
    }
}

#[async_trait]
impl PreConfirmedCendeWriterTrait for PreConfirmedCendeWriter {
    async fn send_start_new_round(&self) -> PreConfirmedCendeWriterResult<()> {
        self.pre_confirmed_cende_writer_client
            .send_start_new_round(self.block_number, self.proposal_round)
            .await
    }
    async fn send_pre_confirmed_txs(
        &self,
        pre_confirmed_txs: Vec<TransactionHash>,
    ) -> PreConfirmedCendeWriterResult<()> {
        if pre_confirmed_txs.is_empty() {
            return Ok(());
        }

        self.pre_confirmed_cende_writer_client
            .send_pre_confirmed_txs(self.block_number, self.proposal_round, pre_confirmed_txs)
            .await
    }

    async fn send_executed_txs(
        &self,
        executed_txs: Vec<(TransactionHash, TransactionReceipt)>,
    ) -> PreConfirmedCendeWriterResult<()> {
        if executed_txs.is_empty() {
            return Ok(());
        }

        self.pre_confirmed_cende_writer_client
            .send_executed_txs(self.block_number, self.proposal_round, executed_txs)
            .await
    }
}

pub struct PreConfirmedCendeWriterFactory {
    pub pre_confirmed_cende_writer_client: Arc<dyn PreConfirmedCendeWriterClientTrait>,
}

impl PreConfirmedCendeWriterFactory {
    pub fn create_pre_confirmed_cende_writer(
        &self,
        block_number: BlockNumber,
        proposal_round: Round,
    ) -> PreConfirmedCendeWriter {
        PreConfirmedCendeWriter::new(
            block_number,
            proposal_round,
            self.pre_confirmed_cende_writer_client.clone(),
        )
    }
}
