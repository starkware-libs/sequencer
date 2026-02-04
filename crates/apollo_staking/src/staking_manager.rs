use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};

use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_protobuf::consensus::Round;
use apollo_staking_config::config::{
    find_config_for_epoch,
    StakersConfig,
    StakingManagerConfig,
    StakingManagerDynamicConfig,
    StakingManagerStaticConfig,
};
use apollo_state_sync_types::communication::SharedStateSyncClient;
use async_trait::async_trait;
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ContractAddress;
use static_assertions::const_assert;
use tracing::warn;

use crate::committee_provider::{
    Committee,
    CommitteeProvider,
    CommitteeProviderError,
    CommitteeProviderResult,
    Staker,
};
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
    // Eligible proposers in canonical committee order (by weight descending).
    eligible_proposers: Vec<ContractAddress>,
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
    // TODO(Dafna): Define an EpochId type.
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
    committee_data_cache: Mutex<CommitteeDataCache>,

    // Caches the current epoch fetched from the state.
    cached_epoch: Mutex<Option<Epoch>>,

    random_generator: Box<dyn BlockRandomGenerator>,
    static_config: StakingManagerStaticConfig,
    dynamic_config: RwLock<StakingManagerDynamicConfig>,
    config_manager_client: Option<SharedConfigManagerClient>,
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
        config_manager_client: Option<SharedConfigManagerClient>,
    ) -> Self {
        Self {
            staking_contract,
            state_sync_client,
            committee_data_cache: Mutex::new(CommitteeDataCache::new(
                config.static_config.max_cached_epochs,
            )),
            cached_epoch: Mutex::new(None),
            random_generator,
            static_config: config.static_config,
            dynamic_config: RwLock::new(config.dynamic_config),
            config_manager_client,
        }
    }

    async fn update_dynamic_config(&self) {
        let Some(client) = &self.config_manager_client else {
            return;
        };

        match client.get_staking_manager_dynamic_config().await {
            Ok(new_config) => {
                let mut dynamic_config = self.dynamic_config.write().expect("RwLock poisoned");
                *dynamic_config = new_config;
            }
            Err(error) => {
                warn!("Failed to fetch staking manager dynamic config: {error}");
            }
        }
    }

    // Returns the committee data for the given epoch.
    // If the data is not cached, it is fetched from the state and cached.
    async fn committee_data_at_height(
        &self,
        height: BlockNumber,
    ) -> CommitteeProviderResult<Arc<CommitteeData>> {
        let epoch = self.epoch_at_height(height).await?;

        // Attempt to read from cache.
        {
            let cache = self.committee_data_cache.lock().expect("Mutex poisoned");
            if let Some(committee_data) = cache.get(epoch) {
                return Ok(committee_data.clone());
            }
        }

        // Update dynamic config to ensure we have the latest stakers config.
        self.update_dynamic_config().await;

        // Otherwise, build the committee from state, and cache the result.
        let committee_data = Arc::new(self.fetch_and_build_committee_data(epoch).await?);

        // Cache the result.
        let mut cache = self.committee_data_cache.lock().expect("Mutex poisoned");
        cache.insert(epoch, committee_data.clone());

        Ok(committee_data)
    }

    // Queries the state to fetch stakers for the given epoch and builds the full committee data.
    // This includes selecting the committee and preparing cumulative weights for proposer
    // selection, as well as calculating eligible proposers.
    async fn fetch_and_build_committee_data(
        &self,
        epoch: u64,
    ) -> CommitteeProviderResult<CommitteeData> {
        let contract_stakers = self.staking_contract.get_stakers(epoch).await?;
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

        let eligible_proposers = self.calculate_eligible_proposers(&committee_members, epoch);

        Ok(CommitteeData {
            committee_members: Arc::new(committee_members),
            cumulative_weights,
            total_weight,
            eligible_proposers,
        })
    }

    // Selects the committee from the provided stakers and ensures a canonical ordering.
    fn select_committee(&self, mut stakers: StakerSet) -> Committee {
        // Ensure a consistent and deterministic committee ordering.
        // This is important for proposer selection logic to be deterministic and consistent across
        // all nodes.
        stakers.sort_by_key(|staker| (staker.weight, staker.address));

        // Take the top `committee_size` stakers by weight.
        let committee_size = self.dynamic_config.read().expect("RwLock poisoned").committee_size;
        stakers.into_iter().rev().take(committee_size).collect()
    }

    async fn proposer_randomness_block_hash(
        &self,
        current_block_number: BlockNumber,
    ) -> CommitteeProviderResult<Option<BlockHash>> {
        let randomness_source_block = current_block_number
            .0
            .checked_sub(self.static_config.proposer_prediction_window_in_heights);

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

    // Filters the committee to only include stakers eligible to propose.
    // Eligibility is determined by the `can_propose` field in the corresponding ConfiguredStaker.
    // Returns a vector of addresses in the committee's canonical order (by weight descending).
    fn calculate_eligible_proposers(
        &self,
        committee: &Committee,
        epoch: u64,
    ) -> Vec<ContractAddress> {
        // Find the stakers config for the given epoch
        let dynamic_config = self.dynamic_config.read().expect("RwLock poisoned");
        let config_entry = find_config_for_epoch(&dynamic_config.stakers_config, epoch);

        let Some(StakersConfig { stakers, .. }) = config_entry else {
            warn!("No stakers config found for epoch {epoch}, returning empty list.");
            return Vec::new();
        };

        let eligible_stakers: HashSet<ContractAddress> = stakers
            .iter()
            .filter(|staker| staker.can_propose)
            .map(|staker| staker.address)
            .collect();

        // Iterate over the committee to build the list of eligible proposers, keeping the order
        // like in the committee.
        committee
            .iter()
            .filter(|staker| eligible_stakers.contains(&staker.address))
            .map(|staker| staker.address)
            .collect()
    }

    fn try_resolve_epoch_id(&self, epoch: &Epoch, height: BlockNumber) -> Option<u64> {
        if epoch.contains(height) {
            Some(epoch.epoch_id)
        } else if epoch.within_next_epoch_min_bounds(height) {
            Some(epoch.epoch_id + 1)
        } else {
            None
        }
    }

    async fn epoch_at_height(&self, height: BlockNumber) -> CommitteeProviderResult<u64> {
        // Check if we can resolve the epoch from the cache.
        {
            let cached_epoch_guard = self.cached_epoch.lock().expect("Mutex poisoned");
            if let Some(epoch_id) = cached_epoch_guard
                .as_ref()
                .and_then(|epoch| self.try_resolve_epoch_id(epoch, height))
            {
                return Ok(epoch_id);
            }
        }

        // Otherwise, fetch the epoch from the state and cache the result.
        let epoch = self.staking_contract.get_current_epoch().await?;

        let mut cached_epoch_guard = self.cached_epoch.lock().expect("Mutex poisoned");
        *cached_epoch_guard = Some(epoch);

        self.try_resolve_epoch_id(cached_epoch_guard.as_ref().unwrap(), height)
            .ok_or(CommitteeProviderError::InvalidHeight { height })
    }
}

