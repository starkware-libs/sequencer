use std::collections::HashMap;

use committer::patricia_merkle_tree::external_test_utils::single_tree_flow_test;
use pretty_assertions::assert_eq;
use serde_json::Value;

use crate::tests::utils::parse_from_python::parse_input_single_storage_tree_flow_test;

//TODO(Aner, 20/06/2024): this test needs to be fixed to be run correctly in the CI:
//1. Fix the test to measure cpu_time and not wall_time.
//2. Fix the max time threshold to be the expected time for the benchmark test.
const MAX_TIME_FOR_BECHMARK_TEST: f64 = 5.0;
const INPUT: &str = include_str!("../../benches/tree_flow_inputs.json");

#[ignore = "To avoid running the benchmark test in Coverage or without the --release flag."]
#[tokio::test(flavor = "multi_thread")]
pub async fn test_benchmark() {
    let input: HashMap<String, String> = serde_json::from_str(INPUT).unwrap();
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
    assert!(execution_time.as_secs_f64() < MAX_TIME_FOR_BECHMARK_TEST);
}
