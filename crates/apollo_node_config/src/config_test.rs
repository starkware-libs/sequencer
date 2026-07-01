use std::collections::BTreeSet;

use apollo_config::dumping::SerializeConfig;
use apollo_config::ParamPath;
use apollo_infra::component_client::RemoteClientConfig;
use apollo_infra::component_server::{LocalServerConfig, RemoteServerConfig};
use apollo_infra_utils::dumping::serialize_to_file_test;
use apollo_state_sync_config::config::{StateSyncConfig, StateSyncStaticConfig};
use apollo_storage::{StorageConfig, StorageScope};
use rstest::rstest;
use validator::Validate;

use crate::component_config::ComponentConfig;
use crate::component_execution_config::{
    ActiveComponentExecutionConfig,
    ReactiveComponentExecutionConfig,
    ReactiveComponentExecutionMode,
};
use crate::config_utils::private_parameters;
use crate::monitoring::MonitoringConfig;
use crate::node_config::{SequencerNodeConfig, CONFIG_POINTERS, CONFIG_SECRETS_SCHEMA_PATH};

const FIX_BINARY_NAME: &str = "update_apollo_node_config_schema";

const LOCAL_EXECUTION_MODE: ReactiveComponentExecutionMode =
    ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled;
const ENABLE_REMOTE_CONNECTION_MODE: ReactiveComponentExecutionMode =
    ReactiveComponentExecutionMode::LocalExecutionWithRemoteEnabled;

const VALID_URL: &str = "www.google.com";
const VALID_PORT: u16 = 8080;

/// Test the validation of the struct ReactiveComponentExecutionConfig.
/// Validates that execution mode of the component and the local/remote config are at sync.
#[rstest]
#[case::local(ReactiveComponentExecutionMode::Disabled, None, None, None, VALID_URL, VALID_PORT)]
#[case::local(
    ReactiveComponentExecutionMode::Remote,
    None,
    None,
    Some(RemoteClientConfig::default()),
    VALID_URL,
    VALID_PORT
)]
#[case::local(
    LOCAL_EXECUTION_MODE,
    Some(LocalServerConfig::default()),
    None,
    None,
    VALID_URL,
    VALID_PORT
)]
#[case::remote(
    ENABLE_REMOTE_CONNECTION_MODE,
    Some(LocalServerConfig::default()),
    Some(RemoteServerConfig::default()),
    None,
    VALID_URL,
    VALID_PORT
)]
fn valid_component_execution_config(
    #[case] execution_mode: ReactiveComponentExecutionMode,
    #[case] local_server_config: Option<LocalServerConfig>,
    #[case] remote_server_config: Option<RemoteServerConfig>,
    #[case] remote_client_config: Option<RemoteClientConfig>,
    #[case] url: &str,
    #[case] port: u16,
) {
    let component_exe_config = ReactiveComponentExecutionConfig {
        execution_mode,
        local_server_config,
        remote_server_config,
        remote_client_config,
        url: url.to_string(),
        port,
    };
    assert_eq!(component_exe_config.validate(), Ok(()));
}

/// Computes the private-parameter set the way it was historically derived: from the config dump
/// (`SequencerNodeConfig::default().dump()`), filtering private params and remapping pointer
/// members to their pointer target. This is the source of truth the committed secrets schema was
/// generated from.
///
/// TRANSIENT: this mirrors the pre-Phase-4 body of `private_parameters()`. It exists only to guard
/// that re-sourcing `private_parameters()` off the committed secrets schema did not drift from the
/// config-derived set. Remove it together with the transient equivalence test.
fn private_parameters_from_config_dump() -> BTreeSet<ParamPath> {
    let dumped_config = SequencerNodeConfig::default().dump();

    let mut private_values = BTreeSet::new();
    for (param_path, ser_param) in dumped_config.into_iter() {
        if !ser_param.is_private() {
            continue;
        }
        let mut included_as_a_pointer = false;
        for ((pointer_target_param_path, _ser_param), pointing_params) in CONFIG_POINTERS.iter() {
            if pointing_params.contains(&param_path) {
                private_values.insert(pointer_target_param_path.clone());
                included_as_a_pointer = true;
            }
        }
        if !included_as_a_pointer {
            private_values.insert(param_path);
        }
    }
    private_values
}

/// Test that the committed secrets schema file is up to date. To update it, run
/// `cargo run --bin <FIX_BINARY_NAME>`.
#[test]
fn default_config_file_is_up_to_date() {
    serialize_to_file_test(&private_parameters(), CONFIG_SECRETS_SCHEMA_PATH, FIX_BINARY_NAME);
}

