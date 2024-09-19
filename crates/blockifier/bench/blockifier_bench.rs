//! Benchmark module for the blockifier crate. It provides functionalities to benchmark
//! various aspects related to transferring between accounts, including preparation
//! and execution of transfers.
//!
//! The benchmark function `transfers_benchmark` measures the performance of transfers between
//! randomly created accounts, which are iterated over round-robin.
//!
//! The benchmark function `execution_benchmark` measures the performance of the method
//! [`blockifier::transactions::transaction::ExecutableTransaction::execute`] by executing the entry
//! point `advance_counter` of the test contract.
//!
//! The benchmark function `cached_state_benchmark` measures the performance of
//! [`blockifier::state::cached_state::CachedState::add_visited_pcs`] method using a realistic size
//! of data.
//!
//! Run the benchmarks using `cargo bench --bench blockifier_bench`.

use std::time::Duration;

use blockifier::context::BlockContext;
use blockifier::state::cached_state::{CachedState, TransactionalState};
use blockifier::state::state_api::State;
use blockifier::state::visited_pcs::VisitedPcsSet;
use blockifier::test_utils::contracts::FeatureContract;
use blockifier::test_utils::dict_state_reader::DictStateReader;
use blockifier::test_utils::initial_test_state::test_state;
use blockifier::test_utils::transfers_generator::{
    RecipientGeneratorType,
    TransfersGenerator,
    TransfersGeneratorConfig,
};
use blockifier::test_utils::{
    create_calldata,
    CairoVersion,
    BALANCE,
    MAX_L1_GAS_AMOUNT,
    MAX_L1_GAS_PRICE,
};
use blockifier::transaction::account_transaction::AccountTransaction;
use blockifier::transaction::test_utils::{account_invoke_tx, block_context, l1_resource_bounds};
use blockifier::transaction::transactions::ExecutableTransaction;
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use starknet_api::core::ClassHash;
use starknet_api::hash::StarkHash;
use starknet_api::test_utils::NonceManager;
use starknet_api::{felt, invoke_tx_args};

pub fn transfers_benchmark(c: &mut Criterion) {
    let transfers_generator_config = TransfersGeneratorConfig {
        recipient_generator_type: RecipientGeneratorType::Random,
        ..Default::default()
    };
    let mut transfers_generator = TransfersGenerator::new(transfers_generator_config);
    // Create a benchmark group called "transfers", which iterates over the accounts round-robin
    // and performs transfers.
    c.bench_function("transfers", |benchmark| {
        benchmark.iter(|| {
            transfers_generator.execute_transfers();
        })
    });
}

pub fn cached_state_benchmark(c: &mut Criterion) {
    fn get_random_array(size: usize) -> Vec<usize> {
        let mut vec: Vec<usize> = Vec::with_capacity(size);
        for _ in 0..vec.capacity() {
            vec.push(rand::random());
        }
        vec
    }

    fn create_class_hash(class_hash: &str) -> ClassHash {
        ClassHash(StarkHash::from_hex_unchecked(class_hash))
    }

    // The state shared across all iterations.
    let mut cached_state: CachedState<DictStateReader, VisitedPcsSet> = CachedState::default();

    c.bench_function("cached_state", move |benchmark| {
        benchmark.iter_batched(
            || {
                // This anonymous function creates the simulated visited program counters to add in
                // `cached_state`.
                // The numbers are taken from tx hash
                // 0x0177C9365875CAA840EA8F03F97B0E3A8EE8851A8B952BF157B5DBD4FECCB060. This
                // transaction has been chosen randomly, but it may not be representative of the
                // average transaction on Starknet.

                let mut class_hashes = Vec::new();
                let mut random_arrays = Vec::new();

                let class_hash = create_class_hash("a");
                let random_array = get_random_array(11393);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("a");
                let random_array = get_random_array(453);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("a");
                let random_array = get_random_array(604);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("a");
                let random_array = get_random_array(806);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("b");
                let random_array = get_random_array(1327);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("b");
                let random_array = get_random_array(1135);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("b");
                let random_array = get_random_array(213);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("b");
                let random_array = get_random_array(135);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("c");
                let random_array = get_random_array(348);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("c");
                let random_array = get_random_array(88);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("c");
                let random_array = get_random_array(348);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("c");
                let random_array = get_random_array(348);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("d");
                let random_array = get_random_array(875);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("d");
                let random_array = get_random_array(450);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("d");
                let random_array = get_random_array(255);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("d");
                let random_array = get_random_array(210);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("d");
                let random_array = get_random_array(1403);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("d");
                let random_array = get_random_array(210);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("d");
                let random_array = get_random_array(210);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("e");
                let random_array = get_random_array(2386);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                let class_hash = create_class_hash("e");
                let random_array = get_random_array(3602);
                class_hashes.push(class_hash);
                random_arrays.push(random_array);

                (class_hashes, random_arrays)
            },
            |input_data| {
                let mut transactional_state =
                    TransactionalState::create_transactional(&mut cached_state);
                for (class_hash, random_array) in input_data.0.into_iter().zip(input_data.1) {
                    transactional_state.add_visited_pcs(class_hash, &random_array);
                }
                transactional_state.commit();
            },
            BatchSize::SmallInput,
        )
    });
}

pub fn execution_benchmark(c: &mut Criterion) {
    /// This function sets up and returns all the objects required to execute an invoke transaction.
    fn prepare_account_tx()
    -> (AccountTransaction, CachedState<DictStateReader, VisitedPcsSet>, BlockContext) {
        let block_context = block_context();
        let max_resource_bounds = l1_resource_bounds(MAX_L1_GAS_AMOUNT, MAX_L1_GAS_PRICE);
        let cairo_version = CairoVersion::Cairo1;
        let account = FeatureContract::AccountWithoutValidations(cairo_version);
        let test_contract = FeatureContract::TestContract(cairo_version);
        let state =
            test_state(block_context.chain_info(), BALANCE, &[(account, 1), (test_contract, 1)]);
        let account_address = account.get_instance_address(0);
        let contract_address = test_contract.get_instance_address(0);
        let index = felt!(123_u32);
        let base_tx_args = invoke_tx_args! {
            resource_bounds: max_resource_bounds,
            sender_address: account_address,
        };

        let mut nonce_manager = NonceManager::default();
        let counter_diffs = [101_u32, 102_u32];
        let initial_counters = [felt!(counter_diffs[0]), felt!(counter_diffs[1])];
        let calldata_args = vec![index, initial_counters[0], initial_counters[1]];

        let account_tx = account_invoke_tx(invoke_tx_args! {
            nonce: nonce_manager.next(account_address),
            calldata:
                create_calldata(contract_address, "advance_counter", &calldata_args),
            ..base_tx_args
        });
        (account_tx, state, block_context)
    }
    c.bench_function("execution", move |benchmark| {
        benchmark.iter_batched(
            prepare_account_tx,
            |(account_tx, mut state, block_context)| {
                account_tx.execute(&mut state, &block_context, true, true).unwrap()
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(20));
    targets = transfers_benchmark, execution_benchmark, cached_state_benchmark
}
criterion_main!(benches);
