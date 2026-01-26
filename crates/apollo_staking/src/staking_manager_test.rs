use std::collections::HashSet;
use std::sync::Arc;

use apollo_config_manager_types::communication::MockConfigManagerClient;
use apollo_staking_config::config::{
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

use crate::committee_provider::{Committee, CommitteeProvider, CommitteeProviderError, Staker};
use crate::staking_contract::MockStakingContract;
use crate::staking_manager::{Epoch, StakingManager};
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

const EPOCH_1: Epoch = Epoch { epoch_id: 1, start_block: BlockNumber(1), epoch_length: 100 };
const EPOCH_2: Epoch = Epoch { epoch_id: 2, start_block: BlockNumber(101), epoch_length: 100 };

#[fixture]
fn contract() -> MockStakingContract {
    MockStakingContract::new()
}

#[fixture]
fn default_config() -> StakingManagerConfig {
    StakingManagerConfig {
        dynamic_config: StakingManagerDynamicConfig { committee_size: 10, stakers_config: vec![] },
        static_config: StakingManagerStaticConfig {
            max_cached_epochs: 10,
            proposer_prediction_window_in_heights: 10,
        },
    }
}

fn set_stakers(contract: &mut MockStakingContract, epoch: Epoch, stakers: Vec<Staker>) {
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
    #[case] stakers: Vec<Staker>,
    #[case] expected_committee: Committee,
) {
    set_current_epoch(&mut contract, EPOCH_1);
    set_stakers(&mut contract, EPOCH_1, stakers);

    let mut committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(MockStateSyncClient::new()),
        Box::new(MockBlockRandomGenerator::new()),
        StakingManagerConfig {
            dynamic_config: StakingManagerDynamicConfig {
                committee_size: 3,
                stakers_config: vec![],
            },
            ..default_config
        },
        None,
    );

    let committee = committee_manager.get_committee(BlockNumber(1)).await.unwrap();

    assert_eq!(*committee, expected_committee);
}

#[rstest]
#[tokio::test]
async fn get_committee_cache(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
) {
    // Set contract expectations for the first query (Case 1).
    set_current_epoch(&mut contract, EPOCH_1);
    set_stakers(&mut contract, EPOCH_1, vec![STAKER_1]);

    // Set contract expectations for the following queries (Case 3 and 4).
    set_current_epoch_with_times(&mut contract, EPOCH_2, 1..);
    set_stakers(&mut contract, EPOCH_2, vec![STAKER_2]);

    let mut config = default_config;
    config.static_config.max_cached_epochs = 1;
    let mut committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(MockStateSyncClient::new()),
        Box::new(MockBlockRandomGenerator::new()),
        config,
        None,
    );

    // Case 1: Get committee for epoch 1. Cache miss – STAKER_1 fetched from contract.
    let committee = committee_manager.get_committee(BlockNumber(1)).await.unwrap();
    assert_eq!(*committee, vec![STAKER_1]);

    // Case 2: Query epoch 1 again. Cache hit – STAKER_1 returned from cache.
    let committee = committee_manager.get_committee(BlockNumber(1)).await.unwrap();
    assert_eq!(*committee, vec![STAKER_1]);

    // Case 3: Get committee for epoch 2. Cache miss – new state is fetched from the contract.
    let committee = committee_manager.get_committee(BlockNumber(101)).await.unwrap();
    assert_eq!(*committee, vec![STAKER_2]);

    // Case 4: Query epoch 1 again - Invalid Height error. Since the manager advanced to epoch 2 in
    // the previous step, epoch 1 is now considered too old.
    let err = committee_manager.get_committee(BlockNumber(1)).await.unwrap_err();
    assert_matches!(err, CommitteeProviderError::InvalidHeight { .. });
}

#[rstest]
#[tokio::test]
async fn get_committee_for_next_epoch(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
) {
    set_current_epoch_with_times(&mut contract, EPOCH_1, 1..);

    // Set the stakers for the next epoch.
    set_stakers(&mut contract, EPOCH_2, vec![STAKER_1, STAKER_2]);

    let mut committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(MockStateSyncClient::new()),
        Box::new(MockBlockRandomGenerator::new()),
        StakingManagerConfig {
            dynamic_config: StakingManagerDynamicConfig {
                committee_size: 3,
                stakers_config: vec![],
            },
            ..default_config
        },
        None,
    );

    // 1. Valid Query: Height 101 falls within the next epoch's min bounds.
    let committee = (*committee_manager.get_committee(BlockNumber(101)).await.unwrap()).clone();
    assert_eq!(committee.into_iter().collect::<HashSet<_>>(), HashSet::from([STAKER_1, STAKER_2]));

    // 2. Invalid Query: Height 150 exceeds the min bounds of the next epoch.
    // Since the next epoch's length is not known at this point, we cannot know if this height
    // belongs to Epoch 2 or a future Epoch > 2.
    let err = committee_manager.get_committee(BlockNumber(150)).await.unwrap_err();
    assert_matches!(err, CommitteeProviderError::InvalidHeight { .. });
}

