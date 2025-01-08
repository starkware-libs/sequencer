use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use assert_matches::assert_matches;
use colored::Colorize;
use infra_utils::path::resolve_project_relative_path;
use papyrus_config::dumping::SerializeConfig;
use papyrus_config::validators::config_validate;
use papyrus_config::SerializedParam;
use rstest::rstest;
use starknet_api::test_utils::json_utils::assert_json_eq;
use starknet_batcher::block_builder::BlockBuilderConfig;
use starknet_batcher::config::BatcherConfig;
use starknet_sequencer_infra::component_definitions::{LocalServerConfig, RemoteClientConfig};
use validator::Validate;

use crate::config::component_execution_config::{
    ReactiveComponentExecutionConfig,
    ReactiveComponentExecutionMode,
};
use crate::config::config_utils::{create_test_config_load_args, RequiredParams};
use crate::config::node_config::{
    SequencerNodeConfig,
    CONFIG_NON_POINTERS_WHITELIST,
    CONFIG_POINTERS,
    DEFAULT_CONFIG_PATH,
    DEFAULT_PRESET_CONFIG_PATH,
};

const LOCAL_EXECUTION_MODE: ReactiveComponentExecutionMode =
    ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled;
const ENABLE_REMOTE_CONNECTION_MODE: ReactiveComponentExecutionMode =
    ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled;

const VALID_SOCKET: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080);

/// Test the validation of the struct ReactiveComponentExecutionConfig.
/// Validates that execution mode of the component and the local/remote config are at sync.
#[rstest]
#[case::local(ReactiveComponentExecutionMode::Disabled, None, None, VALID_SOCKET)]
#[case::local(
    ReactiveComponentExecutionMode::Remote,
    None,
    Some(RemoteClientConfig::default()),
    VALID_SOCKET
)]
#[case::local(LOCAL_EXECUTION_MODE, Some(LocalServerConfig::default()), None, VALID_SOCKET)]
#[case::remote(
    ENABLE_REMOTE_CONNECTION_MODE,
    Some(LocalServerConfig::default()),
    None,
    VALID_SOCKET
)]
fn test_valid_component_execution_config(
    #[case] execution_mode: ReactiveComponentExecutionMode,
    #[case] local_server_config: Option<LocalServerConfig>,
    #[case] remote_client_config: Option<RemoteClientConfig>,
    #[case] socket: SocketAddr,
) {
    let component_exe_config = ReactiveComponentExecutionConfig {
        execution_mode,
        local_server_config,
        remote_client_config,
        socket,
    };
    assert_eq!(component_exe_config.validate(), Ok(()));
}

// TODO(Arni): share code with
// `papyrus_node::config::config_test::default_config_file_is_up_to_date`.
/// Test the validation of the struct SequencerNodeConfig and that the default config file is up to
/// date. To update the default config file, run:
/// cargo run --bin sequencer_dump_config -q
#[test]
fn test_default_config_file_is_up_to_date() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");
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

    let error_message = format!(
        "{}\n{}",
        "Default config file doesn't match the default SequencerNodeConfig implementation. Please \
         update it using the sequencer_dump_config binary."
            .purple()
            .bold(),
        "Diffs shown below (default config file <<>> dump of SequencerNodeConfig::default())."
    );
    assert_json_eq(&from_default_config_file, &from_code, error_message);
}

/// Test that the default preset config file is up to date.
#[test]
fn test_default_preset_file_is_up_to_date() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");
    let from_default_preset_file: serde_json::Value =
        serde_json::from_reader(File::open(DEFAULT_PRESET_CONFIG_PATH).unwrap()).unwrap();

    let current_preset_config: RequiredParams = RequiredParams::create_for_testing();
    let error_message = format!(
        "{}\n{}",
        "Default preset config file doesn't match the default RequiredParams. Please update it \
         using the sequencer_dump_preset_config binary."
            .purple()
            .bold(),
        "Diffs shown below (default preset config file <<>> dump of \
         RequiredParams::create_for_testing())."
    );
    assert_json_eq(&from_default_preset_file, &current_preset_config.as_json(), error_message);
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

/// Tests compatibility of the required parameter settings: required params (containing required
/// pointer targets) and test util struct.
#[test]
fn test_required_params_setting() {
    // Load the default config file.
    let file =
        std::fs::File::open(resolve_project_relative_path(DEFAULT_CONFIG_PATH).unwrap()).unwrap();
    let mut deserialized = serde_json::from_reader::<_, serde_json::Value>(file).unwrap();
    let expected_required_params = deserialized.as_object_mut().unwrap();
    expected_required_params.retain(|_, value| {
        let param = serde_json::from_value::<SerializedParam>(value.clone()).unwrap();
        param.is_required()
    });
    let expected_required_keys =
        expected_required_params.keys().cloned().collect::<HashSet<String>>();

    let required_params: HashSet<String> =
        RequiredParams::create_for_testing().field_names().into_iter().collect();
    assert_eq!(required_params, expected_required_keys);
}

#[test]
fn test_validate_config_success() {
    let config = SequencerNodeConfig::default();
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_batcher_config_failure() {
    let config = SequencerNodeConfig {
        batcher_config: BatcherConfig {
            input_stream_content_buffer_size: 99,
            block_builder_config: BlockBuilderConfig { tx_chunk_size: 100, ..Default::default() },
            ..Default::default()
        },
        ..Default::default()
    };

    let error = config.validate().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("input_stream_content_buffer_size must be at least tx_chunk_size")
    );
}
