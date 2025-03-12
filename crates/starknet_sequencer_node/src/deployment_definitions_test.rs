use starknet_infra_utils::dumping::serialize_to_file_test;

use crate::deployment_definitions::{
    MAIN_DEPLOYMENT,
    MAIN_DEPLOYMENT_PRESET_PATH,
    TESTING_DEPLOYMENT,
    TESTING_DEPLOYMENT_PRESET_PATH,
};

// TODO(Tsabary): bundle deployment and its preset path together, and create a list of all of these
// pairs. Then in the test, iterate over them and test each one.

/// Test that the deployment file is up to date. To update it run:
/// cargo run --bin deployment_generator -q
#[test]
fn deployment_files_are_up_to_date() {
    serialize_to_file_test(MAIN_DEPLOYMENT, MAIN_DEPLOYMENT_PRESET_PATH);
    serialize_to_file_test(TESTING_DEPLOYMENT, TESTING_DEPLOYMENT_PRESET_PATH);
}
