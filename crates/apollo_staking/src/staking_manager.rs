use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, RwLock};

use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_protobuf::consensus::Round;
use apollo_staking_config::config::{
    get_config_for_epoch,
    ConfiguredStaker,
    StakingManagerConfig,
    StakingManagerDynamicConfig,
};
use apollo_state_sync_types::communication::SharedStateSyncClient;
use async_trait::async_trait;
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::core::ContractAddress;
use static_assertions::const_assert;
use tokio::sync::Mutex;
use tracing::warn;

use crate::committee_provider::{
    CommitteeError,
    CommitteeProvider,
    CommitteeProviderError,
    CommitteeProviderResult,
    CommitteeResult,
    CommitteeTrait,
    StakerSet,
};
use crate::staking_contract::StakingContract;
use crate::utils::BlockRandomGenerator;

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

struct Committee {
    committee_members: Arc<StakerSet>,
    cumulative_weights: Vec<u128>,
    total_weight: u128,
    // Eligible proposers in canonical committee order (by weight descending).
    eligible_proposers: Vec<ContractAddress>,
    // Block hash used for proposer selection randomness.
    // For epoch N, this is the hash of the first block in epoch N-1.
    randomness_block_hash: Option<BlockHash>,
    // Random generator for weighted proposer selection.
    random_generator: Arc<dyn BlockRandomGenerator>,
    // Stores the most recently requested proposer address, keyed by height and round.
    proposer_cache: std::sync::Mutex<Option<(BlockNumber, Round, ContractAddress)>>,
    // If true, get_proposer will use the same deterministic round-robin selection as
    // get_actual_proposer.
    use_only_actual_proposer_selection: bool,
}

// Holds committee data for the highest known epochs, limited in size by `capacity``.
struct CommitteeCache {
    // The maximum number of epochs to cache.
    capacity: usize,
    // A map of epoch to the epoch's data.
    cache: BTreeMap<u64, Arc<Committee>>,
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

struct EpochCache {
    current: Option<Epoch>,
    previous: Option<Epoch>,
}

impl EpochCache {
    fn new() -> Self {
        Self { current: None, previous: None }
    }

    // Tries to resolve the epoch ID by looking only at the current epoch. Past heights are not
    // supported.
    fn try_resolve_epoch_id(&self, height: BlockNumber) -> Option<u64> {
        let current = self.current.as_ref()?;
        if current.contains(height) {
            Some(current.epoch_id)
        } else if current.within_next_epoch_min_bounds(height) {
            Some(current.epoch_id + 1)
        } else {
            None
        }
    }

    // Update the cached epochs with the current and previous epochs from the staking contract.
    async fn update_epochs(
        &mut self,
        staking_contract: Arc<dyn StakingContract>,
    ) -> CommitteeProviderResult<()> {
        let latest_epoch = staking_contract.get_current_epoch().await?;

        if self.current.as_ref().is_some_and(|current| current.epoch_id == latest_epoch.epoch_id) {
            // No change in the current epoch, no need to update anything.
            return Ok(());
        }

        // Determine whether to reuse the current epoch as the previous, or explicitly fetch it from
        // the contract.
        let is_sequential = self
            .current
            .as_ref()
            .is_some_and(|current| current.epoch_id + 1 == latest_epoch.epoch_id);
        let previous_epoch = if is_sequential {
            self.current.take()
        } else {
            staking_contract.get_previous_epoch().await?
        };

        // Update the cache with the fetched epochs.
        self.current = Some(latest_epoch);
        self.previous = previous_epoch;

        Ok(())
    }

    // Returns the Epoch object for the given epoch ID.
    // If the epoch is not cached, returns None.
    fn get_epoch(&self, epoch_id: u64) -> Option<Epoch> {
        let matches = |epoch: &Option<Epoch>| -> Option<Epoch> {
            epoch.as_ref().filter(|epoch| epoch.epoch_id == epoch_id).cloned()
        };
        matches(&self.current).or_else(|| matches(&self.previous))
    }
}

// Responsible for fetching and storing the committee at a given epoch.
// The committee is a subset of nodes (proposer and validators) that are selected to participate in
// the consensus at a given epoch, responsible for proposing blocks and voting on them.
pub struct StakingManager {
    staking_contract: Arc<dyn StakingContract>,
    state_sync_client: SharedStateSyncClient,
    committee_cache: Mutex<CommitteeCache>,

    // Caches the current and previous epochs fetched from the state.
    epoch_cache: Mutex<EpochCache>,

    random_generator: Arc<dyn BlockRandomGenerator>,
    dynamic_config: RwLock<StakingManagerDynamicConfig>,
    config_manager_client: Option<SharedConfigManagerClient>,
    use_only_actual_proposer_selection: bool,
}

impl CommitteeCache {
    pub fn new(capacity: usize) -> Self {
        Self { capacity, cache: BTreeMap::new() }
    }

    pub fn get(&self, epoch: u64) -> Option<&Arc<Committee>> {
        self.cache.get(&epoch)
    }

