#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use committer::{
    block_committer::input::StarknetStorageValue,
    patricia_merkle_tree::{
        external_test_utils::tree_computation_flow, node_data::leaf::LeafModifications,
        types::NodeIndex,
    },
};
use committer_cli::{
    commands::commit, tests::utils::parse_from_python::parse_input_single_storage_tree_flow_test,
};
use criterion::{criterion_group, criterion_main, Criterion};

const CONCURRENCY_MODE: bool = true;
const SINGLE_TREE_FLOW_INPUT: &str = include_str!("tree_flow_inputs.json");
const FLOW_TEST_INPUT: &str = include_str!("committer_flow_inputs.json");

pub fn single_tree_flow_benchmark(criterion: &mut Criterion) {
    let (leaf_modifications, storage, root_hash) = parse_input_single_storage_tree_flow_test(
        &serde_json::from_str(SINGLE_TREE_FLOW_INPUT).unwrap(),
    );

    let runtime = match CONCURRENCY_MODE {
        true => tokio::runtime::Builder::new_multi_thread().build().unwrap(),
        false => tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap(),
    };

    let leaf_modifications = leaf_modifications
        .into_iter()
        .map(|(k, v)| (NodeIndex::FIRST_LEAF + k, v))
        .collect::<LeafModifications<StarknetStorageValue>>();
    let arc_leaf_modifications = Arc::new(leaf_modifications);

    criterion.bench_function("tree_computation_flow", |benchmark| {
        benchmark.iter(|| {
            runtime.block_on(tree_computation_flow(
                Arc::clone(&arc_leaf_modifications),
                &storage,
                root_hash,
            ));
        })
    });
}

pub fn full_committer_flow_benchmark(criterion: &mut Criterion) {
    let runtime = match CONCURRENCY_MODE {
        true => tokio::runtime::Builder::new_multi_thread().build().unwrap(),
        false => tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap(),
    };

    criterion.bench_function("full_committer_flow", |benchmark| {
        benchmark.iter(|| {
            runtime.block_on(commit(FLOW_TEST_INPUT));
        })
    });
}

criterion_group!(
    benches,
    single_tree_flow_benchmark,
    full_committer_flow_benchmark
);
criterion_main!(benches);
