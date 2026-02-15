use std::sync::Arc;

use async_trait::async_trait;
use blockifier::blockifier_versioned_constants::VersionedConstants;
use blockifier::bouncer::BouncerConfig;
use blockifier::context::{BlockContext, ChainInfo};
use blockifier::execution::call_info::Retdata;
use blockifier::execution::entry_point::call_view_entry_point;
use blockifier::state::state_api::StateReader;
use starknet_api::block::BlockInfo;
use starknet_api::core::ContractAddress;
use starknet_api::transaction::fields::Calldata;
use starknet_api::versioned_constants_logic::VersionedConstantsTrait;
use starknet_types_core::felt::Felt;
use thiserror::Error;

use crate::committee_provider::Staker;
use crate::contract_types::{
    ContractStaker,
    GET_CURRENT_EPOCH_DATA_ENTRY_POINT,
    GET_STAKERS_ENTRY_POINT,
};
use crate::staking_contract::{StakingContract, StakingContractResult};
use crate::staking_manager::Epoch;

#[cfg(test)]
#[path = "cairo_staking_contract_test.rs"]
mod cairo_staking_contract_test;

#[derive(Debug, Error)]
pub enum ExtendedStateReaderError {}

/// Trait for extending the StateReader with additional functionality.
// TODO(Dafna): This is very similar to the existing GatewayFixedBlockStateReader trait. Consider
// merging them.
pub trait ExtendedStateReader: StateReader {
    fn get_block_info(&self) -> Result<BlockInfo, ExtendedStateReaderError>;
}

/// Factory for producing fresh state reader bounded to the latest block.
pub trait StateReaderFactory: Send + Sync {
    fn create(&self) -> Box<dyn ExtendedStateReader>;
}

/// Staking contract implementation operating against a deployed Cairo staking contract.
pub struct CairoStakingContract {
    chain_info: ChainInfo,
    contract_address: ContractAddress,
    state_reader_factory: Arc<dyn StateReaderFactory>,
}

impl CairoStakingContract {
    pub fn new(
        chain_info: ChainInfo,
        contract_address: ContractAddress,
        state_reader_factory: Arc<dyn StateReaderFactory>,
    ) -> Self {
        Self { chain_info, contract_address, state_reader_factory }
    }

    fn call_view(&self, entry_point: &str, calldata: Calldata) -> StakingContractResult<Retdata> {
        let state_reader = self.state_reader_factory.create();

        let block_info = state_reader.get_block_info()?;
        let block_context = BlockContext::new(
            block_info,
            self.chain_info.clone(),
            VersionedConstants::latest_constants().clone(),
            BouncerConfig::max(),
        );

        let call_info = call_view_entry_point(
            state_reader,
            Arc::new(block_context),
            self.contract_address,
            entry_point,
            calldata,
        )?;
        Ok(call_info.execution.retdata)
    }
}

#[async_trait]
impl StakingContract for CairoStakingContract {
    async fn get_stakers(&self, epoch: u64) -> StakingContractResult<Vec<Staker>> {
        let calldata = Calldata::from(vec![Felt::from(epoch)]);
        let retdata = self.call_view(GET_STAKERS_ENTRY_POINT, calldata)?;

        // Filter out stakers that don't have a public key.
        let stakers = ContractStaker::from_retdata_many(retdata)?
            .into_iter()
            .filter_map(|contract_staker| {
                contract_staker.public_key.map(|_| Staker::from(&contract_staker))
            })
            .collect();

        Ok(stakers)
    }

    async fn get_current_epoch(&self) -> StakingContractResult<Epoch> {
        let retdata = self.call_view(GET_CURRENT_EPOCH_DATA_ENTRY_POINT, Calldata::from(vec![]))?;
        Ok(Epoch::try_from(retdata)?)
    }

    async fn get_previous_epoch(&self) -> StakingContractResult<Option<Epoch>> {
        todo!("Implement get_previous_epoch for CairoStakingContract")
    }
}
