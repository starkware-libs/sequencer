use std::sync::Arc;

use apollo_batcher_types::batcher_types::{CallContractInput, CallContractOutput};
use apollo_batcher_types::communication::MockBatcherClient;
use mockall::predicate::eq;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::staking::StakingWeight;
use starknet_types_core::felt::Felt;

use crate::cairo_staking_contract::CairoStakingContract;
use crate::committee_provider::Staker;
use crate::contract_types::ContractStaker;
use crate::staking_contract::StakingContract;
use crate::staking_manager::Epoch;

const CONTRACT_ADDRESS: ContractAddress =
    ContractAddress(PatriciaKey::from_hex_unchecked("0xdead"));

const STAKER_1: ContractStaker = ContractStaker {
    contract_address: ContractAddress(PatriciaKey::from_hex_unchecked("0x1")),
    staking_power: StakingWeight(1000),
    public_key: Some(Felt::ONE),
};
const STAKER_2: ContractStaker = ContractStaker {
    contract_address: ContractAddress(PatriciaKey::from_hex_unchecked("0x2")),
    staking_power: StakingWeight(2000),
    public_key: Some(Felt::TWO),
};
const STAKER_3: ContractStaker = ContractStaker {
    contract_address: ContractAddress(PatriciaKey::from_hex_unchecked("0x3")),
    staking_power: StakingWeight(3000),
    public_key: None,
};

const EPOCH_1: Epoch = Epoch { epoch_id: 1, start_block: BlockNumber(1), epoch_length: 100 };
const EPOCH_2: Epoch = Epoch { epoch_id: 2, start_block: BlockNumber(101), epoch_length: 100 };

fn stakers_retdata(stakers: &[ContractStaker]) -> Vec<Felt> {
    let mut felts: Vec<Felt> = stakers.iter().flat_map(<Vec<Felt>>::from).collect();
    felts.insert(0, Felt::from(stakers.len()));
    felts
}

fn epoch_retdata(epoch: &Epoch) -> Vec<Felt> {
    Vec::<Felt>::from(epoch)
}

fn some_epoch_retdata(epoch: &Epoch) -> Vec<Felt> {
    let mut felts = vec![Felt::ZERO]; // Some variant
    felts.extend(Vec::<Felt>::from(epoch));
    felts
}

fn none_epoch_retdata() -> Vec<Felt> {
    vec![Felt::ONE] // None variant
}

fn create_contract(mock: MockBatcherClient) -> CairoStakingContract {
    CairoStakingContract::new(CONTRACT_ADDRESS, Arc::new(mock))
}

#[tokio::test]
async fn get_stakers_success() {
    let mut mock = MockBatcherClient::new();
    mock.expect_call_contract()
        .with(eq(CallContractInput {
            contract_address: CONTRACT_ADDRESS,
            entry_point: "get_stakers".to_string(),
            calldata: vec![Felt::ZERO], // epoch 0
        }))
        .returning(|_| {
            Ok(CallContractOutput { retdata: stakers_retdata(&[STAKER_1, STAKER_2]) })
        });

    let contract = create_contract(mock);
    assert_eq!(
        contract.get_stakers(0).await.unwrap(),
        vec![Staker::from(&STAKER_1), Staker::from(&STAKER_2)]
    );
}

#[tokio::test]
async fn get_stakers_filters_missing_public_key() {
    let mut mock = MockBatcherClient::new();
    mock.expect_call_contract()
        .with(eq(CallContractInput {
            contract_address: CONTRACT_ADDRESS,
            entry_point: "get_stakers".to_string(),
            calldata: vec![Felt::ZERO],
        }))
        .returning(|_| {
            Ok(CallContractOutput {
                retdata: stakers_retdata(&[STAKER_1, STAKER_2, STAKER_3]),
            })
        });

    let contract = create_contract(mock);
    // STAKER_3 has no public key and should be filtered out.
    assert_eq!(
        contract.get_stakers(0).await.unwrap(),
        vec![Staker::from(&STAKER_1), Staker::from(&STAKER_2)]
    );
}

#[tokio::test]
async fn get_current_epoch_success() {
    let mut mock = MockBatcherClient::new();
    mock.expect_call_contract()
        .with(eq(CallContractInput {
            contract_address: CONTRACT_ADDRESS,
            entry_point: "get_current_epoch_data".to_string(),
            calldata: vec![],
        }))
        .returning(|_| Ok(CallContractOutput { retdata: epoch_retdata(&EPOCH_1) }));

    let contract = create_contract(mock);
    assert_eq!(contract.get_current_epoch().await.unwrap(), EPOCH_1);
}

#[tokio::test]
async fn get_previous_epoch_returns_none_by_default() {
    let mut mock = MockBatcherClient::new();
    mock.expect_call_contract()
        .with(eq(CallContractInput {
            contract_address: CONTRACT_ADDRESS,
            entry_point: "get_previous_epoch_data".to_string(),
            calldata: vec![],
        }))
        .returning(|_| Ok(CallContractOutput { retdata: none_epoch_retdata() }));

    let contract = create_contract(mock);
    assert_eq!(contract.get_previous_epoch().await.unwrap(), None);
}

#[tokio::test]
async fn get_previous_epoch_success() {
    let mut mock = MockBatcherClient::new();
    mock.expect_call_contract()
        .with(eq(CallContractInput {
            contract_address: CONTRACT_ADDRESS,
            entry_point: "get_previous_epoch_data".to_string(),
            calldata: vec![],
        }))
        .returning(|_| Ok(CallContractOutput { retdata: some_epoch_retdata(&EPOCH_2) }));

    let contract = create_contract(mock);
    assert_eq!(contract.get_previous_epoch().await.unwrap(), Some(EPOCH_2));
}
