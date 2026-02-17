use std::collections::HashSet;
use std::sync::Arc;

use apollo_config_manager_types::communication::MockConfigManagerClient;
use apollo_staking_config::config::{
    CommitteeConfig,
    ConfiguredStaker,
    StakingManagerConfig,
    StakingManagerDynamicConfig,
    StakingManagerStaticConfig,
};
use apollo_state_sync_types::communication::MockStateSyncClient;
use assert_matches::assert_matches;
use mockall::predicate::eq;
use mockall::TimesRange;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::staking::StakingWeight;
use starknet_types_core::felt::Felt;

use crate::committee_provider::{
    CommitteeError,
    CommitteeProvider,
    CommitteeProviderError,
    Staker,
    StakerSet,
};
use crate::staking_contract::MockStakingContract;
use crate::staking_manager::{Epoch, StakingManager, MIN_EPOCH_LENGTH};
use crate::utils::MockBlockRandomGenerator;

const STAKER_1: Staker = Staker {
    address: ContractAddress(PatriciaKey::from_hex_unchecked("0x1")),
    weight: StakingWeight(1000),
    public_key: Felt::ONE,
};
const STAKER_2: Staker = Staker {
    address: ContractAddress(PatriciaKey::from_hex_unchecked("0x2")),
    weight: StakingWeight(2000),
    public_key: Felt::TWO,
};
const STAKER_3: Staker = Staker {
    address: ContractAddress(PatriciaKey::from_hex_unchecked("0x3")),
    weight: StakingWeight(3000),
    public_key: Felt::THREE,
};
const STAKER_4: Staker = Staker {
    address: ContractAddress(PatriciaKey::from_hex_unchecked("0x4")),
    weight: StakingWeight(4000),
    public_key: Felt::from_raw([0, 0, 0, 4]),
};

const EPOCH_0: Epoch = Epoch { epoch_id: 0, start_block: BlockNumber(0), epoch_length: 100 };
const EPOCH_1: Epoch = Epoch { epoch_id: 1, start_block: BlockNumber(1), epoch_length: 100 };
const EPOCH_2: Epoch = Epoch { epoch_id: 2, start_block: BlockNumber(101), epoch_length: 100 };
const EPOCH_3: Epoch = Epoch { epoch_id: 3, start_block: BlockNumber(201), epoch_length: 100 };

const E1_H1: BlockNumber = EPOCH_1.start_block;
const E1_H2: BlockNumber = BlockNumber(EPOCH_1.start_block.0 + EPOCH_1.epoch_length - 1);
const E2_H1: BlockNumber = EPOCH_2.start_block;
const E2_H2: BlockNumber = BlockNumber(EPOCH_2.start_block.0 + MIN_EPOCH_LENGTH + 1);
const E3_H1: BlockNumber = EPOCH_3.start_block;

fn test_config_with_committee_size(committee_size: usize) -> StakingManagerDynamicConfig {
    let mut config = StakingManagerDynamicConfig::default();
    config.default_committee.committee_size = committee_size;

    let stakers = [STAKER_1, STAKER_2, STAKER_3];
    let configured_stakers: Vec<ConfiguredStaker> =
        stakers.iter().map(|s| create_configured_staker(s, true)).collect();

    config.default_committee.stakers = configured_stakers;

    config
}

#[fixture]
fn contract() -> MockStakingContract {
    MockStakingContract::new()
}

#[fixture]
fn default_config() -> StakingManagerConfig {
    StakingManagerConfig {
        dynamic_config: test_config_with_committee_size(10),
        static_config: StakingManagerStaticConfig {
            max_cached_epochs: 10,
            use_only_actual_proposer_selection: false,
        },
    }
}

fn set_stakers(contract: &mut MockStakingContract, epoch: Epoch, stakers: StakerSet) {
    contract.expect_get_stakers().with(eq(epoch.epoch_id)).returning(move |_| Ok(stakers.clone()));
}

fn set_current_epoch(contract: &mut MockStakingContract, epoch: Epoch) {
    contract.expect_get_current_epoch().times(1).returning(move || Ok(epoch.clone()));
}

fn set_current_epoch_with_times(
    contract: &mut MockStakingContract,
    epoch: Epoch,
    times: impl Into<TimesRange>,
) {
    contract.expect_get_current_epoch().times(times).returning(move || Ok(epoch.clone()));
}

