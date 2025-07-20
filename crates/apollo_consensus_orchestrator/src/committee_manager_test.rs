use std::convert::TryFrom;
use std::sync::Arc;

use apollo_state_sync_types::communication::MockStateSyncClient;
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
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, PatriciaKey, CONTRACT_ADDRESS_DOMAIN_SIZE};
use starknet_api::staking::StakingWeight;
use starknet_api::{contract_address, invoke_tx_args};
use starknet_types_core::felt::Felt;

use crate::committee_manager::{
    Committee,
    CommitteeProvider,
    CommitteeProviderError,
    ExecutionContext,
    RetdataDeserializationError,
    Staker,
    StakerSet,
    StakingManager,
    StakingManagerConfig,
};
use crate::utils::MockBlockRandomGenerator;

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

#[fixture]
fn default_config() -> StakingManagerConfig {
    StakingManagerConfig {
        staking_contract_address: STAKING_CONTRACT.get_instance_address(0),
        max_cached_epochs: 10,
        committee_size: 10,
        proposer_prediction_window_in_heights: 10,
    }
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
#[case::multiple_stakers_equal_weights(vec![STAKER_1, STAKER_2, STAKER_3, Staker { address: ContractAddress(PatriciaKey::from_hex_unchecked("0x0")), .. STAKER_1 }], vec![STAKER_3, STAKER_2, STAKER_1])]
fn get_committee_success(
    default_config: StakingManagerConfig,
    mut state: State,
    block_context: Context,
    #[case] stakers: StakerSet,
    #[case] expected_committee: Committee,
) {
    set_stakers(&mut state, &block_context, &stakers);

    let mut committee_manager = StakingManager::new(
        Box::new(MockBlockRandomGenerator::new()),
        StakingManagerConfig { committee_size: 3, ..default_config },
    );

    let context = ExecutionContext {
        state_reader: state.clone(),
        block_context: block_context.clone(),
        state_sync_client: Arc::new(MockStateSyncClient::new()),
    };
    let committee = committee_manager.get_committee(1, context).unwrap();

    assert_eq!(*committee, expected_committee);
}

#[rstest]
fn get_committee_cache(
    default_config: StakingManagerConfig,
    mut state: State,
    block_context: Context,
) {
    let mut committee_manager = StakingManager::new(
        Box::new(MockBlockRandomGenerator::new()),
        StakingManagerConfig { max_cached_epochs: 1, ..default_config },
    );

    // Case 1: Get committee for epoch 1. Cache miss – STAKER_1 fetched from contract.
    set_stakers(&mut state, &block_context, vec![STAKER_1].as_slice());
    let context = ExecutionContext {
        state_reader: state.clone(),
        block_context: block_context.clone(),
        state_sync_client: Arc::new(MockStateSyncClient::new()),
    };
    let committee = committee_manager.get_committee(1, context).unwrap();
    assert_eq!(*committee, vec![STAKER_1]);

    // Case 2: Query epoch 1 again. Cache hit – STAKER_1 returned from cache despite contract
    // change.
    set_stakers(&mut state, &block_context, vec![STAKER_2].as_slice());
    let context = ExecutionContext {
        state_reader: state.clone(),
        block_context: block_context.clone(),
        state_sync_client: Arc::new(MockStateSyncClient::new()),
    };
    let committee = committee_manager.get_committee(1, context.clone()).unwrap();
    assert_eq!(*committee, vec![STAKER_1]);

    // Case 3: Get committee for epoch 2. Cache miss – STAKER_2 fetched from updated contract state.
    let committee = committee_manager.get_committee(2, context.clone()).unwrap();
    assert_eq!(*committee, vec![STAKER_2]);

    // Case 4: Query epoch 1 again. Cache miss due to the cache being full - STAKER_2 now fetched
    // from contract.
    let committee = committee_manager.get_committee(1, context).unwrap();
    assert_eq!(*committee, vec![STAKER_2]);
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
    mut state: State,
    block_context: Context,
    #[case] random_value: u128,
    #[case] expected_proposer: Staker,
) {
    // The staker weights are 1000, 2000, 3000, and 4000, totaling 10,000.
    // Based on the cumulative weight ranges:
    // - Random values in [0–3999] → STAKER_4
    // - [4000–6999] → STAKER_3
    // - [7000–8999] → STAKER_2
    // - [9000–9999] → STAKER_1

    set_stakers(&mut state, &block_context, &vec![STAKER_1, STAKER_2, STAKER_3, STAKER_4]);

    let mut random_generator = MockBlockRandomGenerator::new();
    random_generator.expect_generate().returning(move |_, _, _, _| random_value);

    let mut committee_manager = StakingManager::new(Box::new(random_generator), default_config);

    let context = ExecutionContext {
        state_reader: state.clone(),
        block_context: block_context.clone(),
        state_sync_client: Arc::new(MockStateSyncClient::new()),
    };
    let proposer = committee_manager.get_proposer(BlockNumber(1), 0, context).await.unwrap();

    assert_eq!(proposer, expected_proposer.address);
}

#[rstest]
#[tokio::test]
async fn get_proposer_empty_committee(
    default_config: StakingManagerConfig,
    state: State,
    block_context: Context,
) {
    let mut random_generator = MockBlockRandomGenerator::new();
    random_generator.expect_generate().returning(move |_, _, _, _| 0);

    let mut committee_manager = StakingManager::new(
        Box::new(random_generator),
        StakingManagerConfig { committee_size: 0, ..default_config },
    );

    let context = ExecutionContext {
        state_reader: state.clone(),
        block_context: block_context.clone(),
        state_sync_client: Arc::new(MockStateSyncClient::new()),
    };
    let err = committee_manager.get_proposer(BlockNumber(1), 0, context).await.unwrap_err();
    assert_matches!(err, CommitteeProviderError::EmptyCommittee);
}

#[rstest]
#[tokio::test]
#[should_panic]
async fn get_proposer_random_value_exceeds_total_weight(
    default_config: StakingManagerConfig,
    mut state: State,
    block_context: Context,
) {
    // Stakers with total weight 10000.
    set_stakers(&mut state, &block_context, &vec![STAKER_1, STAKER_2, STAKER_3, STAKER_4]);

    // Random value is out of range. Valid range is [0, 10000).
    let mut random_generator = MockBlockRandomGenerator::new();
    random_generator.expect_generate().returning(move |_, _, _, _| 10000);

    let mut committee_manager = StakingManager::new(Box::new(random_generator), default_config);

    let context = ExecutionContext {
        state_reader: state.clone(),
        block_context: block_context.clone(),
        state_sync_client: Arc::new(MockStateSyncClient::new()),
    };

    let _ = committee_manager.get_proposer(BlockNumber(1), 0, context).await;
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
