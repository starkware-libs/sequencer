use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerContract;
use papyrus_base_layer::BaseLayerContract;
use starknet_api::block::{BlockHash, BlockNumber};

pub type EthereumBaseLayerSource = EthereumBaseLayerContract;

#[derive(thiserror::Error, Debug)]
pub enum BaseLayerSourceError {
    #[error("Base layer error: {0}")]
    BaseLayerContractError(Box<dyn BaseLayerSourceErrorTrait>),
    #[error("Base layer source creation error: {0}.")]
    BaseLayerSourceCreationError(String),
    #[error(
        "Finality is too high: finality: {finality}, latest L1 block number: \
         {latest_l1_block_number}"
    )]
    FinalityTooHigh { finality: u64, latest_l1_block_number: u64 },
}

pub trait BaseLayerSourceErrorTrait: std::error::Error + Sync + Send {}

impl<Error: std::error::Error + Sync + Send> BaseLayerSourceErrorTrait for Error {}

#[cfg_attr(test, automock)]
#[async_trait]
pub trait BaseLayerSourceTrait {
    async fn latest_proved_block(
        &self,
    ) -> Result<Option<(BlockNumber, BlockHash)>, BaseLayerSourceError>;
}

#[async_trait]
impl<
    Error: std::error::Error + 'static + Sync + Send,
    BaseLayerSource: BaseLayerContract<Error = Error> + Sync + Send,
> BaseLayerSourceTrait for BaseLayerSource
{
    async fn latest_proved_block(
        &self,
    ) -> Result<Option<(BlockNumber, BlockHash)>, BaseLayerSourceError> {
        let finality = 0;
        let latest_l1_block_number = self
            .latest_l1_block_number()
            .await
            .map_err(|e| BaseLayerSourceError::BaseLayerContractError(Box::new(e)))?;
        if latest_l1_block_number < finality {
            return Err(BaseLayerSourceError::FinalityTooHigh { finality, latest_l1_block_number });
        }
        let latest_l1_block_number = latest_l1_block_number.saturating_sub(finality);
        // TODO(guyn): There's no way to actually get an Ok(None) from this function. Consider
        // adding a check against the type of error we get back from get_proved_block_at() and
        // converting that into Ok(None) if we are truly convinced that there are no proved blocks
        // on the chain yet.
        self.get_proved_block_at(latest_l1_block_number)
            .await
            .map_err(|e| BaseLayerSourceError::BaseLayerContractError(Box::new(e)))
            .map(|block| Some((block.number, block.hash)))
    }
}
