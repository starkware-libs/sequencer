use std::convert::TryFrom;
use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier::context::BlockContext;
use blockifier::execution::call_info::Retdata;
use blockifier::state::cached_state::CachedState;
use blockifier::state::state_api::StateReader;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::test_utils::initial_test_state::test_state;
use blockifier::test_utils::BALANCE;
use blockifier::transaction::test_utils::{
    default_all_resource_bounds,
    invoke_tx_with_default_flags,
};
use blockifier::transaction::transactions::ExecutableTransaction;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use rstest::{fixture, rstest};
use starknet_api::core::{ContractAddress, PatriciaKey, CONTRACT_ADDRESS_DOMAIN_SIZE};
use starknet_api::staking::StakingWeight;
use starknet_api::{contract_address, invoke_tx_args};
use starknet_types_core::felt::Felt;

use crate::committee_manager::{
    Committee,
    CommitteeManager,
    CommitteeManagerConfig,
    RetdataDeserializationError,
    Staker,
    StakerSet,
};

const STAKING_CONTRACT: FeatureContract =
    FeatureContract::MockStakingContract(RunnableCairo1::Casm);
const ACCOUNT_CONTRACT: FeatureContract =
    FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm));

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

type Context = Arc<BlockContext>;
type State = CachedState<DictStateReader>;

#[fixture]
fn block_context() -> Context {
    Arc::new(BlockContext::create_for_testing())
}

#[fixture]
fn state(block_context: Context) -> State {
    // Prepare the storage with a mock staking contract, and a dummy account as a staker.
    let state = test_state(
        block_context.chain_info(),
        BALANCE,
        &[(STAKING_CONTRACT, 1), (ACCOUNT_CONTRACT, 1)],
    );

    state
}

fn set_stakers(state: &mut State, block_context: &Context, stakers: &[Staker]) {
    let mut stakers_as_felts: Vec<Felt> = stakers.iter().flat_map(<Vec<Felt>>::from).collect();
    stakers_as_felts.insert(0, Felt::from(stakers.len()));

    // Invoke the set_stakers function on the mock staking contract.
    let account_address = ACCOUNT_CONTRACT.get_instance_address(0);
    let invoke_args = invoke_tx_args! {
        sender_address: account_address,
        calldata: create_calldata(
            STAKING_CONTRACT.get_instance_address(0),
            "set_stakers",
            &stakers_as_felts,
        ),
        resource_bounds: default_all_resource_bounds(),
        nonce: state.get_nonce_at(account_address).unwrap(),
    };
    let account_tx = invoke_tx_with_default_flags(invoke_args);
    assert!(account_tx.execute(state, block_context).is_ok());
}

#[rstest]
#[case::no_stakers(vec![], vec![])]
#[case::single_staker(vec![STAKER_1], vec![STAKER_1])]
#[case::multiple_stakers_less_than_committee_size(vec![STAKER_1, STAKER_2], vec![STAKER_2, STAKER_1])]
#[case::multiple_stakers_equal_to_committee_size(vec![STAKER_1, STAKER_2, STAKER_3], vec![STAKER_3, STAKER_2, STAKER_1])]
#[case::multiple_stakers_greater_than_committee_size(vec![STAKER_1, STAKER_2, STAKER_3, STAKER_4], vec![STAKER_4, STAKER_3, STAKER_2])]
fn get_committee_success(
    mut state: State,
    block_context: Context,
    #[case] stakers: StakerSet,
    #[case] expected_committee: Committee,
) {
    set_stakers(&mut state, &block_context, &stakers);

    let mut committee_manager = CommitteeManager::new(CommitteeManagerConfig {
        staking_contract_address: STAKING_CONTRACT.get_instance_address(0),
        max_cached_epochs: 10,
        committee_size: 3,
    });

    let committee = committee_manager.get_committee(1, state, block_context).unwrap();

    assert_eq!(*committee, expected_committee);
}

#[rstest]
fn get_committee_cache(mut state: State, block_context: Context) {
    let mut committee_manager = CommitteeManager::new(CommitteeManagerConfig {
        staking_contract_address: STAKING_CONTRACT.get_instance_address(0),
        max_cached_epochs: 1,
        committee_size: 10,
    });

    // Case 1: Get committee for epoch 1. Cache miss – STAKER_1 fetched from contract.
    set_stakers(&mut state, &block_context, vec![STAKER_1].as_slice());
    let committee =
        committee_manager.get_committee(1, state.clone(), block_context.clone()).unwrap();
    assert_eq!(*committee, vec![STAKER_1]);

    // Case 2: Query epoch 1 again. Cache hit – STAKER_1 returned from cache despite contract
    // change.
    set_stakers(&mut state, &block_context, vec![STAKER_2].as_slice());
    let committee =
        committee_manager.get_committee(1, state.clone(), block_context.clone()).unwrap();
    assert_eq!(*committee, vec![STAKER_1]);

    // Case 3: Get committee for epoch 2. Cache miss – STAKER_2 fetched from updated contract state.
    let committee =
        committee_manager.get_committee(2, state.clone(), block_context.clone()).unwrap();
    assert_eq!(*committee, vec![STAKER_2]);

    // Case 4: Query epoch 1 again. Cache miss due to the cache being full - STAKER_2 now fetched
    // from contract.
    let committee = committee_manager.get_committee(1, state, block_context).unwrap();
    assert_eq!(*committee, vec![STAKER_2]);
}

// --- TryFrom tests for Staker and ArrayRetdata ---

#[rstest]
fn staker_try_from_valid() {
    let staker = Staker::try_from([Felt::ONE, Felt::TWO, Felt::THREE]).unwrap();
    assert_eq!(staker.address, contract_address!("0x1"));
    assert_eq!(staker.weight, StakingWeight(2));
    assert_eq!(staker.public_key, Felt::THREE);
}

#[rstest]
fn staker_try_from_invalid_address() {
    let err = Staker::try_from([CONTRACT_ADDRESS_DOMAIN_SIZE, Felt::ONE, Felt::ONE]).unwrap_err();
    assert_matches!(err, RetdataDeserializationError::ContractAddressConversionError { .. });
}

#[rstest]
fn staker_try_from_invalid_staked_amount() {
    let err = Staker::try_from([Felt::ONE, Felt::MAX, Felt::ONE]).unwrap_err(); // Felt::MAX is too big for u128
    assert_matches!(err, RetdataDeserializationError::U128ConversionError { .. });
}

#[rstest]
#[case::empty(0)]
#[case::two_elements(2)]
fn staker_array_retdata_try_from_valid(#[case] num_structs: usize) {
    let valid_retdata = [
        [Felt::from(num_structs)].as_slice(),
        vec![Felt::ONE; Staker::CAIRO_OBJECT_NUM_FELTS * num_structs].as_slice(),
    ]
    .concat();

    let result = Staker::from_retdata_many(Retdata(valid_retdata)).unwrap();
    assert_eq!(result.len(), num_structs);
}

#[rstest]
#[case::empty_retdata(vec![])]
#[case::missing_num_structs(vec![Felt::ONE; Staker::CAIRO_OBJECT_NUM_FELTS * 2])]
#[case::invalid_staker_length(vec![Felt::ONE; Staker::CAIRO_OBJECT_NUM_FELTS - 1])]
fn staker_array_retdata_try_from_invalid_length(#[case] retdata: Vec<Felt>) {
    let err = Staker::from_retdata_many(Retdata(retdata)).unwrap_err();
    assert_matches!(err, RetdataDeserializationError::InvalidArrayLength { .. });
}
