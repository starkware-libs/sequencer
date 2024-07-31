use std::collections::HashMap;
use std::fs;

use clap::Error;
use committer::block_committer::input::{ConfigImpl, Input, StarknetStorageValue};
use committer::patricia_merkle_tree::external_test_utils::single_tree_flow_test;
use committer::patricia_merkle_tree::original_skeleton_tree::config::OriginalSkeletonStorageTrieConfig;
use committer::patricia_merkle_tree::updated_skeleton_tree::hash_function::TreeHashFunctionImpl;
use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};
use tempfile::NamedTempFile;

use super::utils::parse_from_python::parse_input_single_storage_tree_flow_test;
use crate::commands::commit;
use crate::parse_input::read::parse_input;
use crate::tests::utils::parse_from_python::TreeFlowInput;

// TODO(Aner, 20/06/2024): these tests needs to be fixed to be run correctly in the CI:
// 1. Fix the test to measure cpu_time and not wall_time.
// 2. Fix the max time threshold to be the expected time for the benchmark test.
const MAX_TIME_FOR_SINGLE_TREE_BECHMARK_TEST: f64 = 5.0;
const MAX_TIME_FOR_COMMITTER_FLOW_BECHMARK_TEST: f64 = 5.0;
const SINGLE_TREE_FLOW_INPUT: &str = include_str!("../../test_inputs/tree_flow_inputs.json");
const FLOW_TEST_INPUT: &str = include_str!("../../test_inputs/committer_flow_inputs.json");
const OUTPUT_PATH: &str = "benchmark_output.txt";
const EXPECTED_NUMBER_OF_FILES: usize = 100;

#[derive(derive_more::Deref)]
struct FactMap(Map<String, Value>);

impl<'de> Deserialize<'de> for FactMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self(serde_json::from_str(&String::deserialize(deserializer)?).unwrap()))
    }
}

struct CommitterInput(Input<ConfigImpl>);

impl<'de> Deserialize<'de> for CommitterInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self(parse_input(&String::deserialize(deserializer)?).unwrap()))
    }
}

#[derive(Deserialize)]
struct CommitterRegressionInput {
    committer_input: CommitterInput,
    contract_states_root: String,
    contract_classes_root: String,
    expected_facts: FactMap,
}

#[derive(Deserialize)]
struct TreeRegressionOutput {
    root_hash: Value,
    storage_changes: Value,
}

#[derive(Deserialize)]
struct StorageObject {
    storage: Value,
}

#[derive(Deserialize)]
struct CommitterRegressionOutput {
    contract_storage_root_hash: Value,
    compiled_class_root_hash: Value,
    storage: StorageObject,
}

struct TreeRegressionInput {
    tree_flow_input: TreeFlowInput,
    expected_hash: String,
    expected_storage_changes: Map<String, Value>,
}

// TODO(Aner, 9/8/24): remove this impl and use the Deserialize derive, by changing the input
// format.
impl<'de> Deserialize<'de> for TreeRegressionInput {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let map = HashMap::deserialize(deserializer)?;
        Ok(Self {
            tree_flow_input: parse_input_single_storage_tree_flow_test(&map),
            expected_hash: map.get("expected_hash").unwrap().to_string(),
            expected_storage_changes: serde_json::from_str(
                map.get("expected_storage_changes").unwrap(),
            )
            .unwrap(),
        })
    }
}

#[ignore = "To avoid running the regression test in Coverage or without the --release flag."]
#[tokio::test(flavor = "multi_thread")]
pub async fn test_regression_single_tree() {
    let TreeRegressionInput {
        tree_flow_input: TreeFlowInput { leaf_modifications, storage, root_hash },
        expected_hash,
        expected_storage_changes,
    } = serde_json::from_str(SINGLE_TREE_FLOW_INPUT).unwrap();

    let start = std::time::Instant::now();
    // Benchmark the single tree flow test.
    let output = single_tree_flow_test::<StarknetStorageValue, TreeHashFunctionImpl>(
        leaf_modifications.clone(),
        storage,
        root_hash,
        OriginalSkeletonStorageTrieConfig::new(&leaf_modifications, false),
    )
    .await;
    let execution_time = std::time::Instant::now() - start;

    // Assert correctness of the output of the single tree flow test.
    let TreeRegressionOutput { root_hash, storage_changes: Value::Object(actual_storage_changes) } =
        serde_json::from_str(&output).unwrap()
    else {
        panic!("Expected storage changes object to be an object.");
    };
    assert_eq!(root_hash, expected_hash);

    assert_eq!(actual_storage_changes, expected_storage_changes);

    // 4. Assert the execution time does not exceed the threshold.
    assert!(execution_time.as_secs_f64() < MAX_TIME_FOR_SINGLE_TREE_BECHMARK_TEST);
}

pub async fn test_single_committer_flow(input: String, output_path: String) -> Result<(), Error> {
    let CommitterRegressionInput {
        committer_input,
        contract_states_root: expected_contract_states_root,
        contract_classes_root: expected_contract_classes_root,
        expected_facts,
    } = serde_json::from_str(&input).unwrap();
    // Benchmark the committer flow test.
    commit(committer_input.0, output_path.to_owned()).await;

    // Assert correctness of the output of the committer flow test.
    let CommitterRegressionOutput {
        contract_storage_root_hash,
        compiled_class_root_hash,
        storage: StorageObject { storage: Value::Object(storage_changes) },
    } = serde_json::from_str(&std::fs::read_to_string(output_path).unwrap()).unwrap()
    else {
        panic!("Expected the storage to be an object.");
    };

    assert_eq!(contract_storage_root_hash, expected_contract_states_root);
    assert_eq!(compiled_class_root_hash, expected_contract_classes_root);
    assert_eq!(storage_changes, *expected_facts);

    // Assert the execution time does not exceed the threshold.
    // TODO(Aner, 20/06/2024): Add cpu_time time measurement and verify the time is below the
    // threshold.
    Ok(())
}
#[ignore = "To avoid running the regression test in Coverage or without the --release flag."]
#[tokio::test(flavor = "multi_thread")]
pub async fn test_regression_committer_flow() {
    let start = std::time::Instant::now();
    let result =
        test_single_committer_flow(FLOW_TEST_INPUT.to_string(), OUTPUT_PATH.to_string()).await;
    if result.is_err() {
        panic!("Error {}", result.err().unwrap());
    }
    let execution_time = std::time::Instant::now() - start;
    assert!(execution_time.as_secs_f64() < MAX_TIME_FOR_COMMITTER_FLOW_BECHMARK_TEST);
}

#[ignore = "To avoid running the regression test in Coverage or without the --release flag."]
#[tokio::test(flavor = "multi_thread")]
pub async fn test_regression_committer_all_files() {
    assert_eq!(
        fs::read_dir("./test_inputs/regression_files").unwrap().count(),
        EXPECTED_NUMBER_OF_FILES
    );
    let dir_path = fs::read_dir("./test_inputs/regression_files").unwrap();
    let mut tasks = Vec::with_capacity(EXPECTED_NUMBER_OF_FILES);
    for entry in dir_path {
        tasks.push(tokio::task::spawn(async move {
            let file_path = entry.unwrap().path();
            let output_file = NamedTempFile::new().unwrap();
            let result = test_single_committer_flow(
                fs::read_to_string(file_path.clone()).unwrap(),
                output_file.path().to_str().unwrap().to_string(),
            )
            .await;
            if result.is_err() {
                panic!("Error {} for file: {:?}", result.err().unwrap(), file_path);
            }
        }));
    }
    futures::future::try_join_all(tasks).await.unwrap();
}