fn set_previous_epoch(contract: &mut MockStakingContract, epoch: Option<Epoch>) {
    contract.expect_get_previous_epoch().times(1..).returning(move || Ok(epoch.clone()));
}

// Helper function to create a MockStateSyncClient that returns a block hash for any block.
fn create_state_sync_client_with_block_hash() -> MockStateSyncClient {
    let mut state_sync_client = MockStateSyncClient::new();
    state_sync_client
        .expect_get_block_hash()
        .returning(|_| Ok(starknet_api::block::BlockHash(Felt::ZERO)));
    state_sync_client
}

#[rstest]
#[case::no_stakers(vec![], vec![])]
#[case::single_staker(vec![STAKER_1], vec![STAKER_1])]
#[case::multiple_stakers_less_than_committee_size(vec![STAKER_1, STAKER_2], vec![STAKER_2, STAKER_1])]
#[case::multiple_stakers_equal_to_committee_size(vec![STAKER_1, STAKER_2, STAKER_3], vec![STAKER_3, STAKER_2, STAKER_1])]
#[case::multiple_stakers_greater_than_committee_size(vec![STAKER_1, STAKER_2, STAKER_3, STAKER_4], vec![STAKER_4, STAKER_3, STAKER_2])]
#[case::multiple_stakers_equal_weights(vec![STAKER_1, STAKER_2, STAKER_3, Staker { address: ContractAddress(PatriciaKey::from_hex_unchecked("0x0")), .. STAKER_1 }], vec![STAKER_3, STAKER_2, STAKER_1])]
#[tokio::test]
async fn get_committee_success(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
    #[case] stakers: StakerSet,
    #[case] expected_committee: StakerSet,
) {
    set_current_epoch(&mut contract, EPOCH_1);
    set_previous_epoch(&mut contract, Some(EPOCH_0));
    set_stakers(&mut contract, EPOCH_1, stakers);

    let committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(create_state_sync_client_with_block_hash()),
        Arc::new(MockBlockRandomGenerator::new()),
        StakingManagerConfig {
            dynamic_config: test_config_with_committee_size(3),
            ..default_config
        },
        None,
    );

    let committee = committee_manager.get_committee(E1_H1).await.unwrap();

    assert_eq!(*committee.members(), expected_committee);
}

#[rstest]
#[tokio::test]
async fn get_committee_cache(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
) {
    // Set contract expectations for the first query (Case 1).
    set_current_epoch(&mut contract, EPOCH_1);
    set_previous_epoch(&mut contract, Some(EPOCH_0));
    set_stakers(&mut contract, EPOCH_1, vec![STAKER_1]);
    set_stakers(&mut contract, EPOCH_2, vec![STAKER_2]);

    let mut config = default_config;
    config.static_config.max_cached_epochs = 1;
    let committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(create_state_sync_client_with_block_hash()),
        Arc::new(MockBlockRandomGenerator::new()),
        config,
        None,
    );

    // Case 1: Get committee for epoch 1. Cache miss – STAKER_1 fetched from contract.
    let committee = committee_manager.get_committee(E1_H1).await.unwrap();
    assert_eq!(*committee.members(), vec![STAKER_1]);

    // Case 2: Query epoch 1 again. Cache hit – STAKER_1 returned from cache.
    let committee = committee_manager.get_committee(E1_H1).await.unwrap();
    assert_eq!(*committee.members(), vec![STAKER_1]);

    // Case 3: Get committee for epoch 2. Cache miss – new state is fetched from the contract.
    let committee = committee_manager.get_committee(E2_H1).await.unwrap();
    assert_eq!(*committee.members(), vec![STAKER_2]);
}