#[async_trait]
impl CommitteeProvider for StakingManager {
    // Returns the committee for the epoch at the given height.
    // The height must be within the bounds of the current epoch, or the next epoch's min bounds
    // (see `MIN_EPOCH_LENGTH`).
    async fn get_committee(&self, height: BlockNumber) -> CommitteeProviderResult<Arc<Committee>> {
        let committee_data = self.committee_data_at_height(height).await?;
        Ok(committee_data.committee_members.clone())
    }

    // Returns the address of the proposer for the specified height and round.
    // The proposer is chosen from the committee corresponding to the epoch of the given height.
    // Selection is based on a deterministic random number derived from the height, round,
    // and the hash of a past block — offset by `config.proposer_prediction_window`.
    async fn get_proposer(
        &self,
        height: BlockNumber,
        round: Round,
    ) -> CommitteeProviderResult<ContractAddress> {
        // Try to get the hash of the block used for proposer selection randomness.
        let block_hash = self.proposer_randomness_block_hash(height).await?;

        // Get the committee for the epoch this height belongs to.
        let committee_data = self.committee_data_at_height(height).await?;

        // Generate a pseudorandom value in the range [0, total_weight) based on the height, round,
        // and block hash.
        let random_value =
            self.random_generator.generate(height, round, block_hash, committee_data.total_weight);

        // Select a proposer from the committee using the generated random.
        let proposer = self.choose_proposer(&committee_data, random_value)?;
        Ok(proposer.address)
    }

    // Returns the address of the actual proposer using round-robin selection from eligible
    // stakers. Unlike `get_proposer` which uses weighted random selection, this method filters the
    // committee based on the `can_propose` configuration and uses deterministic round-robin.
    async fn get_actual_proposer(
        &self,
        height: BlockNumber,
        round: Round,
    ) -> CommitteeProviderResult<ContractAddress> {
        // Get the committee data for the epoch this height belongs to.
        let committee_data = self.committee_data_at_height(height).await?;

        assert!(
            !committee_data.eligible_proposers.is_empty(),
            "There should be at least one eligible proposer."
        );

        let height_usize: usize = height.0.try_into().expect("Cannot convert height to usize");
        let round_usize: usize = round.try_into().expect("Cannot convert round to usize");

        // Use round-robin selection: (height + round) % eligible_count
        let i = (height_usize + round_usize) % committee_data.eligible_proposers.len();
        Ok(committee_data.eligible_proposers[i])
    }
}
