use std::collections::HashSet;
use std::convert::TryFrom;
use std::sync::Arc;

use apollo_state_sync_types::communication::MockStateSyncClient;
use assert_matches::assert_matches;
use blockifier::abi::constants::STORED_BLOCK_HASH_BUFFER;
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

use crate::committee_provider::{
    Committee,
    CommitteeProvider,
    CommitteeProviderError,
    ExecutionContext,
    Staker,
};
use crate::contract_types::{ContractStaker, RetdataDeserializationError, TryFromIterator};
use crate::staking_manager::{Epoch, StakingManager, StakingManagerConfig, MIN_EPOCH_LENGTH};
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

const EPOCH_1: Epoch = Epoch { epoch_id: 1, start_block: BlockNumber(1), epoch_length: 100 };
const EPOCH_2: Epoch = Epoch { epoch_id: 2, start_block: BlockNumber(101), epoch_length: 100 };

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

fn execute_call(
    state: &mut State,
    block_context: &Context,
    function_name: &str,
    calldata: &[Felt],
) {
    let account_address = ACCOUNT_CONTRACT.get_instance_address(0);
    let invoke_args = invoke_tx_args! {
        sender_address: account_address,
        calldata: create_calldata(STAKING_CONTRACT.get_instance_address(0), function_name, calldata),
        resource_bounds: default_all_resource_bounds(),
        nonce: state.get_nonce_at(account_address).unwrap(),
    };
    let account_tx = invoke_tx_with_default_flags(invoke_args);
    let result = account_tx.execute(state, block_context).unwrap();
    assert!(!result.execute_call_info.unwrap().execution.failed);
}

fn set_stakers(state: &mut State, block_context: &Context, stakers: &[ContractStaker]) {
    let mut raw_felts: Vec<Felt> = stakers.iter().flat_map(<Vec<Felt>>::from).collect();
    raw_felts.insert(0, Felt::from(stakers.len()));

    execute_call(state, block_context, "set_stakers", &raw_felts);
}

fn set_current_epoch(state: &mut State, block_context: &Context, epoch: Epoch) {
    execute_call(state, block_context, "set_current_epoch", &Vec::<Felt>::from(&epoch));
}

#[rstest]
fn test_min_epoch_length() {
    const {
        assert!(
            MIN_EPOCH_LENGTH >= STORED_BLOCK_HASH_BUFFER,
            "MIN_EPOCH_LENGTH must be >= STORED_BLOCK_HASH_BUFFER"
        );
    }
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
    #[case] stakers: Vec<Staker>,
    #[case] expected_committee: Committee,
) {
    set_current_epoch(&mut state, &block_context, EPOCH_1);

    let contract_stakers: Vec<ContractStaker> = stakers.iter().map(&ContractStaker::from).collect();
    set_stakers(&mut state, &block_context, &contract_stakers);

    let mut committee_manager = StakingManager::new(
        Box::new(MockBlockRandomGenerator::new()),
        StakingManagerConfig { committee_size: 3, ..default_config },
    );

    let context = ExecutionContext {
        state_reader: state.clone(),
        block_context: block_context.clone(),
        state_sync_client: Arc::new(MockStateSyncClient::new()),
    };
    let committee = committee_manager.get_committee(BlockNumber(1), context).unwrap();

    assert_eq!(*committee, expected_committee);
}

#[rstest]
fn get_committee_cache(
    default_config: StakingManagerConfig,
    mut state: State,
    block_context: Context,
) {
    set_current_epoch(&mut state, &block_context, EPOCH_1);

    let mut committee_manager = StakingManager::new(
        Box::new(MockBlockRandomGenerator::new()),
        StakingManagerConfig { max_cached_epochs: 1, ..default_config },
    );

    // Case 1: Get committee for epoch 1. Cache miss – STAKER_1 fetched from contract.
    set_stakers(&mut state, &block_context, &[ContractStaker::from(&STAKER_1)]);
    let context = ExecutionContext {
        state_reader: state.clone(),
        block_context: block_context.clone(),
        state_sync_client: Arc::new(MockStateSyncClient::new()),
    };
    let committee = committee_manager.get_committee(BlockNumber(1), context).unwrap();
    assert_eq!(*committee, vec![STAKER_1]);

    // Case 2: Query epoch 1 again. Cache hit – STAKER_1 returned from cache despite contract
    // change.
    set_current_epoch(&mut state, &block_context, EPOCH_2);
    set_stakers(&mut state, &block_context, &[ContractStaker::from(&STAKER_2)]);
    let context = ExecutionContext {
        state_reader: state.clone(),
        block_context: block_context.clone(),
        state_sync_client: Arc::new(MockStateSyncClient::new()),
    };
    let committee = committee_manager.get_committee(BlockNumber(1), context.clone()).unwrap();
    assert_eq!(*committee, vec![STAKER_1]);

    // Case 3: Get committee for epoch 2. Cache miss – new state is fetched from the contract.
    let committee = committee_manager.get_committee(BlockNumber(101), context.clone()).unwrap();
    assert_eq!(*committee, vec![STAKER_2]);

    // Case 4: Query epoch 1 again - Invalid Height error. Since the manager advanced to epoch 2 in
    // the previous step, epoch 1 is now considered too old.
    let err = committee_manager.get_committee(BlockNumber(1), context).unwrap_err();
    assert_matches!(err, CommitteeProviderError::InvalidHeight { .. });
}

