use std::collections::BTreeMap;
use std::sync::Arc;

use apollo_consensus::types::Round;
use apollo_state_sync_types::communication::SharedStateSyncClient;
use async_trait::async_trait;
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ContractAddress;
use static_assertions::const_assert;

use crate::committee_provider::{
    Committee,
    CommitteeProvider,
    CommitteeProviderError,
    CommitteeProviderResult,
    Staker,
};
use crate::config::StakingManagerConfig;
use crate::staking_contract::StakingContract;
use crate::utils::BlockRandomGenerator;

pub type StakerSet = Vec<Staker>;

#[cfg(test)]
#[path = "staking_manager_test.rs"]
mod staking_manager_test;

// The minimum number of blocks in an epoch. This is used to anticipate if a
// height falls within the next epoch, even when the exact epoch length is unknown.
//
// CONSTRAINT: Must be ≥ `STORED_BLOCK_HASH_BUFFER` - the maximum StateSync lag.
// A smaller value could cause the consensus tip to advance beyond our knowledge of the next epoch,
// resulting in a failure to retrieve the committee.
const MIN_EPOCH_LENGTH: u64 = 10;
const_assert!(MIN_EPOCH_LENGTH >= STORED_BLOCK_HASH_BUFFER);

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

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Epoch {
    pub(crate) epoch_id: u64,
    pub(crate) start_block: BlockNumber,
    pub(crate) epoch_length: u64,
}

fn range_contains(height: BlockNumber, start: BlockNumber, length: u64) -> bool {
    height >= start && height.0 < start.0 + length
}

impl Epoch {
    fn contains(&self, height: BlockNumber) -> bool {
        range_contains(height, self.start_block, self.epoch_length)
    }

    fn within_next_epoch_min_bounds(&self, height: BlockNumber) -> bool {
        let next_epoch_start_block = BlockNumber(self.start_block.0 + self.epoch_length);
        range_contains(height, next_epoch_start_block, MIN_EPOCH_LENGTH)
    }
}

// Responsible for fetching and storing the committee at a given epoch.
// The committee is a subset of nodes (proposer and validators) that are selected to participate in
// the consensus at a given epoch, responsible for proposing blocks and voting on them.
pub struct StakingManager {
    staking_contract: Arc<dyn StakingContract>,
    state_sync_client: SharedStateSyncClient,
    committee_data_cache: CommitteeDataCache,

    // Caches the current epoch fetched from the state.
    cached_epoch: Option<Epoch>,

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
        staking_contract: Arc<dyn StakingContract>,
        state_sync_client: SharedStateSyncClient,
        random_generator: Box<dyn BlockRandomGenerator>,
        config: StakingManagerConfig,
    ) -> Self {
        Self {
            staking_contract,
            state_sync_client,
            committee_data_cache: CommitteeDataCache::new(config.max_cached_epochs),
            cached_epoch: None,
            random_generator,
            config,
        }
    }

    // Returns the committee data for the given epoch.
    // If the data is not cached, it is fetched from the state and cached.
    fn committee_data_at_epoch(
        &mut self,
        epoch: u64,
    ) -> CommitteeProviderResult<Arc<CommitteeData>> {
        // Attempt to read from cache.
        if let Some(committee_data) = self.committee_data_cache.get(epoch) {
            return Ok(committee_data.clone());
        }

        // Otherwise, build the committee from state, and cache the result.
        let committee_data = Arc::new(self.fetch_and_build_committee_data(epoch)?);
        self.committee_data_cache.insert(epoch, committee_data.clone());

        Ok(committee_data)
    }

    // Queries the state to fetch stakers for the given epoch and builds the full committee data.
    // This includes selecting the committee and preparing cumulative weights for proposer
    // selection.
    fn fetch_and_build_committee_data(&self, epoch: u64) -> CommitteeProviderResult<CommitteeData> {
        let contract_stakers = self.staking_contract.get_stakers(epoch)?;
        let committee_members = self.select_committee(contract_stakers);

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
    ) -> CommitteeProviderResult<Option<BlockHash>> {
        let randomness_source_block =
            current_block_number.0.checked_sub(self.config.proposer_prediction_window_in_heights);

        match randomness_source_block {
            None => {
                Ok(None) // Not enough history to look back; return None.
            }
            Some(block_number) => {
                let block_hash =
                    self.state_sync_client.get_block_hash(BlockNumber(block_number)).await?;
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

    fn epoch_at_height(&mut self, height: BlockNumber) -> CommitteeProviderResult<u64> {
        if self.cached_epoch.as_ref().is_none_or(|epoch| !epoch.contains(height)) {
            // Fetch the epoch from the state.
            let epoch = self.staking_contract.get_current_epoch()?;
            self.cached_epoch = Some(epoch);
        }

        let cached_epoch = self.cached_epoch.as_ref().expect("cached_epoch should be set");

        if cached_epoch.contains(height) {
            Ok(cached_epoch.epoch_id)
        } else if cached_epoch.within_next_epoch_min_bounds(height) {
            Ok(cached_epoch.epoch_id + 1)
        } else {
            Err(CommitteeProviderError::InvalidHeight { height })
        }
    }
}

#[async_trait]
impl CommitteeProvider for StakingManager {
    // Returns the committee for the epoch at the given height.
    // The height must be within the bounds of the current epoch, or the next epoch's min bounds
    // (see `MIN_EPOCH_LENGTH`).
    fn get_committee(&mut self, height: BlockNumber) -> CommitteeProviderResult<Arc<Committee>> {
        let epoch = self.epoch_at_height(height)?;

        let committee_data = self.committee_data_at_epoch(epoch)?;
        Ok(committee_data.committee_members.clone())
    }

    // Returns the address of the proposer for the specified height and round.
    // The proposer is chosen from the committee corresponding to the epoch of the given height.
    // Selection is based on a deterministic random number derived from the height, round,
    // and the hash of a past block — offset by `config.proposer_prediction_window`.
    async fn get_proposer(
        &mut self,
        height: BlockNumber,
        round: Round,
    ) -> CommitteeProviderResult<ContractAddress> {
        // Try to get the hash of the block used for proposer selection randomness.
        let block_hash = self.proposer_randomness_block_hash(height).await?;

        // Get the committee for the epoch this height belongs to.
        let epoch = self.epoch_at_height(height)?;
        let committee_data = self.committee_data_at_epoch(epoch)?;

        // Generate a pseudorandom value in the range [0, total_weight) based on the height, round,
        // and block hash.
        let random_value =
            self.random_generator.generate(height, round, block_hash, committee_data.total_weight);

        // Select a proposer from the committee using the generated random.
        let proposer = self.choose_proposer(&committee_data, random_value)?;
        Ok(proposer.address)
    }
}