#[rstest]
#[tokio::test]
async fn get_committee_for_next_epoch(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
) {
    set_current_epoch_with_times(&mut contract, EPOCH_1, 1..);
    set_previous_epoch(&mut contract, Some(EPOCH_0));

    // Set the stakers for the next epoch.
    set_stakers(&mut contract, EPOCH_2, vec![STAKER_1, STAKER_2]);

    let committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(create_state_sync_client_with_block_hash()),
        Arc::new(MockBlockRandomGenerator::new()),
        StakingManagerConfig {
            dynamic_config: test_config_with_committee_size(3),
            ..default_config
        },
        None,
    );

    // 1. Valid Query: E2_H1 falls within the next epoch's min bounds.
    let committee = committee_manager.get_committee(E2_H1).await.unwrap().members().clone();
    assert_eq!(committee.into_iter().collect::<HashSet<_>>(), HashSet::from([STAKER_1, STAKER_2]));

    // 2. Invalid Query: E2_H2 exceeds the min bounds of the next epoch.
    // Since the next epoch's length is not known at this point, we cannot know if this height
    // belongs to Epoch 2 or a future Epoch > 2.
    let err = committee_manager.get_committee(E2_H2).await.err().unwrap();
    assert_matches!(err, CommitteeProviderError::InvalidHeight { .. });
}

#[rstest]
#[tokio::test]
async fn get_committee_applies_dynamic_config_changes(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
) {
    set_current_epoch_with_times(&mut contract, EPOCH_1, 1..);
    set_previous_epoch(&mut contract, Some(EPOCH_0));
    set_stakers(&mut contract, EPOCH_1, vec![STAKER_1, STAKER_2, STAKER_3]);
    set_stakers(&mut contract, EPOCH_2, vec![STAKER_1, STAKER_2, STAKER_3]);

    let mut config_manager_client = MockConfigManagerClient::new();
    config_manager_client
        .expect_get_staking_manager_dynamic_config()
        .times(1)
        .return_once(|| Ok(test_config_with_committee_size(2)));
    config_manager_client
        .expect_get_staking_manager_dynamic_config()
        .times(1)
        .return_once(|| Ok(test_config_with_committee_size(1)));

    let committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(create_state_sync_client_with_block_hash()),
        Arc::new(MockBlockRandomGenerator::new()),
        default_config,
        Some(Arc::new(config_manager_client)),
    );

    let committee = committee_manager.get_committee(E1_H1).await.unwrap();
    assert_eq!(committee.members().len(), 2);

    let committee = committee_manager.get_committee(E2_H1).await.unwrap();
    assert_eq!(committee.members().len(), 1);
}

#[rstest]
#[case(9999, STAKER_1)]
#[case(9000, STAKER_1)]
#[case(8999, STAKER_2)]
#[case(7000, STAKER_2)]
#[case(6999, STAKER_3)]
#[case(4000, STAKER_3)]
#[case(3999, STAKER_4)]
#[case(0, STAKER_4)]
#[tokio::test]
async fn get_proposer_success(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
    #[case] random_value: u128,
    #[case] expected_proposer: Staker,
) {
    set_current_epoch(&mut contract, EPOCH_1);
    set_previous_epoch(&mut contract, Some(EPOCH_0));

    // The staker weights are 1000, 2000, 3000, and 4000, totaling 10,000.
    // Based on the cumulative weight ranges:
    // - Random values in [0–3999] → STAKER_4
    // - [4000–6999] → STAKER_3
    // - [7000–8999] → STAKER_2
    // - [9000–9999] → STAKER_1

    set_stakers(&mut contract, EPOCH_1, vec![STAKER_1, STAKER_2, STAKER_3, STAKER_4]);

    let mut state_sync_client = MockStateSyncClient::new();
    state_sync_client
        .expect_get_block_hash()
        .with(eq(EPOCH_0.start_block))
        .returning(|_| Ok(starknet_api::block::BlockHash(Felt::ZERO)));

    let mut random_generator = MockBlockRandomGenerator::new();
    random_generator.expect_generate().returning(move |_, _, _, _| random_value);

    let committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(state_sync_client),
        Arc::new(random_generator),
        default_config,
        None,
    );

    let committee = committee_manager.get_committee(E1_H1).await.unwrap();
    let proposer = committee.get_proposer(E1_H1, 0).unwrap();

    assert_eq!(proposer, expected_proposer.address);
}

