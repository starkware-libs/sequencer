use std::sync::{Arc, Mutex};

use assert_matches::assert_matches;
use blockifier_test_utils::cairo_versions::{CairoVersion, RunnableCairo1};
use blockifier_test_utils::calldata::create_calldata;
use blockifier_test_utils::contracts::FeatureContract;
use pretty_assertions::assert_eq;
use rstest::rstest;
use starknet_api::contract_class::compiled_class_hash::HashVersion;
use starknet_api::test_utils::declare::executable_declare_tx;
use starknet_api::test_utils::deploy_account::executable_deploy_account_tx;
use starknet_api::test_utils::invoke::executable_invoke_tx;
use starknet_api::test_utils::DEFAULT_STRK_L1_GAS_PRICE;
use starknet_api::transaction::fields::{Fee, ValidResourceBounds};
use starknet_api::transaction::TransactionVersion;
use starknet_api::{declare_tx_args, deploy_account_tx_args, felt, invoke_tx_args, nonce};
use starknet_types_core::felt::Felt;

use crate::blockifier::config::TransactionExecutorConfig;
use crate::blockifier::transaction_executor::{
    TransactionExecutor,
    TransactionExecutorError,
    BLOCK_STATE_ACCESS_ERR,
};
use crate::bouncer::{Bouncer, BouncerWeights};
use crate::concurrency::worker_pool::WorkerPool;
use crate::context::BlockContext;
use crate::state::cached_state::CachedState;
use crate::state::state_api::StateReader;
use crate::test_utils::contracts::FeatureContractTrait;
use crate::test_utils::initial_test_state::test_state_with_contract_manager;
use crate::test_utils::l1_handler::l1handler_tx;
use crate::test_utils::{maybe_dummy_block_hash_and_number, BALANCE};
use crate::transaction::account_transaction::AccountTransaction;
use crate::transaction::errors::TransactionExecutionError;
use crate::transaction::test_utils::{
    block_context,
    calculate_class_info_for_testing,
    create_test_init_data,
    emit_n_events_tx,
    l1_resource_bounds,
    TestInitData,
};
use crate::transaction::transaction_execution::Transaction;

fn tx_executor_test_body<S: StateReader>(
    state: CachedState<S>,
    block_context: BlockContext,
    tx: Transaction,
    expected_bouncer_weights: BouncerWeights,
) {
    let block_number_hash_pair =
        maybe_dummy_block_hash_and_number(block_context.block_info().block_number);
    let mut tx_executor = TransactionExecutor::pre_process_and_create(
        state,
        block_context,
        block_number_hash_pair,
        TransactionExecutorConfig::default(),
    )
    .unwrap();
    // TODO(Arni, 30/03/2024): Consider adding a test for the transaction execution info. If A test
    // should not be added, rename the test to `test_bouncer_info`.
    // TODO(Arni, 30/03/2024): Test all bouncer weights.
    let _tx_execution_output = tx_executor.execute(&tx).unwrap();
    let bouncer = tx_executor.bouncer.lock().unwrap();
    let bouncer_weights = bouncer.get_bouncer_weights();
    assert_eq!(bouncer_weights.state_diff_size, expected_bouncer_weights.state_diff_size);
    assert_eq!(
        bouncer_weights.message_segment_length,
        expected_bouncer_weights.message_segment_length
    );
    assert_eq!(bouncer_weights.n_events, expected_bouncer_weights.n_events);
}

