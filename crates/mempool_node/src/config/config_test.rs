use std::env;
use std::fs::File;

use assert_json_diff::assert_json_eq;
use assert_matches::assert_matches;
use colored::Colorize;
use mempool_test_utils::get_absolute_path;
use papyrus_config::dumping::SerializeConfig;
use rstest::rstest;
use starknet_mempool_infra::component_definitions::{
    LocalComponentCommunicationConfig,
    RemoteClientConfig,
    RemoteServerConfig,
};
use validator::Validate;

use crate::config::{
    ComponentExecutionConfig,
    ComponentExecutionMode,
    SequencerNodeConfig,
    CONFIG_POINTERS,
    DEFAULT_CONFIG_PATH,
};

/// Test the validation of the struct ComponentExecutionConfig.
/// Validates that execution mode of the component and the local/remote config are at sync.
#[rstest]
#[case::local(ComponentExecutionMode::Local, None, None)]
#[case::remote(ComponentExecutionMode::Remote, Some(RemoteClientConfig::default()), None)]
#[case::remote(ComponentExecutionMode::Remote, None, Some(RemoteServerConfig::default()))]
fn test_valid_component_execution_config(
    #[case] execution_mode: ComponentExecutionMode,
    #[case] remote_client_config: Option<RemoteClientConfig>,
    #[case] remote_server_config: Option<RemoteServerConfig>,
) {
    // Initialize a valid config and check that the validator returns Ok.

    let local_config = if execution_mode == ComponentExecutionMode::Local {
        Some(LocalComponentCommunicationConfig::default())
    } else {
        None
    };

    let component_exe_config = ComponentExecutionConfig {
        execution_mode,
        local_config,
        remote_client_config,
        remote_server_config,
        ..ComponentExecutionConfig::default()
    };
    assert_eq!(component_exe_config.validate(), Ok(()));
}

/// Test the validation of the struct SequencerNodeConfig and that the default config file is up to
/// date. To update the default config file, run:
/// cargo run --bin sequencer_dump_config -q
#[test]
fn default_config_file_is_up_to_date() {
    env::set_current_dir(get_absolute_path("")).expect("Couldn't set working dir.");
    let from_default_config_file: serde_json::Value =
        serde_json::from_reader(File::open(DEFAULT_CONFIG_PATH).unwrap()).unwrap();

    let default_config = SequencerNodeConfig::default();
    assert_matches!(default_config.validate(), Ok(()));

    // Create a temporary file and dump the default config to it.
    let mut tmp_file_path = env::temp_dir();
    tmp_file_path.push("cfg.json");
    default_config.dump_to_file(&CONFIG_POINTERS, tmp_file_path.to_str().unwrap()).unwrap();

    // Read the dumped config from the file.
    let from_code: serde_json::Value =
        serde_json::from_reader(File::open(tmp_file_path).unwrap()).unwrap();

    println!(
        "{}",
        "Default config file doesn't match the default NodeConfig implementation. Please update \
         it using the sequencer_dump_config binary."
            .purple()
            .bold()
    );
    println!(
        "Diffs shown below (default config file <<>> dump of SequencerNodeConfig::default())."
    );
    assert_json_eq!(from_default_config_file, from_code)
}