#[rstest]
#[tokio::test]
async fn get_proposer_for_next_epoch(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
) {
    set_current_epoch_with_times(&mut contract, EPOCH_1, 2);
    set_previous_epoch(&mut contract, Some(EPOCH_0));
    set_stakers(&mut contract, EPOCH_2, vec![STAKER_1, STAKER_2, STAKER_3, STAKER_4]);

    let mut state_sync_client = MockStateSyncClient::new();
    state_sync_client
        .expect_get_block_hash()
        .with(eq(EPOCH_1.start_block))
        .returning(|_| Ok(starknet_api::block::BlockHash(Felt::ZERO)));

    let mut random_generator = MockBlockRandomGenerator::new();
    random_generator.expect_generate().returning(move |_, _, _, _| 0);

    let committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(state_sync_client),
        Arc::new(random_generator),
        default_config,
        None,
    );

    // Query a height in Epoch 2, that is within the min bounds of the next epoch.
    let committee = committee_manager.get_committee(E2_H1).await.unwrap();
    let proposer = committee.get_proposer(E2_H1, 0).unwrap();
    assert_eq!(proposer, STAKER_4.address);

    // Query a height in Epoch 2, that is outside the min bounds of the next epoch.
    let err = committee_manager.get_committee(E2_H2).await.err().unwrap();
    assert_matches!(err, CommitteeProviderError::InvalidHeight { .. });
}

#[rstest]
#[tokio::test]
async fn get_proposer_epoch_cache_updates(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
) {
    // Expectations for the first get_proposer call (no epoch is cached):
    // Initially set the contract current epoch to Epoch 1 and previous epoch to Epoch 0.
    set_current_epoch(&mut contract, EPOCH_1);
    contract.expect_get_previous_epoch().times(1).returning(|| Ok(Some(EPOCH_0)));
    set_stakers(&mut contract, EPOCH_1, vec![STAKER_1]);

    let mut state_sync_client = MockStateSyncClient::new();
    state_sync_client
        .expect_get_block_hash()
        .with(eq(EPOCH_0.start_block))
        .returning(|_| Ok(starknet_api::block::BlockHash(Felt::ZERO)));

    // Expectations for the second get_proposer call (epoch 1 and 0 should be cached):
    // Advance the contract current epoch to Epoch 3, and previous epoch to Epoch 2.
    set_current_epoch(&mut contract, EPOCH_3);
    contract.expect_get_previous_epoch().times(1).returning(|| Ok(Some(EPOCH_2)));
    set_stakers(&mut contract, EPOCH_3, vec![STAKER_3]);

    state_sync_client
        .expect_get_block_hash()
        .with(eq(EPOCH_2.start_block))
        .returning(|_| Ok(starknet_api::block::BlockHash(Felt::ZERO)));

    let mut random_generator = MockBlockRandomGenerator::new();
    random_generator.expect_generate().returning(move |_, _, _, _| 0);

    let committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(state_sync_client),
        Arc::new(random_generator),
        default_config,
        None,
    );

    // Query a height in Epoch 1, should return STAKER_1.
    let committee = committee_manager.get_committee(E1_H1).await.unwrap();
    let proposer = committee.get_proposer(E1_H1, 0).unwrap();
    assert_eq!(proposer, STAKER_1.address);

    // Query a height in Epoch 3, should return STAKER_3.
    let committee = committee_manager.get_committee(E3_H1).await.unwrap();
    let proposer = committee.get_proposer(E3_H1, 0).unwrap();
    assert_eq!(proposer, STAKER_3.address);
}

#[rstest]
#[tokio::test]
async fn get_proposer_empty_committee(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
) {
    set_current_epoch(&mut contract, EPOCH_1);
    set_previous_epoch(&mut contract, Some(EPOCH_0));
    set_stakers(&mut contract, EPOCH_1, vec![]);

    let mut state_sync_client = MockStateSyncClient::new();
    state_sync_client
        .expect_get_block_hash()
        .returning(|_| Ok(starknet_api::block::BlockHash(Felt::ZERO)));

    let mut random_generator = MockBlockRandomGenerator::new();
    random_generator.expect_generate().returning(move |_, _, _, _| 0);

    let committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(state_sync_client),
        Arc::new(random_generator),
        StakingManagerConfig {
            dynamic_config: test_config_with_committee_size(0),
            ..default_config
        },
        None,
    );

    let committee = committee_manager.get_committee(E1_H1).await.unwrap();
    assert_matches!(committee.get_proposer(E1_H1, 0).unwrap_err(), CommitteeError::EmptyCommittee);
}