#[rstest]
#[case::tx_version_0(
    TransactionVersion::ZERO,
    CairoVersion::Cairo0,
    BouncerWeights {
        state_diff_size: 0,
        message_segment_length: 0,
        n_events: 0,
        ..BouncerWeights::empty()
    }
)]
#[case::tx_version_1(
    TransactionVersion::ONE,
    CairoVersion::Cairo0,
    BouncerWeights {
        state_diff_size: 2,
        message_segment_length: 0,
        n_events: 0,
        ..BouncerWeights::empty()
    }
)]
#[case::tx_version_2(
    TransactionVersion::TWO,
    CairoVersion::Cairo1(RunnableCairo1::Casm),
    BouncerWeights {
        state_diff_size: 4,
        message_segment_length: 0,
        n_events: 0,
        ..BouncerWeights::empty()
    }
)]
#[case::tx_version_3(
    TransactionVersion::THREE,
    CairoVersion::Cairo1(RunnableCairo1::Casm),
    BouncerWeights {
        state_diff_size: 4,
        message_segment_length: 0,
        n_events: 0,
        ..BouncerWeights::empty()
    }
)]
fn test_declare(
    block_context: BlockContext,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1(RunnableCairo1::Casm))]
    account_cairo_version: CairoVersion,
    #[case] tx_version: TransactionVersion,
    #[case] cairo_version: CairoVersion,
    #[case] expected_bouncer_weights: BouncerWeights,
) {
    let account_contract = FeatureContract::AccountWithoutValidations(account_cairo_version);
    let declared_contract = FeatureContract::Empty(cairo_version);
    let state = test_state_with_contract_manager(
        &block_context.chain_info,
        BALANCE,
        &[(account_contract, 1)],
    );

    let declare_tx = executable_declare_tx(
        declare_tx_args! {
            sender_address: account_contract.get_instance_address(0),
            class_hash: declared_contract.get_class_hash(),
            compiled_class_hash: declared_contract.get_compiled_class_hash(&HashVersion::V2),
            version: tx_version,
            resource_bounds: l1_resource_bounds(0_u8.into(), DEFAULT_STRK_L1_GAS_PRICE.into()),
        },
        calculate_class_info_for_testing(declared_contract.get_class()),
    );
    let tx = AccountTransaction::new_for_sequencing(declare_tx).into();
    tx_executor_test_body(state, block_context, tx, expected_bouncer_weights);
}

#[rstest]
fn test_deploy_account(
    block_context: BlockContext,
    #[values(TransactionVersion::ONE, TransactionVersion::THREE)] version: TransactionVersion,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1(RunnableCairo1::Casm))]
    cairo_version: CairoVersion,
) {
    let account_contract = FeatureContract::AccountWithoutValidations(cairo_version);
    let state = test_state_with_contract_manager(
        &block_context.chain_info,
        BALANCE,
        &[(account_contract, 0)],
    );

    let deploy_account_tx = executable_deploy_account_tx(deploy_account_tx_args! {
        class_hash: account_contract.get_class_hash(),
        resource_bounds: l1_resource_bounds(0_u8.into(), DEFAULT_STRK_L1_GAS_PRICE.into()),
        version,
    });
    let tx = AccountTransaction::new_for_sequencing(deploy_account_tx).into();
    let expected_bouncer_weights = BouncerWeights {
        state_diff_size: 3,
        message_segment_length: 0,
        n_events: 0,
        ..BouncerWeights::empty()
    };
    tx_executor_test_body(state, block_context, tx, expected_bouncer_weights);
}

#[rstest]
#[case::invoke_function_base_case(
    "assert_eq",
    vec![
        felt!(3_u32), // x.
        felt!(3_u32)  // y.
    ],
    BouncerWeights {
        state_diff_size: 2,
        message_segment_length: 0,
        n_events: 0,
        ..BouncerWeights::empty()
    }
)]
#[case::emit_event_syscall(
    "test_emit_events",
    vec![
        felt!(1_u32), // events_number.
        felt!(0_u32), // keys length.
        felt!(0_u32)  // data length.
    ],
    BouncerWeights {
        state_diff_size: 2,
        message_segment_length: 0,
        n_events: 1,
        ..BouncerWeights::empty()
    }
)]
#[case::storage_write_syscall(
    "test_count_actual_storage_changes",
    vec![],
    BouncerWeights {
        state_diff_size: 6,
        message_segment_length: 0,
        n_events: 0,
        ..BouncerWeights::empty()
    }
)]
fn test_invoke(
    block_context: BlockContext,
    #[values(TransactionVersion::ONE, TransactionVersion::THREE)] version: TransactionVersion,
    #[values(CairoVersion::Cairo0, CairoVersion::Cairo1(RunnableCairo1::Casm))]
    cairo_version: CairoVersion,
    #[case] entry_point_name: &str,
    #[case] entry_point_args: Vec<Felt>,
    #[case] expected_bouncer_weights: BouncerWeights,
) {
    let test_contract = FeatureContract::TestContract(cairo_version);
    let account_contract = FeatureContract::AccountWithoutValidations(cairo_version);
    let state = test_state_with_contract_manager(
        &block_context.chain_info,
        BALANCE,
        &[(test_contract, 1), (account_contract, 1)],
    );

    let calldata =
        create_calldata(test_contract.get_instance_address(0), entry_point_name, &entry_point_args);
    let invoke_tx = executable_invoke_tx(invoke_tx_args! {
        sender_address: account_contract.get_instance_address(0),
        calldata,
        version,
        resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
    });
    let tx = AccountTransaction::new_for_sequencing(invoke_tx).into();
    tx_executor_test_body(state, block_context, tx, expected_bouncer_weights);
}

