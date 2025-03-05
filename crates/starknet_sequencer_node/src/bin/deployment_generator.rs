use starknet_infra_utils::dumping::serialize_to_file;
use starknet_sequencer_node::deployment_definitions::{
    MAIN_DEPLOYMENT,
    MAIN_DEPLOYMENT_PRESET_PATH,
};

/// Creates the deployment json file.
fn main() {
    serialize_to_file(MAIN_DEPLOYMENT, MAIN_DEPLOYMENT_PRESET_PATH);
}
