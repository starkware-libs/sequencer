#![allow(clippy::unwrap_used)]

// This file is for benchmarking the committer flow.
// The input files for the different benchmarks are downloaded from GCS, using the prefix stored in
// starknet_committer_and_os_cli/src/committer_cli/tests/flow_test_files_prefix. In order to
// update them, generate a new random prefix (the hash of the initial new commit can be used) and
// update it in the mentioned file. Then upload the new files to GCS with this new prefix (run e.g.,
// gcloud storage cp LOCAL_FILE gs://committer-testing-artifacts/NEW_PREFIX/tree_flow_inputs.json).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use starknet_api::core::ContractAddress;
use starknet_api::hash::HashOutput;
use starknet_committer::block_committer::input::{Input, StarknetStorageValue};
use starknet_committer::db::external_test_utils::tree_computation_flow;
use starknet_committer::db::facts_db::types::FactsDbInitialRead;
use starknet_committer::db::facts_db::FactsNodeLayout;
use starknet_committer::hash_function::hash::TreeHashFunctionImpl;
use starknet_committer::patricia_merkle_tree::tree::OriginalSkeletonTrieConfig;
use starknet_committer_and_os_cli::committer_cli::commands::commit;
use starknet_committer_and_os_cli::committer_cli::parse_input::cast::CommitterFactsDbInputImpl;
use starknet_committer_and_os_cli::committer_cli::parse_input::read::parse_input;
use starknet_committer_and_os_cli::committer_cli::tests::parse_from_python::TreeFlowInput;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::map_storage::MapStorage;

const CONCURRENCY_MODE: bool = false;
const SINGLE_TREE_FLOW_INPUT: &str = include_str!("../test_inputs/tree_flow_inputs.json");
const FLOW_TEST_INPUT: &str = include_str!("../test_inputs/committer_flow_inputs.json");
const OUTPUT_PATH: &str = "benchmark_output.txt";
const COMMITTER_FLOW_N_TIMES: usize = 25;
const TREE_COMPUTATION_FLOW_N_TIMES: usize = 25;
const MEASUREMENT_TIME: Duration = Duration::from_secs(50);

/// Per-iteration timings for single-tree flow: each clone and the tree_computation_flow call.
struct SingleTreeIterationTimings {
    leaf_modifications_clone: Duration,
    storage_clone: Duration,
    tree_computation_flow: Duration,
}

/// Runs the tree computation flow sequentially; returns timings for each clone and call per
/// iteration.
async fn repeat_tree_computation_flow(
    leaf_modifications: LeafModifications<StarknetStorageValue>,
    storage: &MapStorage,
    root_hash: HashOutput,
    config: OriginalSkeletonTrieConfig,
    contract_address: &ContractAddress,
    n_times: usize,
) -> Vec<SingleTreeIterationTimings> {
    let mut results = Vec::with_capacity(n_times);
    for _ in 0..n_times {
        let t0 = Instant::now();
        let lm = leaf_modifications.clone();
        let leaf_modifications_clone = t0.elapsed();

        let t1 = Instant::now();
        let mut storage_copy = MapStorage(storage.0.clone());
        let storage_clone = t1.elapsed();

        let t2 = Instant::now();
        tree_computation_flow::<StarknetStorageValue, FactsNodeLayout, TreeHashFunctionImpl>(
            lm,
            &mut storage_copy,
            root_hash,
            config.clone(),
            contract_address,
        )
        .await;
        let tree_computation_flow = t2.elapsed();

        results.push(SingleTreeIterationTimings {
            leaf_modifications_clone,
            storage_clone,
            tree_computation_flow,
        });
    }
    results
}