#[rstest]
#[tokio::test]
#[should_panic]
async fn get_proposer_random_value_exceeds_total_weight(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
) {
    set_current_epoch(&mut contract, EPOCH_1);
    set_previous_epoch(&mut contract, Some(EPOCH_0));

    // Stakers with total weight 10000.
    set_stakers(&mut contract, EPOCH_1, vec![STAKER_1, STAKER_2, STAKER_3, STAKER_4]);

    let mut state_sync_client = MockStateSyncClient::new();
    state_sync_client
        .expect_get_block_hash()
        .returning(|_| Ok(starknet_api::block::BlockHash(Felt::ZERO)));

    // Random value is out of range. Valid range is [0, 10000).
    let mut random_generator = MockBlockRandomGenerator::new();
    random_generator.expect_generate().returning(move |_, _, _, _| 10000);

    let committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(state_sync_client),
        Arc::new(random_generator),
        default_config,
        None,
    );

    let committee = committee_manager.get_committee(E1_H1).await.unwrap();
    let _ = committee.get_proposer(E1_H1, 0);
}

#[rstest]
#[tokio::test]
async fn get_proposer_cache(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
) {
    set_current_epoch(&mut contract, EPOCH_1);
    set_previous_epoch(&mut contract, Some(EPOCH_0));
    set_stakers(&mut contract, EPOCH_1, vec![STAKER_1, STAKER_2, STAKER_3, STAKER_4]);

    let mut state_sync_client = MockStateSyncClient::new();
    state_sync_client
        .expect_get_block_hash()
        .times(1)
        .returning(|_| Ok(starknet_api::block::BlockHash(Felt::ZERO)));

    // Expect random generator to be called 3 times total (once per cache miss).
    let mut random_generator = MockBlockRandomGenerator::new();
    random_generator.expect_generate().times(3).returning(move |_, _, _, _| 0);

    let committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(state_sync_client),
        Arc::new(random_generator),
        default_config,
        None,
    );

    let committee = committee_manager.get_committee(E1_H1).await.unwrap();

    // Query 1: (H1, 0) - cache miss, should fetch from state.
    assert_eq!(committee.get_proposer(E1_H1, 0).unwrap(), STAKER_4.address);

    // Query 2: (H1, 0) - cache hit, should return from cache.
    assert_eq!(committee.get_proposer(E1_H1, 0).unwrap(), STAKER_4.address);

    // Query 3: (H2, 0) - different height, cache miss.
    assert_eq!(committee.get_proposer(E1_H2, 0).unwrap(), STAKER_4.address);

    // Query 4: (H2, 1) - different round, cache miss.
    assert_eq!(committee.get_proposer(E1_H2, 1).unwrap(), STAKER_4.address);

    // Query 5: (H2, 1) - cache hit.
    assert_eq!(committee.get_proposer(E1_H2, 1).unwrap(), STAKER_4.address);
}

#[rstest]
#[case::height_1_round_0(BlockNumber(1), 0, STAKER_2)]
#[case::height_1_round_1(BlockNumber(1), 1, STAKER_1)]
#[case::height_1_round_2(BlockNumber(1), 2, STAKER_3)]
#[case::height_2_round_0(BlockNumber(2), 0, STAKER_1)]
#[case::height_2_round_1(BlockNumber(2), 1, STAKER_3)]
#[case::height_2_round_2(BlockNumber(2), 2, STAKER_2)]
#[case::height_3_round_0(BlockNumber(3), 0, STAKER_3)]
#[tokio::test]
async fn test_get_proposer_with_actual_proposer_flag(
    mut default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
    #[case] height: BlockNumber,
    #[case] round: u32,
    #[case] expected_proposer: Staker,
) {
    // Test that when use_only_actual_proposer_selection = true,
    // get_proposer returns the same result as get_actual_proposer.
    // Expected committee order: [STAKER_3, STAKER_2, STAKER_1] (by weight descending)
    // With 3 eligible proposers, index = (height + round) % 3

    set_current_epoch(&mut contract, EPOCH_1);
    set_previous_epoch(&mut contract, Some(EPOCH_0));
    set_stakers(&mut contract, EPOCH_1, vec![STAKER_1, STAKER_2, STAKER_3]);

    default_config.static_config.use_only_actual_proposer_selection = true;
    let committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(create_state_sync_client_with_block_hash()),
        Arc::new(MockBlockRandomGenerator::new()),
        default_config,
        None,
    );

    let committee = committee_manager.get_committee(height).await.unwrap();
    // When the flag is true, get_proposer should return the same as get_actual_proposer.
    let proposer_result = committee.get_proposer(height, round).unwrap();
    let actual_proposer = committee.get_actual_proposer(height, round);
    assert_eq!(proposer_result, actual_proposer);
    assert_eq!(proposer_result, expected_proposer.address);
}