    pub fn insert(&mut self, epoch: u64, data: Arc<Committee>) {
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
        random_generator: Arc<dyn BlockRandomGenerator>,
        config: StakingManagerConfig,
        config_manager_client: Option<SharedConfigManagerClient>,
    ) -> Self {
        Self {
            staking_contract,
            state_sync_client,
            committee_cache: Mutex::new(CommitteeCache::new(
                config.static_config.max_cached_epochs,
            )),
            epoch_cache: Mutex::new(EpochCache::new()),
            random_generator,
            dynamic_config: RwLock::new(config.dynamic_config),
            config_manager_client,
            use_only_actual_proposer_selection: config
                .static_config
                .use_only_actual_proposer_selection,
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
    async fn committee_at_height(
        &self,
        height: BlockNumber,
    ) -> CommitteeProviderResult<Arc<Committee>> {
        let epoch = self.epoch_at_height(height).await?;

        // Attempt to read from cache.
        {
            let cache = self.committee_cache.lock().await;
            if let Some(committee) = cache.get(epoch) {
                return Ok(committee.clone());
            }
        }

        // Otherwise, build the committee from state, and cache the result.
        let committee = Arc::new(self.fetch_and_build_committee(epoch).await?);

        // Cache the result.
        let mut cache = self.committee_cache.lock().await;
        cache.insert(epoch, committee.clone());

        Ok(committee)
    }

    // Queries the state to fetch stakers for the given epoch and builds the full committee data.
    // This includes selecting the committee and preparing cumulative weights for proposer
    // selection, as well as calculating eligible proposers.
    async fn fetch_and_build_committee(&self, epoch: u64) -> CommitteeProviderResult<Committee> {
        // Update dynamic config to ensure we have the latest stakers config.
        self.update_dynamic_config().await;

        // Get the config to inject and use for committee building.
        // Clone it to avoid holding the lock across await.
        let dynamic_config = self.dynamic_config.read().expect("RwLock poisoned").clone();

        // Always use get_stakers_with_config - works for all implementations.
        let contract_stakers =
            self.staking_contract.get_stakers_with_config(epoch, &dynamic_config).await?;

        // Get the active committee config for this epoch (includes size and stakers).
        let active_config = get_config_for_epoch(
            &dynamic_config.default_committee,
            &dynamic_config.override_committee,
            epoch,
        )
        .clone();

        let committee_members =
            self.select_committee(contract_stakers, active_config.committee_size);

        // Prepare the data needed for proposer selection.
        let cumulative_weights: Vec<u128> = committee_members
            .iter()
            .scan(0, |acc, staker| {
                *acc = u128::checked_add(*acc, staker.weight.0).expect("Total weight overflow.");
                Some(*acc)
            })
            .collect();
        let total_weight = *cumulative_weights.last().unwrap_or(&0);

        let eligible_proposers =
            self.calculate_eligible_proposers(&committee_members, &active_config.stakers);

        // Calculate the randomness block hash for this epoch.
        let randomness_block_hash = self.proposer_randomness_block_hash(epoch).await?;

        Ok(Committee {
            committee_members: Arc::new(committee_members),
            cumulative_weights,
            total_weight,
            eligible_proposers,
            randomness_block_hash,
            random_generator: self.random_generator.clone(),
            proposer_cache: std::sync::Mutex::new(None),
            use_only_actual_proposer_selection: self.use_only_actual_proposer_selection,
        })
    }

    // Selects the committee from the provided stakers and ensures a canonical ordering.
    fn select_committee(&self, mut stakers: StakerSet, committee_size: usize) -> StakerSet {
        // Ensure a consistent and deterministic committee ordering.
        // This is important for proposer selection logic to be deterministic and consistent across
        // all nodes.
        stakers.sort_by_key(|staker| (staker.weight, staker.address));

        // Take the top `committee_size` stakers by weight.
        stakers.into_iter().rev().take(committee_size).collect()
    }

    // Returns the block hash used for proposer selection randomness for the given epoch.
    // For epoch N, this returns the hash of the first block in epoch N-1.
    // Returns None for epoch 0 (first epoch has no previous epoch).
    // Assumes the epoch cache is synced at this point.
    async fn proposer_randomness_block_hash(
        &self,
        epoch: u64,
    ) -> CommitteeProviderResult<Option<BlockHash>> {
        // First epoch has no previous epoch.
        if epoch == 0 {
            return Ok(None);
        }

        let previous_epoch_id = epoch - 1;

        // Get the previous epoch from the cache.
        let prev_epoch = {
            let cache = self.epoch_cache.lock().await;
            cache.get_epoch(previous_epoch_id)
        };

        // If the cache is missing the previous epoch, treat it as an error.
        let prev_epoch = prev_epoch
            .ok_or(CommitteeProviderError::MissingInformation { epoch_id: previous_epoch_id })?;

        // Get the hash of the first block in the previous epoch.
        let block_hash = self.state_sync_client.get_block_hash(prev_epoch.start_block).await?;
        Ok(Some(block_hash))
    }

    // Filters the committee to only include stakers eligible to propose.
    // Eligibility is determined by the `can_propose` field in the corresponding ConfiguredStaker.
    // Returns a vector of addresses in the committee's canonical order (by weight descending).
    fn calculate_eligible_proposers(
        &self,
        committee_members: &StakerSet,
        config_stakers: &[ConfiguredStaker],
    ) -> Vec<ContractAddress> {
        let eligible_stakers: HashSet<ContractAddress> = config_stakers
            .iter()
            .filter(|staker| staker.can_propose)
            .map(|staker| staker.address)
            .collect();

        // Iterate over the committee to build the list of eligible proposers, keeping the order
        // like in the committee.
        committee_members
            .iter()
            .filter(|staker| eligible_stakers.contains(&staker.address))
            .map(|staker| staker.address)
            .collect()
    }

    // Returns the epoch ID for the given height.
    // Syncs the epoch cache from state if the height cannot be resolved from cache.
    async fn epoch_at_height(&self, height: BlockNumber) -> CommitteeProviderResult<u64> {
        // Try to resolve the epoch from cache.
        let mut epoch_cache = self.epoch_cache.lock().await;
        if let Some(epoch_id) = epoch_cache.try_resolve_epoch_id(height) {
            return Ok(epoch_id);
        }

        // Otherwise, sync the epochs from the state.
        epoch_cache.update_epochs(self.staking_contract.clone()).await?;

        epoch_cache
            .try_resolve_epoch_id(height)
            .ok_or(CommitteeProviderError::InvalidHeight { height })
    }
}

impl CommitteeTrait for Committee {
    fn get_proposer(&self, height: BlockNumber, round: Round) -> CommitteeResult<ContractAddress> {
        if self.use_only_actual_proposer_selection {
            return Ok(self.get_actual_proposer(height, round));
        }

        if self.committee_members.is_empty() {
            return Err(CommitteeError::EmptyCommittee);
        }

        // Check if we can return from cache.
        if let Some(address) = self
            .proposer_cache
            .lock()
            .expect("Mutex poisoned")
            .filter(|(h, r, _)| *h == height && *r == round)
            .map(|(_, _, addr)| addr)
        {
            return Ok(address);
        }

        // Generate a pseudorandom value in the range [0, total_weight) based on the height, round,
        // and block hash.
        let random_value = self.random_generator.generate(
            height,
            round,
            self.randomness_block_hash,
            self.total_weight,
        );

        // Select a proposer from the committee using the generated random and update the cache.
        let proposer = self.choose_proposer(random_value);
        *self.proposer_cache.lock().expect("Mutex poisoned") = Some((height, round, proposer));

        Ok(proposer)
    }

    fn get_actual_proposer(&self, height: BlockNumber, round: Round) -> ContractAddress {
        assert!(
            !self.eligible_proposers.is_empty(),
            "There should be at least one eligible proposer."
        );

        let height_usize: usize = height.0.try_into().expect("Cannot convert height to usize");
        let round_usize: usize = round.try_into().expect("Cannot convert round to usize");

        // Use round-robin selection: (height + round) % eligible_count
        let i = (height_usize + round_usize) % self.eligible_proposers.len();
        self.eligible_proposers[i]
    }

    // Returns the list of committee members.
    fn members(&self) -> &StakerSet {
        &self.committee_members
    }
}

impl Committee {
    // Chooses a proposer from the committee using a weighted random selection.
    // The selection is based on the provided random value, where a staker's chance of selection is
    // proportional to its weight.
    // Note: the random value must be in the range [0, self.total_weight).
    fn choose_proposer(&self, random: u128) -> ContractAddress {
        let total_weight = self.total_weight;
        assert!(
            random < self.total_weight,
            "Invalid random value {random}: exceeds total weight limit of {total_weight}."
        );

        // Iterates over stakers and selects staker `i` if `random < cumulative_weights[i]`.
        // Each staker occupies a range of values proportional to their weight, defined as:
        //     [cumulative_weights[i - 1], cumulative_weights[i])
        // Since we iterate in order, the first staker whose cumulative weight exceeds `random`
        // is the one whose range contains it.
        for (i, cum_weight) in self.cumulative_weights.iter().enumerate() {
            if random < *cum_weight {
                return self
                    .committee_members
                    .get(i)
                    .expect(
                        "Inconsistent committee data; cumulative_weights and committee_members \
                         are not the same length.",
                    )
                    .address;
            }
        }

        // We should never reach this point.
        panic!("Inconsistent committee data; cumulative_weights inconsistent with total weight.")
    }
}

#[async_trait]
impl CommitteeProvider for StakingManager {
    // Returns the committee for the epoch at the given height.
    // The height must be within the bounds of the current epoch, or the next epoch's min bounds
    // (see `MIN_EPOCH_LENGTH`).
    async fn get_committee(
        &self,
        height: BlockNumber,
    ) -> CommitteeProviderResult<Arc<dyn CommitteeTrait>> {
        Ok(self.committee_at_height(height).await?)
    }
}
