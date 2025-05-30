use apollo_batcher_types::batcher_types::Round;
use async_trait::async_trait;
use blockifier::fee::receipt::TransactionReceipt;
use starknet_api::block::BlockNumber;
use starknet_api::transaction::TransactionHash;
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum PreConfirmedCendeClientError {}
pub type PreConfirmedCendeClientResult<T> = Result<T, PreConfirmedCendeClientError>;
#[async_trait]
pub trait PreConfirmedCendeClientTrait: Send + Sync {
    async fn send_start_new_round(
        &self,
        block_number: BlockNumber,
        proposal_round: Round,
    ) -> PreConfirmedCendeClientResult<()>;
    async fn send_pre_confirmed_txs(
        &self,
        block_number: BlockNumber,
        proposal_round: Round,
        pre_confirmed_txs: Vec<TransactionHash>,
    ) -> PreConfirmedCendeClientResult<()>;
    async fn send_executed_txs(
        &self,
        block_number: BlockNumber,
        proposal_round: Round,
        executed_txs: Vec<(TransactionHash, TransactionReceipt)>,
    ) -> PreConfirmedCendeClientResult<()>;
}

// TODO(noamsp): Remove this empty client once the Cende  client is implemented.
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