// Helper function to create ConfiguredStaker for testing
fn create_configured_staker(staker: &Staker, can_propose: bool) -> ConfiguredStaker {
    ConfiguredStaker {
        address: staker.address,
        weight: staker.weight,
        public_key: staker.public_key,
        can_propose,
    }
}

#[rstest]
#[case::height_1_round_0(BlockNumber(1), 0, STAKER_2)]
#[case::height_1_round_1(BlockNumber(1), 1, STAKER_1)]
#[case::height_1_round_2(BlockNumber(1), 2, STAKER_3)]
#[case::height_2_round_0(BlockNumber(2), 0, STAKER_1)]
#[case::height_2_round_1(BlockNumber(2), 1, STAKER_3)]
#[case::height_2_round_2(BlockNumber(2), 2, STAKER_2)]
#[case::height_3_round_0(BlockNumber(3), 0, STAKER_3)]
#[tokio::test]
async fn get_actual_proposer_all_eligible(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
    #[case] height: BlockNumber,
    #[case] round: u32,
    #[case] expected_proposer: Staker,
) {
    // Test round-robin selection across different heights and rounds.
    // Expected committee order: [STAKER_3, STAKER_2, STAKER_1] (by weight descending)
    // With 3 eligible proposers, index = (height + round) % 3

    set_current_epoch(&mut contract, EPOCH_1);
    set_previous_epoch(&mut contract, Some(EPOCH_0));
    set_stakers(&mut contract, EPOCH_1, vec![STAKER_1, STAKER_2, STAKER_3]);

    let committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(create_state_sync_client_with_block_hash()),
        Arc::new(MockBlockRandomGenerator::new()),
        default_config,
        None,
    );

    let committee = committee_manager.get_committee(height).await.unwrap();
    let proposer = committee.get_actual_proposer(height, round);
    assert_eq!(proposer, expected_proposer.address);
}

#[rstest]
#[case::height_1_round_0(BlockNumber(1), 0, STAKER_2)]
#[case::height_1_round_1(BlockNumber(1), 1, STAKER_3)]
#[case::height_1_round_2(BlockNumber(1), 2, STAKER_2)]
#[case::height_1_round_3(BlockNumber(1), 3, STAKER_3)]
#[case::height_2_round_0(BlockNumber(2), 0, STAKER_3)]
#[case::height_2_round_1(BlockNumber(2), 1, STAKER_2)]
#[case::height_2_round_2(BlockNumber(2), 2, STAKER_3)]
#[case::height_3_round_0(BlockNumber(3), 0, STAKER_2)]
#[case::height_3_round_1(BlockNumber(3), 1, STAKER_3)]
#[case::height_3_round_2(BlockNumber(3), 2, STAKER_2)]
#[tokio::test]
async fn get_actual_proposer_some_eligible(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
    #[case] height: BlockNumber,
    #[case] round: u32,
    #[case] expected_proposer: Staker,
) {
    // Expected committee order: [STAKER_4, STAKER_3, STAKER_2, STAKER_1] (by weight descending)
    // With 2 eligible proposers, index = (height + round) % 2

    let stakers = vec![STAKER_1, STAKER_2, STAKER_3, STAKER_4];
    // Only STAKER_2 and STAKER_3 are eligible to propose, STAKER_4 not in the config.
    let configured_stakers = vec![
        create_configured_staker(&STAKER_1, false),
        create_configured_staker(&STAKER_2, true),
        create_configured_staker(&STAKER_3, true),
    ];

    set_current_epoch(&mut contract, EPOCH_1);
    set_previous_epoch(&mut contract, Some(EPOCH_0));
    set_stakers(&mut contract, EPOCH_1, stakers);

    let default_committee =
        CommitteeConfig { start_epoch: 0, committee_size: 10, stakers: configured_stakers };
    let committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(create_state_sync_client_with_block_hash()),
        Arc::new(MockBlockRandomGenerator::new()),
        StakingManagerConfig {
            dynamic_config: StakingManagerDynamicConfig {
                default_committee,
                override_committee: None,
            },
            ..default_config
        },
        None,
    );

    let committee = committee_manager.get_committee(height).await.unwrap();
    let proposer = committee.get_actual_proposer(height, round);
    assert_eq!(proposer, expected_proposer.address);
}