/// TRANSIENT: proves that the file-sourced `private_parameters()` returns the same set as the
/// historical config-dump-derived computation, guarding the Phase-4 re-source against drift. Remove
/// this together with `private_parameters_from_config_dump` once the config-derivation path is
/// gone.
#[test]
fn private_parameters_matches_config_dump_derivation() {
    let file_sourced: BTreeSet<ParamPath> = private_parameters();
    let config_derived = private_parameters_from_config_dump();
    assert_eq!(
        file_sourced, config_derived,
        "File-sourced private_parameters() drifted from the config-dump derivation. Regenerate \
         the secrets schema with `cargo run --bin {FIX_BINARY_NAME}`."
    );
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

fn state_sync_config_with_full_archive() -> StateSyncConfig {
    StateSyncConfig {
        static_config: StateSyncStaticConfig {
            storage_config: StorageConfig {
                scope: StorageScope::FullArchive,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn validation_only_with_gateway_enabled_fails() {
    let config = SequencerNodeConfig {
        validation_only: true,
        components: ComponentConfig {
            gateway: ReactiveComponentExecutionConfig::default(),
            http_server: ActiveComponentExecutionConfig::default(),
            mempool: ReactiveComponentExecutionConfig::default(),
            mempool_p2p: ReactiveComponentExecutionConfig::default(),
            ..Default::default()
        },
        state_sync_config: Some(state_sync_config_with_full_archive()),
        ..Default::default()
    };
    let err = config.validate_node_config().unwrap_err();
    assert!(format!("{err:?}").contains("gateway"), "Unexpected error: {err:?}");
}

#[test]
fn validation_only_with_http_server_enabled_fails() {
    // Disable gateway to reach the http_server check.
    let config = SequencerNodeConfig {
        validation_only: true,
        components: ComponentConfig {
            gateway: ReactiveComponentExecutionConfig::disabled(),
            http_server: ActiveComponentExecutionConfig::default(),
            mempool: ReactiveComponentExecutionConfig::default(),
            mempool_p2p: ReactiveComponentExecutionConfig::default(),
            ..Default::default()
        },
        gateway_config: None,
        state_sync_config: Some(state_sync_config_with_full_archive()),
        ..Default::default()
    };
    let err = config.validate_node_config().unwrap_err();
    assert!(format!("{err:?}").contains("http_server"), "Unexpected error: {err:?}");
}

#[test]
fn validation_only_with_mempool_enabled_fails() {
    // Disable gateway and http_server to reach the mempool check.
    let config = SequencerNodeConfig {
        validation_only: true,
        components: ComponentConfig {
            gateway: ReactiveComponentExecutionConfig::disabled(),
            http_server: ActiveComponentExecutionConfig::disabled(),
            mempool: ReactiveComponentExecutionConfig::default(),
            mempool_p2p: ReactiveComponentExecutionConfig::default(),
            ..Default::default()
        },
        gateway_config: None,
        http_server_config: None,
        state_sync_config: Some(state_sync_config_with_full_archive()),
        ..Default::default()
    };
    let err = config.validate_node_config().unwrap_err();
    assert!(format!("{err:?}").contains("mempool"), "Unexpected error: {err:?}");
}

#[test]
fn validation_only_with_mempool_p2p_enabled_fails() {
    // Disable gateway, http_server and mempool to reach the mempool_p2p check.
    let config = SequencerNodeConfig {
        validation_only: true,
        components: ComponentConfig {
            gateway: ReactiveComponentExecutionConfig::disabled(),
            http_server: ActiveComponentExecutionConfig::disabled(),
            mempool: ReactiveComponentExecutionConfig::disabled(),
            mempool_p2p: ReactiveComponentExecutionConfig::default(),
            ..Default::default()
        },
        gateway_config: None,
        http_server_config: None,
        mempool_config: None,
        state_sync_config: Some(state_sync_config_with_full_archive()),
        ..Default::default()
    };
    let err = config.validate_node_config().unwrap_err();
    assert!(format!("{err:?}").contains("mempool_p2p"), "Unexpected error: {err:?}");
}

// The config manager is a local infrastructure component; running it remotely would make its
// consumers (e.g. the mempool) reach it over a network RPC that can fail mid-request. Both
// remote-capable execution modes must be rejected at validation time so the client is always local.
#[test]
fn config_manager_remote_is_rejected() {
    // `config_manager_config` is None so the per-component "set iff running locally" check passes
    // (remote is not running locally) and we reach the config_manager-specific validation.
    let config = SequencerNodeConfig {
        components: ComponentConfig {
            config_manager: ReactiveComponentExecutionConfig::remote(VALID_URL.into(), VALID_PORT),
            ..Default::default()
        },
        config_manager_config: None,
        state_sync_config: Some(state_sync_config_with_full_archive()),
        ..Default::default()
    };
    let err = config.validate_node_config().unwrap_err();
    assert!(
        format!("{err:?}").contains("config_manager must run locally"),
        "Unexpected error: {err:?}"
    );
}

#[test]
fn config_manager_local_with_remote_enabled_is_rejected() {
    // This mode runs locally, so the default (Some) config_manager_config satisfies the
    // per-component check and we reach the config_manager-specific validation.
    let config = SequencerNodeConfig {
        components: ComponentConfig {
            config_manager: ReactiveComponentExecutionConfig::local_with_remote_enabled(
                VALID_URL.into(),
                VALID_PORT,
            ),
            ..Default::default()
        },
        state_sync_config: Some(state_sync_config_with_full_archive()),
        ..Default::default()
    };
    let err = config.validate_node_config().unwrap_err();
    assert!(
        format!("{err:?}").contains("config_manager must run locally"),
        "Unexpected error: {err:?}"
    );
}

#[test]
fn validation_only_with_tx_ingestion_disabled_succeeds() {
    let config = SequencerNodeConfig {
        validation_only: true,
        components: ComponentConfig {
            gateway: ReactiveComponentExecutionConfig::disabled(),
            http_server: ActiveComponentExecutionConfig::disabled(),
            mempool: ReactiveComponentExecutionConfig::disabled(),
            mempool_p2p: ReactiveComponentExecutionConfig::disabled(),
            ..Default::default()
        },
        gateway_config: None,
        http_server_config: None,
        mempool_config: None,
        mempool_p2p_config: None,
        state_sync_config: Some(state_sync_config_with_full_archive()),
        ..Default::default()
    };
    assert!(config.validate_node_config().is_ok());
}
