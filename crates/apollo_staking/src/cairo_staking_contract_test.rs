use std::sync::{Arc, Mutex};

use blockifier::context::{BlockContext, ChainInfo};
use blockifier::state::cached_state::CachedState;
use blockifier::state::state_api::StateReader;
use blockifier::test_utils::BALANCE;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::test_utils::initial_test_state::test_state;
use blockifier::transaction::test_utils::{
    default_all_resource_bounds, invoke_tx_with_default_flags,
};
use blockifier::transaction::transactions::ExecutableTransaction;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use rstest::{fixture, rstest};
use starknet_api::block::{BlockInfo, BlockNumber};
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::invoke_tx_args;
use starknet_api::staking::StakingWeight;
use starknet_types_core::felt::Felt;

use crate::cairo_staking_contract::{
    CairoStakingContract, ExtendedStateReader, StateReaderFactory,
};
use crate::committee_provider::Staker;
use crate::contract_types::ContractStaker;
use crate::staking_contract::StakingContract;
use crate::staking_manager::Epoch;

type State = CachedState<DictStateReader>;

const STAKING_CONTRACT: FeatureContract =
    FeatureContract::MockStakingContract(RunnableCairo1::Casm);
const ACCOUNT_CONTRACT: FeatureContract =
    FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm));

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

#[derive(Clone)]
struct TestStateReaderFactory {
    base_state: Arc<Mutex<State>>,
}

impl StateReaderFactory for TestStateReaderFactory {
    fn create(&self) -> Box<dyn ExtendedStateReader> {
        Box::new(self.base_state.lock().unwrap().clone())
    }
}

impl ExtendedStateReader for State {
    fn get_block_info(
        &self,
    ) -> Result<BlockInfo, crate::cairo_staking_contract::ExtendedStateReaderError> {
        Ok(BlockInfo::create_for_testing())
    }
}

#[fixture]
fn state() -> Arc<Mutex<State>> {
    // Prepare the storage with a mock staking contract, and a dummy account as a staker.
    let state = test_state(
        &ChainInfo::create_for_testing(),
        BALANCE,
        &[(STAKING_CONTRACT, 1), (ACCOUNT_CONTRACT, 1)],
    );

    Arc::new(Mutex::new(state))
}
fn create_contract(state: Arc<Mutex<State>>) -> CairoStakingContract {
    let chain_info = ChainInfo::create_for_testing();
    let contract_address = STAKING_CONTRACT.get_instance_address(0);
    let factory = TestStateReaderFactory { base_state: state.clone() };
    CairoStakingContract::new(chain_info, contract_address, Arc::new(factory))
}

fn execute_call(state: &mut State, function_name: &str, calldata: &[Felt]) {
    let account_address = ACCOUNT_CONTRACT.get_instance_address(0);
    let invoke_args = invoke_tx_args! {
        sender_address: account_address,
        calldata: create_calldata(STAKING_CONTRACT.get_instance_address(0), function_name, calldata),
        resource_bounds: default_all_resource_bounds(),
        nonce: state.get_nonce_at(account_address).unwrap(),
    };
    let account_tx = invoke_tx_with_default_flags(invoke_args);
    let result = account_tx.execute(state, &BlockContext::create_for_testing()).unwrap();
    assert!(!result.execute_call_info.unwrap().execution.failed);
}

fn set_stakers(state: &mut State, stakers: &[ContractStaker]) {
    let mut raw_felts: Vec<Felt> = stakers.iter().flat_map(<Vec<Felt>>::from).collect();
    raw_felts.insert(0, Felt::from(stakers.len()));

    execute_call(state, "set_stakers", &raw_felts);
}

fn set_current_epoch(state: &mut State, epoch: Epoch) {
    execute_call(state, "set_current_epoch", &Vec::<Felt>::from(&epoch));
}

#[rstest]
#[tokio::test]
async fn get_stakers_success(state: Arc<Mutex<State>>) {
    let contract = create_contract(state.clone());

    set_stakers(&mut state.lock().unwrap(), &[STAKER_1, STAKER_2]);
    assert_eq!(
        contract.get_stakers(0).await.unwrap(),
        vec![Staker::from(&STAKER_1), Staker::from(&STAKER_2)]
    );

    // Change the state and verify that the contract is updated.
    set_stakers(&mut state.lock().unwrap(), &[STAKER_2]);
    assert_eq!(contract.get_stakers(0).await.unwrap(), vec![Staker::from(&STAKER_2)]);
}

#[rstest]
#[tokio::test]
async fn get_stakers_filters_missing_public_key(state: Arc<Mutex<State>>) {
    let contract = create_contract(state.clone());

    // STAKER_3 has no public key, so it should be filtered out.
    set_stakers(&mut state.lock().unwrap(), &[STAKER_1, STAKER_2, STAKER_3]);
    assert_eq!(
        contract.get_stakers(0).await.unwrap(),
        vec![Staker::from(&STAKER_1), Staker::from(&STAKER_2)]
    );
}

#[rstest]
#[tokio::test]
async fn get_current_epoch_success(state: Arc<Mutex<State>>) {
    let contract = create_contract(state.clone());

    set_current_epoch(&mut state.lock().unwrap(), EPOCH_1);
    assert_eq!(contract.get_current_epoch().await.unwrap(), EPOCH_1);

    // Change the state and verify that the contract is updated.
    set_current_epoch(&mut state.lock().unwrap(), EPOCH_2);
    assert_eq!(contract.get_current_epoch().await.unwrap(), EPOCH_2);
}