#[rstest]
#[tokio::test]
#[should_panic]
async fn get_actual_proposer_no_eligible_panics(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
) {
    set_current_epoch(&mut contract, EPOCH_1);
    set_stakers(&mut contract, EPOCH_1, vec![STAKER_1, STAKER_2, STAKER_3]);

    // All stakers have can_propose = false
    let default_committee = CommitteeConfig {
        start_epoch: 0,
        committee_size: 10,
        stakers: vec![create_configured_staker(&STAKER_1, false)],
    };

    let committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(create_state_sync_client_with_block_hash()),
        Arc::new(MockBlockRandomGenerator::new()),
        StakingManagerConfig {
            dynamic_config: StakingManagerDynamicConfig {
                default_committee,
                override_committee: None,
            },
            ..default_config
        },
        None,
    );

    // Should panic.
    let committee = committee_manager.get_committee(E1_H1).await.unwrap();
    let _ = committee.get_actual_proposer(E1_H1, 0);
}

#[rstest]
#[tokio::test]
async fn test_get_actual_proposer_epoch_changes(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
) {
    set_current_epoch_with_times(&mut contract, EPOCH_1, 1..);
    set_previous_epoch(&mut contract, Some(EPOCH_0));
    set_stakers(&mut contract, EPOCH_1, vec![STAKER_1, STAKER_2, STAKER_3]);
    set_stakers(&mut contract, EPOCH_2, vec![STAKER_1, STAKER_2, STAKER_3]);

    // In epoch 1 (using default), only STAKER_1 can propose
    // In epoch 2 (using override), only STAKER_3 can propose
    let default_committee = CommitteeConfig {
        start_epoch: 0,
        committee_size: 10,
        stakers: vec![
            create_configured_staker(&STAKER_1, true),
            create_configured_staker(&STAKER_2, false),
            create_configured_staker(&STAKER_3, false),
        ],
    };
    let override_committee = Some(CommitteeConfig {
        start_epoch: 2,
        committee_size: 10,
        stakers: vec![
            create_configured_staker(&STAKER_1, false),
            create_configured_staker(&STAKER_2, false),
            create_configured_staker(&STAKER_3, true),
        ],
    });

    let committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(create_state_sync_client_with_block_hash()),
        Arc::new(MockBlockRandomGenerator::new()),
        StakingManagerConfig {
            dynamic_config: StakingManagerDynamicConfig { default_committee, override_committee },
            ..default_config
        },
        None,
    );

    // In epoch 1, should always get STAKER_1
    let committee = committee_manager.get_committee(E1_H1).await.unwrap();
    let proposer = committee.get_actual_proposer(E1_H1, 0);
    assert_eq!(proposer, STAKER_1.address);
    let committee = committee_manager.get_committee(E1_H2).await.unwrap();
    let proposer = committee.get_actual_proposer(E1_H2, 5);
    assert_eq!(proposer, STAKER_1.address);

    // In epoch 2, should always get STAKER_3
    let committee = committee_manager.get_committee(E2_H1).await.unwrap();
    let proposer = committee.get_actual_proposer(E2_H1, 0);
    assert_eq!(proposer, STAKER_3.address);
    let committee = committee_manager.get_committee(E2_H1.unchecked_next()).await.unwrap();
    let proposer = committee.get_actual_proposer(E2_H1.unchecked_next(), 5);
    assert_eq!(proposer, STAKER_3.address);
}

#[rstest]
#[tokio::test]
async fn get_actual_proposer_invalid_height(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
) {
    set_current_epoch(&mut contract, EPOCH_1);
    set_previous_epoch(&mut contract, Some(EPOCH_0));
    set_stakers(&mut contract, EPOCH_1, vec![STAKER_1, STAKER_2, STAKER_3]);

    let committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(create_state_sync_client_with_block_hash()),
        Arc::new(MockBlockRandomGenerator::new()),
        default_config,
        None,
    );

    let err = committee_manager.get_committee(E2_H2).await.err().unwrap();
    assert_matches!(err, CommitteeProviderError::InvalidHeight { .. });
}
