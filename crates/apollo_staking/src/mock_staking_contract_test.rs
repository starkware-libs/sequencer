use std::sync::Arc;

use apollo_staking_config::config::{
    CommitteeConfig,
    ConfiguredStaker,
    StakingManagerDynamicConfig,
};
use apollo_state_sync_types::communication::{MockStateSyncClient, SharedStateSyncClient};
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::staking::StakingWeight;
use starknet_types_core::felt::Felt;

use crate::mock_staking_contract::MockStakingContract;
use crate::staking_contract::StakingContract;
use crate::staking_manager::Epoch;

const STAKER_1: ConfiguredStaker = ConfiguredStaker {
    address: ContractAddress(PatriciaKey::from_hex_unchecked("0x1")),
    weight: StakingWeight(10),
    public_key: Felt::ONE,
    can_propose: true,
};

const STAKER_2: ConfiguredStaker = ConfiguredStaker {
    address: ContractAddress(PatriciaKey::from_hex_unchecked("0x2")),
    weight: StakingWeight(20),
    public_key: Felt::TWO,
    can_propose: false,
};

const EPOCH_LENGTH: u64 = MockStakingContract::EPOCH_LENGTH;

const EPOCH_0: Epoch =
    Epoch { epoch_id: 0, start_block: BlockNumber(0), epoch_length: EPOCH_LENGTH };
// A block in the middle of epoch 0.
const E0_H1: BlockNumber = BlockNumber(EPOCH_LENGTH / 2);

const EPOCH_1: Epoch =
    Epoch { epoch_id: 1, start_block: BlockNumber(EPOCH_LENGTH), epoch_length: EPOCH_LENGTH };
// A block in the middle of epoch 1.
const E1_H1: BlockNumber = BlockNumber(EPOCH_LENGTH + EPOCH_LENGTH / 2);

const EPOCH_2: Epoch =
    Epoch { epoch_id: 2, start_block: BlockNumber(2 * EPOCH_LENGTH), epoch_length: EPOCH_LENGTH };
// First block of epoch 2.
const E2_H1: BlockNumber = BlockNumber(2 * EPOCH_LENGTH);

const EPOCH_9: Epoch =
    Epoch { epoch_id: 9, start_block: BlockNumber(9 * EPOCH_LENGTH), epoch_length: EPOCH_LENGTH };

const EPOCH_10: Epoch =
    Epoch { epoch_id: 10, start_block: BlockNumber(10 * EPOCH_LENGTH), epoch_length: EPOCH_LENGTH };
// A block in the middle of epoch 10.
const E10_H1: BlockNumber = BlockNumber(10 * EPOCH_LENGTH + 1);

#[fixture]
fn mock_state_sync_client() -> SharedStateSyncClient {
    Arc::new(MockStateSyncClient::new())
}

#[fixture]
fn default_config() -> StakingManagerDynamicConfig {
    StakingManagerDynamicConfig {
        default_committee: CommitteeConfig {
            start_epoch: 0,
            committee_size: 100,
            stakers: vec![STAKER_1],
        },
        override_committee: None,
    }
}

fn mock_client_with_latest(block: Option<BlockNumber>) -> SharedStateSyncClient {
    let mut mock = MockStateSyncClient::new();
    mock.expect_get_latest_block_number().returning(move || Ok(block));
    Arc::new(mock)
}

#[rstest]
#[tokio::test]
async fn get_stakers_with_config_picks_latest_config_for_epoch(
    mock_state_sync_client: SharedStateSyncClient,
    default_config: StakingManagerDynamicConfig,
) {
    let input_config = StakingManagerDynamicConfig {
        override_committee: Some(CommitteeConfig {
            start_epoch: 3,
            committee_size: 100,
            stakers: vec![STAKER_1, STAKER_2],
        }),
        ..default_config
    };

    let contract = MockStakingContract::new(mock_state_sync_client, input_config.clone());

    // Epoch 1 < 3, so should use default (STAKER_1 only).
    let stakers = contract.get_stakers_with_config(1, &input_config).await.unwrap();
    assert_eq!(stakers.len(), 1);

    // Epoch 4 >= 3, so should use override (STAKER_1 and STAKER_2).
    let stakers = contract.get_stakers_with_config(4, &input_config).await.unwrap();
    assert_eq!(stakers.len(), 2);
}

#[rstest]
#[tokio::test]
async fn get_stakers_uses_internal_default_config(
    mock_state_sync_client: SharedStateSyncClient,
    default_config: StakingManagerDynamicConfig,
) {
    let contract = MockStakingContract::new(mock_state_sync_client, default_config);

    // get_stakers() should use the internal default_config.
    let stakers = contract.get_stakers(0).await.unwrap();
    assert_eq!(stakers.len(), 1);
    assert_eq!(stakers[0].address, STAKER_1.address);
}

#[rstest]
#[case::epoch_0(E0_H1, EPOCH_0)]
#[case::epoch_1(E1_H1, EPOCH_1)]
#[case::epoch_2(E2_H1, EPOCH_2)]
#[case::epoch_10(E10_H1, EPOCH_10)]
#[tokio::test]
async fn get_current_epoch_success(
    #[case] block_number: BlockNumber,
    #[case] expected_epoch: Epoch,
    default_config: StakingManagerDynamicConfig,
) {
    let contract =
        MockStakingContract::new(mock_client_with_latest(Some(block_number)), default_config);

    let epoch = contract.get_current_epoch().await.unwrap();
    assert_eq!(epoch, expected_epoch);
}

#[rstest]
#[tokio::test]
async fn get_current_epoch_defaults_to_epoch_zero_when_no_blocks(
    default_config: StakingManagerDynamicConfig,
) {
    let contract = MockStakingContract::new(mock_client_with_latest(None), default_config);

    let epoch = contract.get_current_epoch().await.unwrap();
    assert_eq!(epoch, EPOCH_0);
}

#[rstest]
#[case::epoch_0_returns_none(E0_H1, None)]
#[case::epoch_1_returns_epoch_0(E1_H1, Some(EPOCH_0))]
#[case::epoch_2_returns_epoch_1(E2_H1, Some(EPOCH_1))]
#[case::epoch_10_returns_epoch_9(E10_H1, Some(EPOCH_9))]
#[tokio::test]
async fn get_previous_epoch_success(
    #[case] block_number: BlockNumber,
    #[case] expected_previous_epoch: Option<Epoch>,
    default_config: StakingManagerDynamicConfig,
) {
    let contract =
        MockStakingContract::new(mock_client_with_latest(Some(block_number)), default_config);

    let previous_epoch = contract.get_previous_epoch().await.unwrap();
    assert_eq!(previous_epoch, expected_previous_epoch);
}

#[rstest]
#[tokio::test]
async fn get_previous_epoch_returns_none_when_no_blocks(
    default_config: StakingManagerDynamicConfig,
) {
    let contract = MockStakingContract::new(mock_client_with_latest(None), default_config);

    let previous_epoch = contract.get_previous_epoch().await.unwrap();
    assert_eq!(previous_epoch, None);
}
