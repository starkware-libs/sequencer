use std::env;

use starknet_infra_utils::path::resolve_project_relative_path;
use starknet_sequencer_node::config::config_utils::RequiredParams;
use starknet_sequencer_node::config::node_config::DEFAULT_PRESET_CONFIG_PATH;

/// Updates the default preset config file by:
/// cargo run --bin sequencer_dump_preset_config -q
fn main() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    let preset_file_path = RequiredParams::create_for_testing()
        .dump_to_file(DEFAULT_PRESET_CONFIG_PATH, env::current_dir().unwrap());
    assert!(
        preset_file_path.exists(),
        "Can't create default preset config file: {:?}",
        preset_file_path
    );
}
