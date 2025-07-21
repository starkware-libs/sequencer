use std::collections::BTreeMap;
use std::sync::Arc;

use apollo_consensus::types::Round;
use apollo_state_sync_types::communication::SharedStateSyncClient;
use async_trait::async_trait;
use blockifier::execution::entry_point::call_view_entry_point;
use blockifier::state::state_api::StateReader;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ContractAddress;
use starknet_api::transaction::fields::Calldata;
use starknet_types_core::felt::Felt;

use crate::committee_provider::{
    Committee,
    CommitteeProvider,
    CommitteeProviderError,
    CommitteeProviderResult,
    ExecutionContext,
};
use crate::contract_types::Staker;
use crate::utils::BlockRandomGenerator;

pub type StakerSet = Vec<Staker>;

const GET_STAKERS_ENTRY_POINT: &str = "get_stakers";
const EPOCH_LENGTH: u64 = 100; // Number of heights in an epoch.

#[cfg(test)]
#[path = "staking_manager_test.rs"]
mod staking_manager_test;

// TODO(Dafna): implement SerializeConfig and Validate for this struct. Specifically, the Validate
// should check that proposer_prediction_window_in_heights >= STORED_BLOCK_HASH_BUFFER.
pub struct StakingManagerConfig {
    pub staking_contract_address: ContractAddress,
    pub max_cached_epochs: usize,

    // The desired number of committee members to select from the available stakers.
    // If there are fewer stakers than `committee_size`, a smaller committee will be selected.
    pub committee_size: usize,

    // Defines how many heights in advance the proposer can be predicted.
    // While the exact identity may depend on staker prediction constraints,
    // the proposer selection logic becomes deterministic at this offset.
    pub proposer_prediction_window_in_heights: u64,
}

struct CommitteeData {
    committee_members: Arc<Committee>,
    cumulative_weights: Vec<u128>,
    total_weight: u128,
}

// Holds committee data for the highest known epochs, limited in size by `capacity``.
struct CommitteeDataCache {
    // The maximum number of epochs to cache.
    capacity: usize,
    // A map of epoch to the epoch's data.
    cache: BTreeMap<u64, Arc<CommitteeData>>,
}

// Responsible for fetching and storing the committee at a given epoch.
// The committee is a subset of nodes (proposer and validators) that are selected to participate in
// the consensus at a given epoch, responsible for proposing blocks and voting on them.
pub struct StakingManager {
    committee_data_cache: CommitteeDataCache,
    random_generator: Box<dyn BlockRandomGenerator>,
    config: StakingManagerConfig,
}

impl CommitteeDataCache {
    pub fn new(capacity: usize) -> Self {
        Self { capacity, cache: BTreeMap::new() }
    }

    pub fn get(&self, epoch: u64) -> Option<&Arc<CommitteeData>> {
        self.cache.get(&epoch)
    }

    pub fn insert(&mut self, epoch: u64, data: Arc<CommitteeData>) {
        self.cache.insert(epoch, data);
        if self.cache.len() > self.capacity {
            self.cache.pop_first();
        }
    }
}

impl StakingManager {
    pub fn new(
        random_generator: Box<dyn BlockRandomGenerator>,
        config: StakingManagerConfig,
    ) -> Self {
        Self {
            committee_data_cache: CommitteeDataCache::new(config.max_cached_epochs),
            random_generator,
            config,
        }
    }

    // Returns the committee data for the given epoch.
    // If the data is not cached, it is fetched from the state and cached.
    fn committee_data_at_epoch<S: StateReader>(
        &mut self,
        epoch: u64,
        execution_context: ExecutionContext<S>,
    ) -> CommitteeProviderResult<Arc<CommitteeData>> {
        // Attempt to read from cache.
        if let Some(committee_data) = self.committee_data_cache.get(epoch) {
            return Ok(committee_data.clone());
        }

        // Otherwise, build the committee from state, and cache the result.
        let committee_data =
            Arc::new(self.fetch_and_build_committee_data(epoch, execution_context)?);
        self.committee_data_cache.insert(epoch, committee_data.clone());

        Ok(committee_data)
    }

