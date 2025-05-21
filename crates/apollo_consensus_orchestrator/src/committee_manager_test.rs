use std::convert::TryFrom;
use std::sync::Arc;

use assert_matches::assert_matches;
use blockifier::context::BlockContext;
use blockifier::execution::call_info::Retdata;
use blockifier::state::cached_state::CachedState;
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
use starknet_api::core::CONTRACT_ADDRESS_DOMAIN_SIZE;
use starknet_api::staking::StakingWeight;
use starknet_api::{contract_address, invoke_tx_args};
use starknet_types_core::felt::Felt;

use crate::committee_manager::{
    CommitteeManager,
    CommitteeManagerConfig,
    RetdataDeserializationError,
    Staker,
};

const STAKING_CONTRACT: FeatureContract =
    FeatureContract::MockStakingContract(RunnableCairo1::Casm);
const ACCOUNT_CONTRACT: FeatureContract =
    FeatureContract::AccountWithoutValidations(CairoVersion::Cairo1(RunnableCairo1::Casm));

#[fixture]
fn block_context() -> Arc<BlockContext> {
    Arc::new(BlockContext::create_for_testing())
}

#[fixture]
fn state(block_context: Arc<BlockContext>) -> CachedState<DictStateReader> {
    // Prepare the storage with a mock staking contract, and a dummy account as a staker.
    let state = test_state(
        block_context.chain_info(),
        BALANCE,
        &[(STAKING_CONTRACT, 1), (ACCOUNT_CONTRACT, 1)],
    );

    state
}

fn set_stakers(
    state: &mut CachedState<DictStateReader>,
    block_context: Arc<BlockContext>,
    stakers: &[Staker],
) {
    let mut stakers_as_felts: Vec<Felt> = stakers.iter().flat_map(<Vec<Felt>>::from).collect();
    stakers_as_felts.insert(0, Felt::from(stakers.len()));

    // Invoke the set_stakers function on the mock staking contract.
    let invoke_args = invoke_tx_args! {
        sender_address: ACCOUNT_CONTRACT.get_instance_address(0),
        calldata: create_calldata(
            STAKING_CONTRACT.get_instance_address(0),
            "set_stakers",
            &stakers_as_felts,
        ),
        resource_bounds: default_all_resource_bounds(),
    };
    let account_tx = invoke_tx_with_default_flags(invoke_args.clone());
    account_tx.execute(state, &block_context).unwrap();
}

#[rstest]
#[case::empty_committee(vec![])]
#[case::single_staker(vec![
    Staker { address: contract_address!("0x1"), weight: StakingWeight(1000), public_key: Felt::ONE },
])]
#[case::multiple_stakers(vec![
    Staker { address: contract_address!("0x1"), weight: StakingWeight(1000), public_key: Felt::ONE },
    Staker { address: contract_address!("0x2"), weight: StakingWeight(2000), public_key: Felt::TWO },
])]
fn get_committee_at_epoch_success(
    mut state: CachedState<DictStateReader>,
    block_context: Arc<BlockContext>,
    #[case] stakers: Vec<Staker>,
) {
    set_stakers(&mut state, block_context.clone(), &stakers);

    let committee_manager = CommitteeManager::new(CommitteeManagerConfig {
        staking_contract_address: STAKING_CONTRACT.get_instance_address(0),
    });

    let committee = committee_manager.get_committee_at_epoch(1, state, block_context).unwrap();

    assert_eq!(committee, stakers);
}

// --- TryFrom tests for Staker and ArrayRetdata ---

#[rstest]
fn staker_try_from_valid() {
    let staker = Staker::try_from([Felt::ONE, Felt::ONE, Felt::ONE]).unwrap();
    assert_eq!(staker.address, contract_address!("0x1"));
    assert_eq!(staker.weight, StakingWeight(1));
    assert_eq!(staker.public_key, Felt::ONE);
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
fn staker_array_retdata_try_from_valid() {
    const NUM_ELEMENTS: usize = 2;
    let valid_retdata = [
        [Felt::from(NUM_ELEMENTS)].as_slice(),
        [Felt::ONE; Staker::CAIRO_OBJECT_LENGTH * NUM_ELEMENTS].as_slice(),
    ]
    .concat();

    let result = Staker::from_retdata_many(Retdata(valid_retdata)).unwrap();
    assert_eq!(result.len(), NUM_ELEMENTS);
}

#[rstest]
#[case::empty_retdata(vec![])]
#[case::missing_num_elements(vec![Felt::ONE; Staker::CAIRO_OBJECT_LENGTH * 2])]
#[case::invalid_staker_length(vec![Felt::ONE; Staker::CAIRO_OBJECT_LENGTH - 1])]
fn staker_array_retdata_try_from_invalid_length(#[case] retdata: Vec<Felt>) {
    let err = Staker::from_retdata_many(Retdata(retdata)).unwrap_err();
    assert_matches!(err, RetdataDeserializationError::InvalidRetdataLength { .. });
}
