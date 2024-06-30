use std::collections::HashMap;

use committer::patricia_merkle_tree::external_test_utils::single_tree_flow_test;
use pretty_assertions::assert_eq;
use serde_json::Value;

use crate::{
    commands::commit, tests::utils::parse_from_python::parse_input_single_storage_tree_flow_test,
};

//TODO(Aner, 20/06/2024): this test needs to be fixed to be run correctly in the CI:
//1. Fix the test to measure cpu_time and not wall_time.
//2. Fix the max time threshold to be the expected time for the benchmark test.
const MAX_TIME_FOR_SINGLE_TREE_BECHMARK_TEST: f64 = 5.0;
const MAX_TIME_FOR_COMMITTER_FLOW_BECHMARK_TEST: f64 = 5.0;
const SINGLE_TREE_FLOW_INPUT: &str = include_str!("../../benches/tree_flow_inputs.json");
//TODO(Aner, 20/06/2024): modify the committer_flow_inputs.json file to be from pseudo-real data
// and to include the expected output.
const FLOW_TEST_INPUT: &str = include_str!("../../benches/committer_flow_inputs.json");
const OUTPUT_PATH: &str = "benchmark_output.txt";

#[ignore = "To avoid running the benchmark test in Coverage or without the --release flag."]
#[tokio::test(flavor = "multi_thread")]
pub async fn test_benchmark_single_tree() {
    let input: HashMap<String, String> = serde_json::from_str(SINGLE_TREE_FLOW_INPUT).unwrap();
    let (leaf_modifications, storage, root_hash) =
        parse_input_single_storage_tree_flow_test(&input);
    let expected_hash = input.get("expected_hash").unwrap();

    let start = std::time::Instant::now();
    // Benchmark the single tree flow test.
    let output = single_tree_flow_test(leaf_modifications, storage, root_hash).await;
    let execution_time = std::time::Instant::now() - start;

    let output_map: HashMap<&str, Value> = serde_json::from_str(&output).unwrap();
    let output_hash = output_map.get("root_hash").unwrap();
    assert_eq!(output_hash.as_str().unwrap(), expected_hash);

    // 4. Assert the execution time does not exceed the threshold.
    assert!(execution_time.as_secs_f64() < MAX_TIME_FOR_SINGLE_TREE_BECHMARK_TEST);
}

#[ignore = "To avoid running the benchmark test in Coverage or without the --release flag."]
#[tokio::test(flavor = "multi_thread")]
pub async fn test_benchmark_committer_flow() {
    let start = std::time::Instant::now();
    // Benchmark the committer flow test.
    commit(FLOW_TEST_INPUT, OUTPUT_PATH.to_owned()).await;
    let execution_time = std::time::Instant::now() - start;

    // TODO(Aner, 20/06/2024): add assert for the output of the committer flow test.

    // Assert the execution time does not exceed the threshold.
    assert!(execution_time.as_secs_f64() < MAX_TIME_FOR_COMMITTER_FLOW_BECHMARK_TEST);
}