#[rstest]
fn get_committee_filters_out_stakers_without_public_key(
    default_config: StakingManagerConfig,
    mut state: State,
    block_context: Context,
) {
    set_current_epoch(&mut state, &block_context, EPOCH_1);

    // Prepare the stakers that will be set in the contract. Set the public key of the first staker
    // to None.
    let mut contract_stakers: Vec<ContractStaker> =
        [STAKER_1, STAKER_2, STAKER_3].iter().map(&ContractStaker::from).collect();
    contract_stakers[0].public_key = None;

    set_stakers(&mut state, &block_context, &contract_stakers);

    let mut committee_manager = StakingManager::new(
        Box::new(MockBlockRandomGenerator::new()),
        StakingManagerConfig { committee_size: 3, ..default_config },
    );

    let context = ExecutionContext {
        state_reader: state.clone(),
        block_context: block_context.clone(),
        state_sync_client: Arc::new(MockStateSyncClient::new()),
    };
    let committee = (*committee_manager.get_committee(BlockNumber(1), context).unwrap()).clone();

    // STAKER_1 should be filtered out. Comparing HashSets since the order of the stakers is not
    // important.
    assert_eq!(committee.into_iter().collect::<HashSet<_>>(), HashSet::from([STAKER_2, STAKER_3]));
}

