use starknet_infra_utils::dumping::serialize_to_file;
use starknet_sequencer_node::deployment_definitions::{
    MAIN_DEPLOYMENT,
    MAIN_DEPLOYMENT_PRESET_PATH,
    TESTING_DEPLOYMENT,
    TESTING_DEPLOYMENT_PRESET_PATH,
};

// TODO(Tsabary): bundle deployment and its preset path together, and create a list of all of these
// pairs. Then in the test, iterate over them and test each one.

/// Creates the deployment json file.
fn main() {
    serialize_to_file(MAIN_DEPLOYMENT, MAIN_DEPLOYMENT_PRESET_PATH);
    serialize_to_file(TESTING_DEPLOYMENT, TESTING_DEPLOYMENT_PRESET_PATH);
}