pub fn single_tree_flow_benchmark(criterion: &mut Criterion) {
    let TreeFlowInput { leaf_modifications, storage, root_hash } =
        serde_json::from_str(SINGLE_TREE_FLOW_INPUT).unwrap();
    let runtime = match CONCURRENCY_MODE {
        true => tokio::runtime::Builder::new_multi_thread().build().unwrap(),
        false => tokio::runtime::Builder::new_current_thread().build().unwrap(),
    };

    let leaf_modifications = leaf_modifications
        .into_iter()
        .map(|(k, v)| (NodeIndex::FIRST_LEAF + k, v))
        .collect::<LeafModifications<StarknetStorageValue>>();

    let dummy_contract_address = ContractAddress::from(0_u128);
    let leaf_clone_timings: Rc<RefCell<Vec<Duration>>> = Rc::new(RefCell::new(Vec::new()));
    let storage_clone_timings: Rc<RefCell<Vec<Duration>>> = Rc::new(RefCell::new(Vec::new()));
    let tree_flow_timings: Rc<RefCell<Vec<Duration>>> = Rc::new(RefCell::new(Vec::new()));
    let sample_total_timings: Rc<RefCell<Vec<Duration>>> = Rc::new(RefCell::new(Vec::new()));

    let leaf_clone_timings_cl = Rc::clone(&leaf_clone_timings);
    let storage_clone_timings_cl = Rc::clone(&storage_clone_timings);
    let tree_flow_timings_cl = Rc::clone(&tree_flow_timings);
    let sample_total_timings_cl = Rc::clone(&sample_total_timings);

    criterion.bench_function("tree_computation_flow", move |benchmark| {
        benchmark.iter_batched(
            || leaf_modifications.clone(),
            |leaf_modifications_input| {
                let sample_start = Instant::now();
                let timings = runtime.block_on(repeat_tree_computation_flow(
                    leaf_modifications_input,
                    &storage,
                    root_hash,
                    OriginalSkeletonTrieConfig::new_for_classes_or_storage_trie(false),
                    &dummy_contract_address,
                    TREE_COMPUTATION_FLOW_N_TIMES,
                ));
                sample_total_timings_cl.borrow_mut().push(sample_start.elapsed());
                for t in timings {
                    leaf_clone_timings_cl.borrow_mut().push(t.leaf_modifications_clone);
                    storage_clone_timings_cl.borrow_mut().push(t.storage_clone);
                    tree_flow_timings_cl.borrow_mut().push(t.tree_computation_flow);
                }
            },
            BatchSize::LargeInput,
        )
    });

    fn sum_durations(durations: &[Duration]) -> Duration {
        durations.iter().fold(Duration::ZERO, |a, b| a + *b)
    }
    fn print_op_summary(name: &str, times: &[Duration]) {
        let sum = sum_durations(times);
        let count = times.len();
        let avg = if count > 0 { sum / count as u32 } else { Duration::ZERO };
        eprintln!("[single_tree] {}: sum={:?}, iterations={}, avg={:?}", name, sum, count, avg);
    }

    let total_benchmark = sum_durations(&sample_total_timings.borrow());
    if total_benchmark > Duration::ZERO {
        eprintln!("=== single_tree_flow_benchmark timing summary ===");
        print_op_summary("leaf_modifications_clone", &leaf_clone_timings.borrow());
        print_op_summary("storage_clone", &storage_clone_timings.borrow());
        print_op_summary("tree_computation_flow", &tree_flow_timings.borrow());
        eprintln!(
            "[single_tree] total_benchmark_time: {:?} (sum of all sample durations)",
            total_benchmark
        );
        eprintln!("==================================================");
    }
}

/// Per-iteration timings for full committer: each clone and the commit call.
struct FullCommitterIterationTimings {
    input_clone: Duration,
    output_path_clone: Duration,
    storage_clone: Duration,
    commit_call: Duration,
}

/// Runs the commit flow sequentially; returns timings for each clone and call per iteration.
async fn repeat_commit(
    input: Input<FactsDbInitialRead>,
    output_path: String,
    storage: MapStorage,
    n_times: usize,
) -> Vec<FullCommitterIterationTimings> {
    let mut results = Vec::with_capacity(n_times);
    for _ in 0..n_times {
        let t0 = Instant::now();
        let input_copy = input.clone();
        let input_clone = t0.elapsed();

        let t1 = Instant::now();
        let output_path_copy = output_path.clone();
        let output_path_clone = t1.elapsed();

        let t2 = Instant::now();
        let storage_copy = MapStorage(storage.0.clone());
        let storage_clone = t2.elapsed();

        let t3 = Instant::now();
        commit(input_copy, output_path_copy, storage_copy).await;
        let commit_call = t3.elapsed();

        results.push(FullCommitterIterationTimings {
            input_clone,
            output_path_clone,
            storage_clone,
            commit_call,
        });
    }
    results
}