    // Queries the state to fetch stakers for the given epoch and builds the full committee data.
    // This includes selecting the committee and preparing cumulative weights for proposer
    // selection.
    fn fetch_and_build_committee_data<S: StateReader>(
        &self,
        epoch: u64,
        execution_context: ExecutionContext<S>,
    ) -> CommitteeProviderResult<CommitteeData> {
        let call_info = call_view_entry_point(
            execution_context.state_reader,
            execution_context.block_context,
            self.config.staking_contract_address,
            GET_STAKERS_ENTRY_POINT,
            Calldata(vec![Felt::from(epoch)].into()),
        )?;

        let stakers = Staker::from_retdata_many(call_info.execution.retdata)?;
        let committee_members = self.select_committee(stakers);

        // Prepare the data needed for proposer selection.
        let cumulative_weights: Vec<u128> = committee_members
            .iter()
            .scan(0, |acc, staker| {
                *acc = u128::checked_add(*acc, staker.weight.0).expect("Total weight overflow.");
                Some(*acc)
            })
            .collect();
        let total_weight = *cumulative_weights.last().unwrap_or(&0);

        Ok(CommitteeData {
            committee_members: Arc::new(committee_members),
            cumulative_weights,
            total_weight,
        })
    }

    // Selects the committee from the provided stakers and ensures a canonical ordering.
    fn select_committee(&self, mut stakers: StakerSet) -> Committee {
        // Ensure a consistent and deterministic committee ordering.
        // This is important for proposer selection logic to be deterministic and consistent across
        // all nodes.
        stakers.sort_by_key(|staker| (staker.weight, staker.address));

        // Take the top `committee_size` stakers by weight.
        stakers.into_iter().rev().take(self.config.committee_size).collect()
    }

    async fn proposer_randomness_block_hash(
        &self,
        current_block_number: BlockNumber,
        state_sync_client: SharedStateSyncClient,
    ) -> CommitteeProviderResult<Option<BlockHash>> {
        let randomness_source_block =
            current_block_number.0.checked_sub(self.config.proposer_prediction_window_in_heights);

        match randomness_source_block {
            None => {
                Ok(None) // Not enough history to look back; return None.
            }
            Some(block_number) => {
                let block_hash =
                    state_sync_client.get_block_hash(BlockNumber(block_number)).await?;
                Ok(Some(block_hash))
            }
        }
    }

    // Chooses a proposer from the committee using a weighted random selection.
    // The selection is based on the provided random value, where a staker's chance of selection is
    // proportional to its weight.
    // Note: the random value must be in the range [0, committee_data.total_weight).
    fn choose_proposer<'a>(
        &self,
        committee_data: &'a CommitteeData,
        random: u128,
    ) -> CommitteeProviderResult<&'a Staker> {
        if committee_data.committee_members.is_empty() {
            return Err(CommitteeProviderError::EmptyCommittee);
        }

        let total_weight = committee_data.total_weight;
        assert!(
            random < committee_data.total_weight,
            "Invalid random value {random}: exceeds total weight limit of {total_weight}."
        );

        // Iterates over stakers and selects staker `i` if `random < cumulative_weights[i]`.
        // Each staker occupies a range of values proportional to their weight, defined as:
        //     [cumulative_weights[i - 1], cumulative_weights[i])
        // Since we iterate in order, the first staker whose cumulative weight exceeds `random`
        // is the one whose range contains it.
        for (i, cum_weight) in committee_data.cumulative_weights.iter().enumerate() {
            if random < *cum_weight {
                return committee_data.committee_members.get(i).ok_or_else(|| {
                    panic!(
                        "Inconsistent committee data; cumulative_weights and committee_members \
                         are not the same length."
                    )
                });
            }
        }

        // We should never reach this point.
        panic!("Inconsistent committee data; cumulative_weights inconsistent with total weight.")
    }
}

#[async_trait]
impl CommitteeProvider for StakingManager {
    fn get_committee<S: StateReader>(
        &mut self,
        epoch: u64,
        execution_context: ExecutionContext<S>,
    ) -> CommitteeProviderResult<Arc<Committee>> {
        let committee_data = self.committee_data_at_epoch(epoch, execution_context)?;
        Ok(committee_data.committee_members.clone())
    }

    async fn get_proposer<S: StateReader + Send>(
        &mut self,
        height: BlockNumber,
        round: Round,
        execution_context: ExecutionContext<S>,
    ) -> CommitteeProviderResult<ContractAddress> {
        // Try to get the hash of the block used for proposer selection randomness.
        let block_hash = self
            .proposer_randomness_block_hash(height, execution_context.state_sync_client.clone())
            .await?;

        // Get the committee for the epoch this height belongs to.
        let epoch = height.0 / EPOCH_LENGTH; // TODO(Dafna): export to a utility function.
        let committee_data = self.committee_data_at_epoch(epoch, execution_context)?;

        // Generate a pseudorandom value in the range [0, total_weight) based on the height, round,
        // and block hash.
        let random_value =
            self.random_generator.generate(height, round, block_hash, committee_data.total_weight);

        // Select a proposer from the committee using the generated random.
        let proposer = self.choose_proposer(&committee_data, random_value)?;
        Ok(proposer.address)
    }
}
