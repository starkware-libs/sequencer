use std::sync::Arc;

use apollo_batcher_types::batcher_types::ProposalId;
use async_trait::async_trait;
use blockifier::fee::receipt::TransactionReceipt;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum PreConfirmedCendeWriterClientError {}
pub type PreConfirmedCendeWriterClientResult<T> = Result<T, PreConfirmedCendeWriterClientError>;

#[async_trait]
pub trait PreConfirmedCendeWriterClientTrait: Send + Sync {
    async fn send_new_proposal_initiated(
        &self,
        block_number: BlockNumber,
        proposal_id: ProposalId,
    ) -> PreConfirmedCendeWriterClientResult<()>;
    async fn send_pre_confirmed_txs(
        &self,
        pre_confirmed_txs: Vec<TransactionHash>,
    ) -> PreConfirmedCendeWriterClientResult<()>;
    async fn send_executed_txs(
        &self,
        executed_txs: Vec<(TransactionHash, TransactionReceipt)>,
    ) -> PreConfirmedCendeWriterClientResult<()>;
}

pub type SharedPreConfirmedCendeWriterClient = Arc<dyn PreConfirmedCendeWriterClientTrait>;

// TODO(noamsp): Remove this empty client once the Cende writer client is implemented.
pub struct EmptyPreConfirmedCendeWriterClient;
#[async_trait]
impl PreConfirmedCendeWriterClientTrait for EmptyPreConfirmedCendeWriterClient {
    async fn send_new_proposal_initiated(
        &self,
        _block_number: BlockNumber,
        _proposal_id: ProposalId,
    ) -> PreConfirmedCendeWriterClientResult<()> {
        Ok(())
    }
    async fn send_pre_confirmed_txs(
        &self,
        _pre_confirmed_txs: Vec<TransactionHash>,
    ) -> PreConfirmedCendeWriterClientResult<()> {
        Ok(())
    }
    async fn send_executed_txs(
        &self,
        _executed_txs: Vec<(TransactionHash, TransactionReceipt)>,
    ) -> PreConfirmedCendeWriterClientResult<()> {
        Ok(())
    }
}

#[derive(Clone, Debug, Error)]
pub enum PreConfirmedCendeWriterError {
    #[error(transparent)]
    PreConfirmedCendeWriterClientError(#[from] PreConfirmedCendeWriterClientError),
}
pub type PreConfirmedCendeWriterResult<T> = Result<T, PreConfirmedCendeWriterError>;

pub struct PreConfirmedCendeWriter {
    pub block_number: BlockNumber,
    pub proposal_id: ProposalId,
    pub pre_confirmed_cende_writer_client: SharedPreConfirmedCendeWriterClient,
}

impl PreConfirmedCendeWriter {
    pub fn new(
        block_number: BlockNumber,
        proposal_id: ProposalId,
        pre_confirmed_cende_writer_client: SharedPreConfirmedCendeWriterClient,
    ) -> Self {
        Self { block_number, proposal_id, pre_confirmed_cende_writer_client }
    }
    pub async fn send_new_proposal_initiated(&mut self) -> PreConfirmedCendeWriterResult<()> {
        Ok(self
            .pre_confirmed_cende_writer_client
            .send_new_proposal_initiated(self.block_number, self.proposal_id)
            .await?)
    }
    pub async fn write_pre_confirmed_txs(
        &mut self,
        pre_confirmed_txs: Vec<TransactionHash>,
    ) -> PreConfirmedCendeWriterResult<()> {
        if pre_confirmed_txs.is_empty() {
            return Ok(());
        }

        Ok(self.pre_confirmed_cende_writer_client.send_pre_confirmed_txs(pre_confirmed_txs).await?)
    }

    pub async fn write_executed_txs(
        &mut self,
        executed_txs: Vec<(TransactionHash, TransactionReceipt)>,
    ) -> PreConfirmedCendeWriterResult<()> {
        if executed_txs.is_empty() {
            return Ok(());
        }

        self.pre_confirmed_cende_writer_client.send_executed_txs(executed_txs).await?;
        Ok(())
    }
}

pub struct PreConfirmedCendeWriterFactory {
    pub pre_confirmed_cende_writer_client: SharedPreConfirmedCendeWriterClient,
}

impl PreConfirmedCendeWriterFactory {
    pub fn create_pre_confirmed_cende_writer(
        &self,
        block_number: BlockNumber,
        proposal_id: ProposalId,
    ) -> PreConfirmedCendeWriter {
        PreConfirmedCendeWriter::new(
            block_number,
            proposal_id,
            self.pre_confirmed_cende_writer_client.clone(),
        )
    }
}