pub fn full_committer_flow_benchmark(criterion: &mut Criterion) {
    let runtime = match CONCURRENCY_MODE {
        true => tokio::runtime::Builder::new_multi_thread().build().unwrap(),
        false => tokio::runtime::Builder::new_current_thread().build().unwrap(),
    };

    // TODO(Aner, 8/7/2024): use structs for deserialization.
    let input: HashMap<String, String> = serde_json::from_str(FLOW_TEST_INPUT).unwrap();
    let committer_input_string = input.get("committer_input").unwrap();

    // TODO(Aner, 27/06/2024): output path should be a pipe (file on memory)
    // to avoid disk IO in the benchmark.
    let parse_timings: RefCell<Vec<Duration>> = RefCell::new(Vec::new());
    let input_clone_timings: RefCell<Vec<Duration>> = RefCell::new(Vec::new());
    let output_path_clone_timings: RefCell<Vec<Duration>> = RefCell::new(Vec::new());
    let storage_clone_timings: RefCell<Vec<Duration>> = RefCell::new(Vec::new());
    let commit_call_timings: RefCell<Vec<Duration>> = RefCell::new(Vec::new());
    let sample_total_timings: RefCell<Vec<Duration>> = RefCell::new(Vec::new());

    criterion.bench_function("full_committer_flow", |benchmark| {
        benchmark.iter(|| {
            runtime.block_on(async {
                let sample_start = Instant::now();

                let parse_start = Instant::now();
                let CommitterFactsDbInputImpl { input, storage, .. } =
                    parse_input(committer_input_string).expect("Failed to parse the given input.");
                parse_timings.borrow_mut().push(parse_start.elapsed());

                let timings =
                    repeat_commit(input, OUTPUT_PATH.to_owned(), storage, COMMITTER_FLOW_N_TIMES)
                        .await;
                sample_total_timings.borrow_mut().push(sample_start.elapsed());

                for t in timings {
                    input_clone_timings.borrow_mut().push(t.input_clone);
                    output_path_clone_timings.borrow_mut().push(t.output_path_clone);
                    storage_clone_timings.borrow_mut().push(t.storage_clone);
                    commit_call_timings.borrow_mut().push(t.commit_call);
                }
            });
        })
    });

    fn sum_durations(durations: &[Duration]) -> Duration {
        durations.iter().fold(Duration::ZERO, |a, b| a + *b)
    }
    fn print_op_summary(name: &str, times: &[Duration]) {
        let sum = sum_durations(times);
        let count = times.len();
        let avg = if count > 0 { sum / count as u32 } else { Duration::ZERO };
        eprintln!("[full_committer] {}: sum={:?}, iterations={}, avg={:?}", name, sum, count, avg);
    }

    let total_benchmark = sum_durations(&sample_total_timings.borrow());
    if total_benchmark > Duration::ZERO {
        eprintln!("=== full_committer_flow_benchmark timing summary ===");
        print_op_summary("parse_input", &parse_timings.borrow());
        print_op_summary("input_clone", &input_clone_timings.borrow());
        print_op_summary("output_path_clone", &output_path_clone_timings.borrow());
        print_op_summary("storage_clone", &storage_clone_timings.borrow());
        print_op_summary("commit_call", &commit_call_timings.borrow());
        eprintln!(
            "[full_committer] total_benchmark_time: {:?} (sum of all sample durations)",
            total_benchmark
        );
        eprintln!("=====================================================");
    }
}

criterion_group!(
    name = benches;
    config = Criterion::default().measurement_time(MEASUREMENT_TIME);
    targets = single_tree_flow_benchmark, full_committer_flow_benchmark
);
criterion_main!(benches);
