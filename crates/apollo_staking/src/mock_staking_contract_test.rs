use std::sync::Arc;

use apollo_config_manager_types::communication::{
    ConfigManagerClientError,
    MockConfigManagerClient,
};
use apollo_config_manager_types::errors::ConfigManagerError;
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

fn mock_client_with_latest(block: Option<BlockNumber>) -> SharedStateSyncClient {
    let mut mock = MockStateSyncClient::new();
    mock.expect_get_latest_block_number().returning(move || Ok(block));
    Arc::new(mock)
}

#[rstest]
#[tokio::test]
async fn get_stakers_picks_latest_config_for_epoch(mock_state_sync_client: SharedStateSyncClient) {
    let default_config =
        CommitteeConfig { start_epoch: 0, committee_size: 100, stakers: vec![STAKER_1] };
    let override_config = Some(CommitteeConfig {
        start_epoch: 3,
        committee_size: 100,
        stakers: vec![STAKER_1, STAKER_2],
    });

    let contract =
        MockStakingContract::new(mock_state_sync_client, default_config, override_config, None);

    // Epoch 1 < 3, so should use default (STAKER_1 only)
    let stakers = contract.get_stakers(1).await.unwrap();
    assert_eq!(stakers.len(), 1);

    // Epoch 4 >= 3, so should use override (STAKER_1 and STAKER_2)
    let stakers = contract.get_stakers(4).await.unwrap();
    assert_eq!(stakers.len(), 2);
}

#[rstest]
#[tokio::test]
async fn get_stakers_no_override(mock_state_sync_client: SharedStateSyncClient) {
    let default_config =
        CommitteeConfig { start_epoch: 0, committee_size: 100, stakers: vec![STAKER_1] };

    let contract = MockStakingContract::new(mock_state_sync_client, default_config, None, None);

    // Should always return default stakers when no override is present
    let stakers = contract.get_stakers(0).await.unwrap();
    assert_eq!(stakers.len(), 1);

    let stakers = contract.get_stakers(100).await.unwrap();
    assert_eq!(stakers.len(), 1);
}

#[rstest]
#[tokio::test]
async fn get_stakers_fetches_dynamic_config_successfully(
    mock_state_sync_client: SharedStateSyncClient,
) {
    let initial_default =
        CommitteeConfig { start_epoch: 0, committee_size: 100, stakers: vec![STAKER_1] };

    let mut mock_config_client = MockConfigManagerClient::new();
    mock_config_client.expect_get_staking_manager_dynamic_config().returning(move || {
        Ok(StakingManagerDynamicConfig {
            default_committee: CommitteeConfig {
                start_epoch: 0,
                committee_size: 100,
                stakers: vec![STAKER_2],
            },
            override_committee: None,
        })
    });

    let contract = MockStakingContract::new(
        mock_state_sync_client,
        initial_default,
        None,
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
    let initial_default =
        CommitteeConfig { start_epoch: 0, committee_size: 100, stakers: vec![STAKER_1] };

    let mut mock_config_client = MockConfigManagerClient::new();
    mock_config_client.expect_get_staking_manager_dynamic_config().returning(|| {
        Err(ConfigManagerClientError::ConfigManagerError(ConfigManagerError::ConfigNotFound(
            "Test error".to_string(),
        )))
    });

    let contract = MockStakingContract::new(
        mock_state_sync_client,
        initial_default,
        None,
        Some(Arc::new(mock_config_client)),
    );

    let stakers = contract.get_stakers(0).await.unwrap();
    // Should use initial config when fetching dynamic config fails.
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
) {
    let contract = MockStakingContract::new(
        mock_client_with_latest(Some(block_number)),
        CommitteeConfig { start_epoch: 0, committee_size: 100, stakers: vec![] },
        None,
        None,
    );

    let epoch = contract.get_current_epoch().await.unwrap();
    assert_eq!(epoch, expected_epoch);
}

#[tokio::test]
async fn get_current_epoch_defaults_to_epoch_zero_when_no_blocks() {
    let contract = MockStakingContract::new(
        mock_client_with_latest(None),
        CommitteeConfig { start_epoch: 0, committee_size: 100, stakers: vec![] },
        None,
        None,
    );

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
) {
    let contract = MockStakingContract::new(
        mock_client_with_latest(Some(block_number)),
        CommitteeConfig { start_epoch: 0, committee_size: 100, stakers: vec![] },
        None,
        None,
    );

    let previous_epoch = contract.get_previous_epoch().await.unwrap();
    assert_eq!(previous_epoch, expected_previous_epoch);
}

#[tokio::test]
async fn get_previous_epoch_returns_none_when_no_blocks() {
    let contract = MockStakingContract::new(
        mock_client_with_latest(None),
        CommitteeConfig { start_epoch: 0, committee_size: 100, stakers: vec![] },
        None,
        None,
    );

    let previous_epoch = contract.get_previous_epoch().await.unwrap();
    assert_eq!(previous_epoch, None);
}