#[rstest]
fn test_l1_handler(block_context: BlockContext) {
    let test_contract = FeatureContract::TestContract(CairoVersion::Cairo1(RunnableCairo1::Casm));
    let state =
        test_state_with_contract_manager(&block_context.chain_info, BALANCE, &[(test_contract, 1)]);

    let tx = Transaction::L1Handler(l1handler_tx(
        Fee(1908000000000000),
        test_contract.get_instance_address(0),
    ));
    let expected_bouncer_weights = BouncerWeights {
        state_diff_size: 4,
        message_segment_length: 7,
        n_events: 0,
        ..BouncerWeights::empty()
    };
    tx_executor_test_body(state, block_context, tx, expected_bouncer_weights);
}

#[rstest]
#[case::happy_flow(BouncerWeights::empty(), 10)]
#[should_panic(expected = "BlockFull: Transaction cannot be added to the current block, block \
                           capacity reached.")]
#[case::block_full(
    BouncerWeights {
        n_events: 4,
        ..BouncerWeights::empty()
    },
    7
)]
#[should_panic(expected = "Transaction size exceeds the maximum block capacity.")]
#[case::transaction_too_large(BouncerWeights::empty(), 11)]

fn test_bouncing(#[case] initial_bouncer_weights: BouncerWeights, #[case] n_events: usize) {
    let max_n_events_in_block = 10;
    let block_context = BlockContext::create_for_bouncer_testing(max_n_events_in_block);

    let TestInitData { state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(
            &block_context.chain_info,
            CairoVersion::Cairo1(RunnableCairo1::Casm),
        );

    // TODO(Yoni, 15/6/2024): turn on concurrency mode.
    let mut tx_executor =
        TransactionExecutor::new(state, block_context, TransactionExecutorConfig::default());

    tx_executor.bouncer.lock().unwrap().set_bouncer_weights(initial_bouncer_weights);

    tx_executor
        .execute(
            &emit_n_events_tx(
                n_events,
                account_address,
                contract_address,
                nonce_manager.next(account_address),
            )
            .into(),
        )
        .map_err(|error| panic!("{error:?}: {error}"))
        .unwrap();
}

#[rstest]
#[case(false, false)]
#[case(true, false)]
#[case(true, true)]
fn test_execute_txs_bouncing(#[case] concurrency_enabled: bool, #[case] external_pool: bool) {
    let config = TransactionExecutorConfig::create_for_testing(concurrency_enabled);
    let max_n_events_in_block = 10;
    let block_context = BlockContext::create_for_bouncer_testing(max_n_events_in_block);

    let TestInitData { state, account_address, contract_address, .. } = create_test_init_data(
        &block_context.chain_info,
        CairoVersion::Cairo1(RunnableCairo1::Casm),
    );

    let pool = if external_pool {
        Some(Arc::new(WorkerPool::start(&config.get_worker_pool_config())))
    } else {
        None
    };

    let mut tx_executor = TransactionExecutor::new_with_pool(state, block_context, config, pool);

    let txs: Vec<Transaction> = [
        emit_n_events_tx(1, account_address, contract_address, nonce!(0_u32)),
        // Transaction too big.
        emit_n_events_tx(
            max_n_events_in_block + 1,
            account_address,
            contract_address,
            nonce!(1_u32),
        ),
        emit_n_events_tx(8, account_address, contract_address, nonce!(1_u32)),
        // No room for this in block - execution should halt.
        emit_n_events_tx(2, account_address, contract_address, nonce!(2_u32)),
        // Has room for this one, but should not be processed at all.
        emit_n_events_tx(1, account_address, contract_address, nonce!(3_u32)),
    ]
    .into_iter()
    .map(Transaction::Account)
    .collect();

    // Run.
    let results = tx_executor.execute_txs(&txs, None);

    // Check execution results.
    let expected_offset = 3;
    assert_eq!(results.len(), expected_offset);

    assert!(results[0].is_ok());
    assert_matches!(
        results[1].as_ref().unwrap_err(),
        TransactionExecutorError::TransactionExecutionError(
            TransactionExecutionError::TransactionTooLarge { .. }
        )
    );
    assert!(results[2].is_ok());

    // Check state.
    assert_eq!(
        tx_executor
            .block_state
            .as_ref()
            .expect(BLOCK_STATE_ACCESS_ERR)
            .get_nonce_at(account_address)
            .unwrap(),
        nonce!(2_u32)
    );

    // Check idempotency: excess transactions should not be added.
    let remaining_txs = &txs[expected_offset..];
    let remaining_tx_results = tx_executor.execute_txs(remaining_txs, None);
    assert_eq!(remaining_tx_results.len(), 0);

    // Reset the bouncer and add the remaining transactions.
    tx_executor.bouncer =
        Mutex::new(Bouncer::new(tx_executor.block_context.bouncer_config.clone())).into();
    let remaining_tx_results = tx_executor.execute_txs(remaining_txs, None);

    assert_eq!(remaining_tx_results.len(), 2);
    assert!(remaining_tx_results[0].is_ok());
    assert!(remaining_tx_results[1].is_ok());
    assert_eq!(
        tx_executor
            .block_state
            .as_ref()
            .expect(BLOCK_STATE_ACCESS_ERR)
            .get_nonce_at(account_address)
            .unwrap(),
        nonce!(4_u32)
    );

    // End test by calling pool.join(), if pool is used.
    if let Some(pool) = tx_executor.worker_pool {
        Arc::try_unwrap(pool).expect("More than one instance of worker pool exists").join();
    }
}

#[cfg(feature = "cairo_native")]
#[rstest::rstest]
/// Tests that Native can handle deep recursion calls without causing a stack overflow.
/// The recursive function must be complex enough to prevent the compiler from optimizing it into a
/// loop. This function was manually tested with increased maximum gas to ensure it reaches a stack
/// overflow.
///
/// Note: Testing the VM is unnecessary here as it simulates the stack where the stack in the heap
/// as a memory segment.
fn test_stack_overflow(#[values(true, false)] concurrency_enabled: bool) {
    let block_context = BlockContext::create_for_account_testing();
    let cairo_version = CairoVersion::Cairo1(RunnableCairo1::Native);
    let TestInitData { state, account_address, contract_address, mut nonce_manager } =
        create_test_init_data(&block_context.chain_info, cairo_version);
    let depth = felt!(1000000_u128);
    let entry_point_args = vec![depth];
    let calldata = create_calldata(contract_address, "test_stack_overflow", &entry_point_args);
    let invoke_tx = executable_invoke_tx(invoke_tx_args! {
        sender_address: account_address,
        calldata,
        nonce: nonce_manager.next(account_address),
        resource_bounds: ValidResourceBounds::create_for_testing_no_fee_enforcement(),
    });
    let account_tx = AccountTransaction::new_for_sequencing(invoke_tx);
    // Ensure the transaction is allocated the maximum gas limits.
    assert!(
        account_tx.resource_bounds().get_l2_bounds().max_amount
            >= block_context.versioned_constants.os_constants.execute_max_sierra_gas
                + block_context.versioned_constants.os_constants.validate_max_sierra_gas
    );
    // Run.
    let config = TransactionExecutorConfig::create_for_testing(concurrency_enabled);
    let mut executor = TransactionExecutor::new(state, block_context, config);
    let results = executor.execute_txs(&vec![account_tx.into()], None);

    let (tx_execution_info, _state_diff) = results[0].as_ref().unwrap();
    assert!(tx_execution_info.is_reverted());
    let err = tx_execution_info.revert_error.clone().unwrap().to_string();

    // Recursion is terminated by resource bounds before stack overflow occurs.
    assert!(err.contains("'Out of gas'"));
}