#[rstest]
#[tokio::test]
async fn get_committee_applies_dynamic_config_changes(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
) {
    set_current_epoch_with_times(&mut contract, EPOCH_1, 1..);
    set_stakers(&mut contract, EPOCH_1, vec![STAKER_1, STAKER_2, STAKER_3]);
    set_stakers(&mut contract, EPOCH_2, vec![STAKER_1, STAKER_2, STAKER_3]);

    let mut config_manager_client = MockConfigManagerClient::new();
    config_manager_client.expect_get_staking_manager_dynamic_config().times(1).return_once(|| {
        Ok(StakingManagerDynamicConfig { committee_size: 2, stakers_config: vec![] })
    });
    config_manager_client.expect_get_staking_manager_dynamic_config().times(1).return_once(|| {
        Ok(StakingManagerDynamicConfig { committee_size: 1, stakers_config: vec![] })
    });

    let mut committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(MockStateSyncClient::new()),
        Box::new(MockBlockRandomGenerator::new()),
        default_config,
        Some(Arc::new(config_manager_client)),
    );

    let committee = committee_manager.get_committee(BlockNumber(1)).await.unwrap();
    assert_eq!(committee.len(), 2);

    let committee = committee_manager.get_committee(BlockNumber(101)).await.unwrap();
    assert_eq!(committee.len(), 1);
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

    // The staker weights are 1000, 2000, 3000, and 4000, totaling 10,000.
    // Based on the cumulative weight ranges:
    // - Random values in [0–3999] → STAKER_4
    // - [4000–6999] → STAKER_3
    // - [7000–8999] → STAKER_2
    // - [9000–9999] → STAKER_1

    set_stakers(&mut contract, EPOCH_1, vec![STAKER_1, STAKER_2, STAKER_3, STAKER_4]);

    let mut random_generator = MockBlockRandomGenerator::new();
    random_generator.expect_generate().returning(move |_, _, _, _| random_value);

    let mut committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(MockStateSyncClient::new()),
        Box::new(random_generator),
        default_config,
        None,
    );

    let proposer = committee_manager.get_proposer(BlockNumber(1), 0).await.unwrap();

    assert_eq!(proposer, expected_proposer.address);
}

#[rstest]
#[tokio::test]
async fn get_proposer_empty_committee(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
) {
    set_current_epoch(&mut contract, EPOCH_1);
    set_stakers(&mut contract, EPOCH_1, vec![]);

    let mut random_generator = MockBlockRandomGenerator::new();
    random_generator.expect_generate().returning(move |_, _, _, _| 0);

    let mut committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(MockStateSyncClient::new()),
        Box::new(random_generator),
        StakingManagerConfig {
            dynamic_config: StakingManagerDynamicConfig {
                committee_size: 0,
                stakers_config: vec![],
            },
            ..default_config
        },
        None,
    );

    let err = committee_manager.get_proposer(BlockNumber(1), 0).await.unwrap_err();
    assert_matches!(err, CommitteeProviderError::EmptyCommittee);
}

#[rstest]
#[tokio::test]
#[should_panic]
async fn get_proposer_random_value_exceeds_total_weight(
    default_config: StakingManagerConfig,
    mut contract: MockStakingContract,
) {
    set_current_epoch(&mut contract, EPOCH_1);

    // Stakers with total weight 10000.
    set_stakers(&mut contract, EPOCH_1, vec![STAKER_1, STAKER_2, STAKER_3, STAKER_4]);

    // Random value is out of range. Valid range is [0, 10000).
    let mut random_generator = MockBlockRandomGenerator::new();
    random_generator.expect_generate().returning(move |_, _, _, _| 10000);

    let mut committee_manager = StakingManager::new(
        Arc::new(contract),
        Arc::new(MockStateSyncClient::new()),
        Box::new(MockBlockRandomGenerator::new()),
        default_config,
        None,
    );

    let _ = committee_manager.get_proposer(BlockNumber(1), 0).await;
}
