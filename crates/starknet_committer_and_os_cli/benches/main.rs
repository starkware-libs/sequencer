#![allow(clippy::unwrap_used)]

// This file is for benchmarking the committer flow.
// The input files for the different benchmarks are downloaded from GCS, using the prefix stored in
// starknet_committer_and_os_cli/src/committer_cli/tests/flow_test_files_prefix. In order to
// update them, generate a new random prefix (the hash of the initial new commit can be used) and
// update it in the mentioned file. Then upload the new files to GCS with this new prefix (run e.g.,
// gcloud storage cp LOCAL_FILE gs://committer-testing-artifacts/NEW_PREFIX/tree_flow_inputs.json).

use std::collections::HashMap;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use starknet_committer::block_committer::input::StarknetStorageValue;
use starknet_committer::hash_function::hash::TreeHashFunctionImpl;
use starknet_committer::patricia_merkle_tree::tree::OriginalSkeletonStorageTrieConfig;
use starknet_committer_and_os_cli::committer_cli::commands::commit;
use starknet_committer_and_os_cli::committer_cli::parse_input::cast::CommitterInputImpl;
use starknet_committer_and_os_cli::committer_cli::parse_input::read::parse_input;
use starknet_committer_and_os_cli::committer_cli::tests::utils::parse_from_python::TreeFlowInput;
use starknet_patricia::patricia_merkle_tree::external_test_utils::tree_computation_flow;
use starknet_patricia::patricia_merkle_tree::node_data::leaf::LeafModifications;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::map_storage::BorrowedMapStorage;

const CONCURRENCY_MODE: bool = true;
const SINGLE_TREE_FLOW_INPUT: &str = include_str!("../test_inputs/tree_flow_inputs.json");
const FLOW_TEST_INPUT: &str = include_str!("../test_inputs/committer_flow_inputs.json");
const OUTPUT_PATH: &str = "benchmark_output.txt";

pub fn single_tree_flow_benchmark(criterion: &mut Criterion) {
    let TreeFlowInput { leaf_modifications, mut storage, root_hash } =
        serde_json::from_str(SINGLE_TREE_FLOW_INPUT).unwrap();
<<<<<<< HEAD
||||||| 01792faa8

=======
    let storage = BorrowedMapStorage { storage: &mut storage };
>>>>>>> origin/main-v0.14.1
    let runtime = match CONCURRENCY_MODE {
        true => tokio::runtime::Builder::new_multi_thread().build().unwrap(),
        false => tokio::runtime::Builder::new_current_thread().build().unwrap(),
    };

    let leaf_modifications = leaf_modifications
        .into_iter()
        .map(|(k, v)| (NodeIndex::FIRST_LEAF + k, v))
        .collect::<LeafModifications<StarknetStorageValue>>();

    criterion.bench_function("tree_computation_flow", move |benchmark| {
        benchmark.iter_batched(
            || leaf_modifications.clone(),
            |leaf_modifications_input| {
                runtime.block_on(
                    tree_computation_flow::<StarknetStorageValue, TreeHashFunctionImpl>(
                        leaf_modifications_input,
                        &storage,
                        root_hash,
                        OriginalSkeletonStorageTrieConfig::new(false),
                    ),
                );
            },
            BatchSize::LargeInput,
        )
    });
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
    criterion.bench_function("full_committer_flow", |benchmark| {
        benchmark.iter(|| {
            runtime.block_on({
                let CommitterInputImpl { input, storage } =
                    parse_input(committer_input_string).expect("Failed to parse the given input.");
                // Set the given log level if handle is passed.
                commit(input, OUTPUT_PATH.to_owned(), storage)
            });
        })
    });
}

criterion_group!(benches, single_tree_flow_benchmark, full_committer_flow_benchmark);
criterion_main!(benches);
