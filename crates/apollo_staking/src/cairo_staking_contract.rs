use apollo_batcher_types::batcher_types::{CallContractInput, CallContractOutput};
use apollo_batcher_types::communication::SharedBatcherClient;
use apollo_cairo_utils::CairoOption;
use async_trait::async_trait;
use blockifier::execution::call_info::Retdata;
use starknet_api::core::ContractAddress;
use starknet_types_core::felt::Felt;
use tracing::info;

use crate::committee_provider::Staker;
use crate::contract_types::{
    ContractStaker,
    GET_CURRENT_EPOCH_DATA_ENTRY_POINT,
    GET_PREVIOUS_EPOCH_DATA_ENTRY_POINT,
    GET_STAKERS_ENTRY_POINT,
};
use crate::staking_contract::{StakingContract, StakingContractResult};
use crate::staking_manager::Epoch;

#[cfg(test)]
#[path = "cairo_staking_contract_test.rs"]
mod cairo_staking_contract_test;

/// Staking contract implementation operating against a deployed Cairo staking contract via the
/// batcher's call_contract RPC.
pub struct CairoStakingContract {
    contract_address: ContractAddress,
    batcher_client: SharedBatcherClient,
}

impl CairoStakingContract {
    pub fn new(contract_address: ContractAddress, batcher_client: SharedBatcherClient) -> Self {
        Self { contract_address, batcher_client }
    }

    async fn call_view(
        &self,
        entry_point: &str,
        calldata: Vec<Felt>,
    ) -> StakingContractResult<Retdata> {
        let output: CallContractOutput = self
            .batcher_client
            .call_contract(CallContractInput {
                contract_address: self.contract_address,
                entry_point: entry_point.to_string(),
                calldata,
            })
            .await?;
        Ok(Retdata(output.retdata))
    }
}

#[async_trait]
impl StakingContract for CairoStakingContract {
    async fn get_stakers(&self, epoch: u64) -> StakingContractResult<Vec<Staker>> {
        info!("Calling staking contract {GET_STAKERS_ENTRY_POINT} for epoch={epoch}.");
        let retdata = self.call_view(GET_STAKERS_ENTRY_POINT, vec![Felt::from(epoch)]).await?;

        // Filter out stakers that don't have a public key.
        let contract_stakers = ContractStaker::from_retdata_many(retdata)?;
        let initial_len = contract_stakers.len();
        let stakers: Vec<Staker> = contract_stakers
            .into_iter()
            .filter_map(|contract_staker| {
                contract_staker.public_key.map(|_| Staker::from(&contract_staker))
            })
            .collect();

        info!(
            "Retrieved {} stakers for epoch={}, filtered out {} without public key.",
            stakers.len(),
            epoch,
            initial_len - stakers.len()
        );

        Ok(stakers)
    }

    async fn get_current_epoch(&self) -> StakingContractResult<Epoch> {
        info!("Calling staking contract {GET_CURRENT_EPOCH_DATA_ENTRY_POINT}.");
        let retdata = self.call_view(GET_CURRENT_EPOCH_DATA_ENTRY_POINT, vec![]).await?;
        let epoch = Epoch::try_from(retdata)?;
        info!("Retrieved current epoch from contract: {epoch:?}.",);
        Ok(epoch)
    }

    async fn get_previous_epoch(&self) -> StakingContractResult<Option<Epoch>> {
        info!("Calling staking contract {GET_PREVIOUS_EPOCH_DATA_ENTRY_POINT}.");
        let retdata = self.call_view(GET_PREVIOUS_EPOCH_DATA_ENTRY_POINT, vec![]).await?;
        let epoch = CairoOption::<Epoch>::try_from(retdata)?.0;
        info!("Retrieved previous epoch from contract: {epoch:?}.");
        Ok(epoch)
    }
}