#[rstest]
fn get_committee_for_next_epoch(
    default_config: StakingManagerConfig,
    mut state: State,
    block_context: Context,
) {
    set_current_epoch(&mut state, &block_context, EPOCH_1);

    let contract_stakers: Vec<ContractStaker> =
        [STAKER_1, STAKER_2].iter().map(&ContractStaker::from).collect();
    set_stakers(&mut state, &block_context, &contract_stakers);

    let mut committee_manager = StakingManager::new(
        Box::new(MockBlockRandomGenerator::new()),
        StakingManagerConfig { committee_size: 3, ..default_config },
    );

    let context = ExecutionContext {
        state_reader: state.clone(),
        block_context: block_context.clone(),
        state_sync_client: Arc::new(MockStateSyncClient::new()),
    };

    // 1. Valid Query: Height 101 falls within the next epoch's min bounds.
    let committee =
        (*committee_manager.get_committee(BlockNumber(101), context.clone()).unwrap()).clone();
    assert_eq!(committee.into_iter().collect::<HashSet<_>>(), HashSet::from([STAKER_1, STAKER_2]));

    // 2. Invalid Query: Height 150 exceeds the min bounds of the next epoch.
    // Since the next epoch's length is not known at this point, we cannot know if this height
    // belongs to Epoch 2 or a future Epoch > 2.
    let err = committee_manager.get_committee(BlockNumber(150), context).unwrap_err();
    assert_matches!(err, CommitteeProviderError::InvalidHeight { .. });
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
    set_current_epoch(&mut state, &block_context, EPOCH_1);

    // The staker weights are 1000, 2000, 3000, and 4000, totaling 10,000.
    // Based on the cumulative weight ranges:
    // - Random values in [0–3999] → STAKER_4
    // - [4000–6999] → STAKER_3
    // - [7000–8999] → STAKER_2
    // - [9000–9999] → STAKER_1

    let contract_stakers: Vec<ContractStaker> =
        [STAKER_1, STAKER_2, STAKER_3, STAKER_4].iter().map(&ContractStaker::from).collect();
    set_stakers(&mut state, &block_context, &contract_stakers);

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
    set_current_epoch(&mut state, &block_context, EPOCH_1);

    // Stakers with total weight 10000.
    let contract_stakers: Vec<ContractStaker> =
        [STAKER_1, STAKER_2, STAKER_3, STAKER_4].iter().map(&ContractStaker::from).collect();
    set_stakers(&mut state, &block_context, &contract_stakers);

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

// --- TryFrom tests for contract types ---

#[rstest]
fn staker_try_from_valid() {
    let staker = ContractStaker::try_from_iter(
        &mut vec![Felt::ONE, Felt::TWO, Felt::ZERO, Felt::THREE].into_iter(),
    )
    .unwrap();
    assert_eq!(staker.contract_address, contract_address!("0x1"));
    assert_eq!(staker.staking_power, StakingWeight(2));
    assert_eq!(staker.public_key, Some(Felt::THREE));

    // A valid staker with no public key.
    let staker =
        ContractStaker::try_from_iter(&mut vec![Felt::ONE, Felt::TWO, Felt::ONE].into_iter())
            .unwrap();
    assert_eq!(staker.contract_address, contract_address!("0x1"));
    assert_eq!(staker.staking_power, StakingWeight(2));
    assert_eq!(staker.public_key, None);
}

#[rstest]
fn staker_try_from_invalid_address() {
    let err = ContractStaker::try_from_iter(
        &mut vec![CONTRACT_ADDRESS_DOMAIN_SIZE, Felt::ONE, Felt::ZERO, Felt::ONE].into_iter(),
    )
    .unwrap_err();
    assert_matches!(err, RetdataDeserializationError::ContractAddressConversionError { .. });
}

#[rstest]
fn staker_try_from_invalid_public_key() {
    let err = ContractStaker::try_from_iter(
        &mut vec![Felt::ONE, Felt::TWO, Felt::TWO, Felt::THREE].into_iter(),
    )
    .unwrap_err();
    assert_matches!(err, RetdataDeserializationError::UnexpectedEnumVariant { .. });
}

#[rstest]
fn staker_try_from_invalid_staked_amount() {
    let err = ContractStaker::try_from_iter(
        &mut vec![Felt::ONE, Felt::MAX, Felt::ZERO, Felt::ONE].into_iter(),
    )
    .unwrap_err(); // Felt::MAX is too big for u128
    assert_matches!(err, RetdataDeserializationError::U128ConversionError { .. });
}

#[rstest]
fn staker_array_retdata_try_from_valid() {
    // Case 1: No stakers.
    let retdata = Retdata(vec![Felt::ZERO]);
    assert!(ContractStaker::from_retdata_many(retdata).unwrap().is_empty());

    // Case 2: 4 Stakers, 1 with no public key.
    let mut expected_stakers: Vec<ContractStaker> =
        [STAKER_1, STAKER_2, STAKER_3, STAKER_4].iter().map(&ContractStaker::from).collect();
    expected_stakers[2].public_key = None;
    let raw_felts = [
        [Felt::from(4)].as_slice(),
        expected_stakers.iter().map(Vec::<Felt>::from).collect::<Vec<_>>().concat().as_slice(),
    ]
    .concat();

    // A sanity check that the raw felts are constructed correctly.
    // 1 felt for the number of stakers (4) + 3 stakers with public key (4 felts) + 1 staker with no
    // public key (3 felts).
    assert_eq!(raw_felts.len(), 1 + 3 * 4 + 3);

    let result = ContractStaker::from_retdata_many(Retdata(raw_felts)).unwrap();
    assert_eq!(result, expected_stakers);
}

#[rstest]
#[case::empty_retdata(vec![])]
#[case::invalid_length_1(vec![Felt::ONE; 3])]
#[case::invalid_length_2(vec![Felt::ONE; 10])]
fn staker_array_retdata_try_from_invalid_length(#[case] raw_felts: Vec<Felt>) {
    let err = ContractStaker::from_retdata_many(Retdata(raw_felts)).unwrap_err();
    assert_matches!(err, RetdataDeserializationError::InvalidObjectLength { .. });
}

#[rstest]
fn epoch_try_from_valid() {
    let epoch = Epoch::try_from(Retdata(vec![Felt::ONE, Felt::TWO, Felt::THREE])).unwrap();
    assert_eq!(epoch, Epoch { epoch_id: 1, start_block: BlockNumber(2), epoch_length: 3 });
}

#[rstest]
fn epoch_try_from_invalid_length() {
    let err = Epoch::try_from(Retdata(vec![Felt::ONE, Felt::TWO])).unwrap_err();
    assert_matches!(err, RetdataDeserializationError::InvalidObjectLength { .. });

    let err =
        Epoch::try_from(Retdata(vec![Felt::ONE, Felt::TWO, Felt::THREE, Felt::ONE])).unwrap_err();
    assert_matches!(err, RetdataDeserializationError::InvalidObjectLength { .. });
}
