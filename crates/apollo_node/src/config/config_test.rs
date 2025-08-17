use std::net::{IpAddr, Ipv4Addr};

use apollo_config::dumping::{combine_config_map_and_pointers, SerializeConfig};
use apollo_infra::component_client::RemoteClientConfig;
use apollo_infra::component_server::LocalServerConfig;
use apollo_infra_utils::dumping::serialize_to_file_test;
use rstest::rstest;
use validator::Validate;

use crate::config::component_execution_config::{
    ReactiveComponentExecutionConfig,
    ReactiveComponentExecutionMode,
};
use crate::config::config_utils::private_parameters;
use crate::config::monitoring::MonitoringConfig;
use crate::config::node_config::{
    SequencerNodeConfig,
    CONFIG_NON_POINTERS_WHITELIST,
    CONFIG_POINTERS,
    CONFIG_SCHEMA_PATH,
    CONFIG_SECRETS_SCHEMA_PATH,
};

const FIX_BINARY_NAME: &str = "update_apollo_node_config_schema";

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
        local_server_config: Some(local_server_config),
        remote_client_config: Some(remote_client_config),
        max_concurrency: 1,
        url: url.to_string(),
        ip,
        port,
    };
    assert_eq!(component_exe_config.validate(), Ok(()));
}

/// Test the validation of the struct SequencerNodeConfig and that the default config file is up to
/// date. To update the default config file, run `cargo run --bin <FIX_BINARY_NAME>`.
#[test]
fn default_config_file_is_up_to_date() {
    let combined_map = combine_config_map_and_pointers(
        SequencerNodeConfig::default().dump(),
        &CONFIG_POINTERS,
        &CONFIG_NON_POINTERS_WHITELIST,
    )
    .unwrap();
    serialize_to_file_test(&combined_map, CONFIG_SCHEMA_PATH, FIX_BINARY_NAME);

    serialize_to_file_test(private_parameters(), CONFIG_SECRETS_SCHEMA_PATH, FIX_BINARY_NAME);
}

#[test]
fn validate_config_success() {
    let config = SequencerNodeConfig::default();
    assert!(config.validate().is_ok());
}

#[rstest]
#[case::monitoring_and_profiling(true, true, true)]
#[case::monitoring_without_profiling(true, false, true)]
#[case::no_monitoring_nor_profiling(false, false, true)]
#[case::no_monitoring_with_profiling(false, true, false)]
fn monitoring_config(
    #[case] collect_metrics: bool,
    #[case] collect_profiling_metrics: bool,
    #[case] expected_successful_validation: bool,
) {
    let component_exe_config = MonitoringConfig { collect_metrics, collect_profiling_metrics };
    assert_eq!(component_exe_config.validate().is_ok(), expected_successful_validation);
}
