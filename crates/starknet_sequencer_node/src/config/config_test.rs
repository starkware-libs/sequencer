use std::env;
use std::fs::File;

use assert_json_diff::assert_json_eq;
use assert_matches::assert_matches;
use colored::Colorize;
use papyrus_config::dumping::SerializeConfig;
use papyrus_config::validators::config_validate;
use rstest::rstest;
use starknet_api::test_utils::get_absolute_path;
use starknet_sequencer_infra::component_definitions::{
    LocalServerConfig,
    RemoteClientConfig,
    RemoteServerConfig,
};
use validator::Validate;

use crate::config::component_execution_config::{ComponentExecutionConfig, ComponentExecutionMode};
use crate::config::node_config::{
    SequencerNodeConfig,
    CONFIG_NON_POINTERS_WHITELIST,
    CONFIG_POINTERS,
    DEFAULT_CONFIG_PATH,
    REQUIRED_PARAM_CONFIG_POINTERS,
};
use crate::config::test_utils::{create_test_config_load_args, RequiredParams};

const LOCAL_EXECUTION_MODE: ComponentExecutionMode =
    ComponentExecutionMode::LocalExecutionWithRemoteDisabled;
const ENABLE_REMOTE_CONNECTION_MODE: ComponentExecutionMode =
    ComponentExecutionMode::LocalExecutionWithRemoteEnabled;

/// Test the validation of the struct ComponentExecutionConfig.
/// Validates that execution mode of the component and the local/remote config are at sync.
#[rstest]
#[case::local(ComponentExecutionMode::Disabled, None, None, None)]
#[case::local(ComponentExecutionMode::Remote, None, Some(RemoteClientConfig::default()), None)]
#[case::local(LOCAL_EXECUTION_MODE, Some(LocalServerConfig::default()), None, None)]
#[case::remote(
    ENABLE_REMOTE_CONNECTION_MODE,
    Some(LocalServerConfig::default()),
    None,
    Some(RemoteServerConfig::default())
)]
fn test_valid_component_execution_config(
    #[case] execution_mode: ComponentExecutionMode,
    #[case] local_server_config: Option<LocalServerConfig>,
    #[case] remote_client_config: Option<RemoteClientConfig>,
    #[case] remote_server_config: Option<RemoteServerConfig>,
) {
    let component_exe_config = ComponentExecutionConfig {
        execution_mode,
        local_server_config,
        remote_client_config,
        remote_server_config,
    };
    assert_eq!(component_exe_config.validate(), Ok(()));
}

/// Test the validation of the struct SequencerNodeConfig and that the default config file is up to
/// date. To update the default config file, run:
/// cargo run --bin sequencer_dump_config -q
#[test]
fn test_default_config_file_is_up_to_date() {
    env::set_current_dir(get_absolute_path("")).expect("Couldn't set working dir.");
    let from_default_config_file: serde_json::Value =
        serde_json::from_reader(File::open(DEFAULT_CONFIG_PATH).unwrap()).unwrap();

    let default_config = SequencerNodeConfig::default();
    assert_matches!(default_config.validate(), Ok(()));

    // Create a temporary file and dump the default config to it.
    let mut tmp_file_path = env::temp_dir();
    tmp_file_path.push("cfg.json");
    default_config
        .dump_to_file(
            &CONFIG_POINTERS,
            &CONFIG_NON_POINTERS_WHITELIST,
            tmp_file_path.to_str().unwrap(),
        )
        .unwrap();

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

/// Tests parsing a node config without additional args.
#[test]
fn test_config_parsing() {
    let required_params = RequiredParams::create_for_testing();
    let args = create_test_config_load_args(required_params);
    let config = SequencerNodeConfig::load_and_process(args);
    let config = config.expect("Parsing function failed.");

    let result = config_validate(&config);
    assert_matches!(result, Ok(_), "Expected Ok but got {:?}", result);
}

/// Tests compatibility of the required parameter settings: pointer targets and test util struct.
#[test]
fn test_required_params_setting() {
    let required_pointers =
        REQUIRED_PARAM_CONFIG_POINTERS.iter().map(|((x, _), _)| x.to_owned()).collect::<Vec<_>>();
    let required_params = RequiredParams::field_names();
    assert_eq!(required_pointers, required_params);
}
