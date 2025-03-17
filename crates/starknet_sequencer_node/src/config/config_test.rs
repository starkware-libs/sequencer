use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::net::{IpAddr, Ipv4Addr};

use colored::Colorize;
use papyrus_config::dumping::SerializeConfig;
use papyrus_config::SerializedParam;
use rstest::rstest;
use starknet_batcher::block_builder::BlockBuilderConfig;
use starknet_batcher::config::BatcherConfig;
use starknet_infra_utils::path::resolve_project_relative_path;
use starknet_infra_utils::test_utils::assert_json_eq;
use starknet_sequencer_infra::component_definitions::{LocalServerConfig, RemoteClientConfig};
use validator::Validate;

use crate::config::component_execution_config::{
    ReactiveComponentExecutionConfig,
    ReactiveComponentExecutionMode,
};
use crate::config::config_utils::RequiredParams;
use crate::config::node_config::{
    SequencerNodeConfig,
    CONFIG_NON_POINTERS_WHITELIST,
    CONFIG_POINTERS,
    DEFAULT_CONFIG_PATH,
};

const LOCAL_EXECUTION_MODE: ReactiveComponentExecutionMode =
    ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled;
const ENABLE_REMOTE_CONNECTION_MODE: ReactiveComponentExecutionMode =
    ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled;

const VALID_URL: &str = "www.google.com";
const VALID_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
const VALID_PORT: u16 = 8080;

/// Test the validation of the struct ReactiveComponentExecutionConfig.
/// Validates that execution mode of the component and the local/remote config are at sync.
#[rstest]
#[case::local(
    ReactiveComponentExecutionMode::Disabled,
    LocalServerConfig::default(),
    RemoteClientConfig::default(),
    VALID_URL,
    VALID_IP,
    VALID_PORT
)]
#[case::local(
    ReactiveComponentExecutionMode::Remote,
    LocalServerConfig::default(),
    RemoteClientConfig::default(),
    VALID_URL,
    VALID_IP,
    VALID_PORT
)]
#[case::local(
    LOCAL_EXECUTION_MODE,
    LocalServerConfig::default(),
    RemoteClientConfig::default(),
    VALID_URL,
    VALID_IP,
    VALID_PORT
)]
#[case::remote(
    ENABLE_REMOTE_CONNECTION_MODE,
    LocalServerConfig::default(),
    RemoteClientConfig::default(),
    VALID_URL,
    VALID_IP,
    VALID_PORT
)]
fn valid_component_execution_config(
    #[case] execution_mode: ReactiveComponentExecutionMode,
    #[case] local_server_config: LocalServerConfig,
    #[case] remote_client_config: RemoteClientConfig,
    #[case] url: &str,
    #[case] ip: IpAddr,
    #[case] port: u16,
) {
    let component_exe_config = ReactiveComponentExecutionConfig {
        execution_mode,
        local_server_config,
        remote_client_config,
        max_concurrency: 1,
        url: url.to_string(),
        ip,
        port,
    };
    assert_eq!(component_exe_config.validate(), Ok(()));
}

/// Test the validation of the struct SequencerNodeConfig and that the default config file is up to
/// date. To update the default config file, run:
/// cargo run --bin sequencer_dump_config -q
#[test]
fn default_config_file_is_up_to_date() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");
    let from_default_config_file: serde_json::Value =
        serde_json::from_reader(File::open(DEFAULT_CONFIG_PATH).unwrap()).unwrap();

    // Create a temporary file and dump the default config to it.
    let mut tmp_file_path = env::temp_dir();
    tmp_file_path.push("cfg.json");
    SequencerNodeConfig::default()
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

/// Tests compatibility of the required parameter settings: required params (containing required
/// pointer targets) and test util struct.
#[test]
fn required_params_setting() {
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
fn validate_config_success() {
    let config = SequencerNodeConfig::default();
    assert!(config.validate().is_ok());
}
