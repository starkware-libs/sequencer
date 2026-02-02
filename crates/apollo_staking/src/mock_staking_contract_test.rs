use std::sync::Arc;

use apollo_config_manager_types::communication::{
    ConfigManagerClientError,
    MockConfigManagerClient,
};
use apollo_config_manager_types::errors::ConfigManagerError;
use apollo_staking_config::config::{ConfiguredStaker, StakersConfig, StakingManagerDynamicConfig};
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

#[fixture]
fn mock_state_sync_client() -> SharedStateSyncClient {
    Arc::new(MockStateSyncClient::new())
}

fn mock_client_with_latest(block: Option<BlockNumber>) -> SharedStateSyncClient {
    let mut mock = MockStateSyncClient::new();
    mock.expect_get_latest_block_number().returning(move || Ok(block));
    Arc::new(mock)
}

#[rstest]
#[tokio::test]
async fn get_stakers_picks_latest_config_for_epoch(mock_state_sync_client: SharedStateSyncClient) {
    let config = vec![
        StakersConfig { start_epoch: 0, stakers: vec![STAKER_1] },
        StakersConfig { start_epoch: 3, stakers: vec![STAKER_1, STAKER_2] },
    ];

    let contract = MockStakingContract::new(mock_state_sync_client, config, None);

    let stakers = contract.get_stakers(1).await.unwrap();
    assert_eq!(stakers.len(), 1);

    let stakers = contract.get_stakers(4).await.unwrap();
    assert_eq!(stakers.len(), 2);
}

#[rstest]
#[tokio::test]
async fn get_stakers_missing_config(mock_state_sync_client: SharedStateSyncClient) {
    let config = vec![StakersConfig { start_epoch: 5, stakers: vec![STAKER_1] }];

    let contract = MockStakingContract::new(mock_state_sync_client, config, None);

    // Should return empty vec because the epoch is before the first config entry.
    let stakers = contract.get_stakers(0).await.unwrap();
    assert!(stakers.is_empty());
}

#[rstest]
#[tokio::test]
async fn get_stakers_fetches_dynamic_config_successfully(
    mock_state_sync_client: SharedStateSyncClient,
) {
    let initial_config = vec![StakersConfig { start_epoch: 0, stakers: vec![STAKER_1] }];

    let mut mock_config_client = MockConfigManagerClient::new();
    mock_config_client.expect_get_staking_manager_dynamic_config().returning(move || {
        Ok(StakingManagerDynamicConfig {
            stakers_config: vec![StakersConfig { start_epoch: 0, stakers: vec![STAKER_2] }],
            ..Default::default()
        })
    });

    let contract = MockStakingContract::new(
        mock_state_sync_client,
        initial_config,
        Some(Arc::new(mock_config_client)),
    );

    let stakers = contract.get_stakers(0).await.unwrap();
    assert_eq!(stakers.len(), 1);
    assert_eq!(stakers[0].address, STAKER_2.address);
}

#[rstest]
#[tokio::test]
async fn get_stakers_falls_back_to_initial_config_when_fetch_fails(
    mock_state_sync_client: SharedStateSyncClient,
) {
    let initial_config = vec![StakersConfig { start_epoch: 0, stakers: vec![STAKER_1] }];

    let mut mock_config_client = MockConfigManagerClient::new();
    mock_config_client.expect_get_staking_manager_dynamic_config().returning(|| {
        Err(ConfigManagerClientError::ConfigManagerError(ConfigManagerError::ConfigNotFound(
            "Test error".to_string(),
        )))
    });

    let contract = MockStakingContract::new(
        mock_state_sync_client,
        initial_config,
        Some(Arc::new(mock_config_client)),
    );

    let stakers = contract.get_stakers(0).await.unwrap();
    // Should use initial config when fetching dynamic config fails.
    assert_eq!(stakers.len(), 1);
    assert_eq!(stakers[0].address, STAKER_1.address);
}

#[rstest]
#[case::epoch_0(BlockNumber(15), Epoch { epoch_id: 0, start_block: BlockNumber(0), epoch_length: MockStakingContract::EPOCH_LENGTH })]
#[case::epoch_1(BlockNumber(45), Epoch { epoch_id: 1, start_block: BlockNumber(30), epoch_length: MockStakingContract::EPOCH_LENGTH })]
#[case::epoch_2(BlockNumber(60), Epoch { epoch_id: 2, start_block: BlockNumber(60), epoch_length: MockStakingContract::EPOCH_LENGTH })]
#[case::epoch_5(BlockNumber(301), Epoch { epoch_id: 10, start_block: BlockNumber(300), epoch_length: MockStakingContract::EPOCH_LENGTH })]
#[tokio::test]
async fn get_current_epoch_success(
    #[case] block_number: BlockNumber,
    #[case] expected_epoch: Epoch,
) {
    let contract =
        MockStakingContract::new(mock_client_with_latest(Some(block_number)), vec![], None);

    let epoch = contract.get_current_epoch().await.unwrap();
    assert_eq!(epoch, expected_epoch);
}

#[tokio::test]
async fn get_current_epoch_defaults_to_epoch_zero_when_no_blocks() {
    let contract = MockStakingContract::new(mock_client_with_latest(None), vec![], None);

    let epoch = contract.get_current_epoch().await.unwrap();
    assert_eq!(
        epoch,
        Epoch {
            epoch_id: 0,
            start_block: BlockNumber(0),
            epoch_length: MockStakingContract::EPOCH_LENGTH
        }
    );
}
