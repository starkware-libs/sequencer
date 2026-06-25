use std::collections::BTreeSet;

use apollo_config::secrets::DEFAULT_REDACTION_OUTPUT;
use apollo_config::{ParamPath, FIELD_SEPARATOR};
use apollo_infra::component_client::RemoteClientConfig;
use apollo_infra::component_server::{LocalServerConfig, RemoteServerConfig};
use apollo_state_sync_config::config::{StateSyncConfig, StateSyncStaticConfig};
use apollo_storage::{StorageConfig, StorageScope};
use rstest::rstest;
use serde_json::Value;
use starknet_api::contract_address;
use starknet_api::core::ChainId;
use validator::Validate;

use crate::component_config::ComponentConfig;
use crate::component_execution_config::{
    ActiveComponentExecutionConfig,
    ReactiveComponentExecutionConfig,
    ReactiveComponentExecutionMode,
};
use crate::config_utils::{normalize_pointer_groups, private_parameters};
use crate::monitoring::MonitoringConfig;
use crate::node_config::{SequencerNodeConfig, CONFIG_SECRETS_SCHEMA_PATH};

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

/// Collects the dotted paths of every leaf in `value` whose serialized form is the
/// `Sensitive<T>` default-redaction sentinel. These are the secret fields the type system can
/// detect automatically. Paths use [`FIELD_SEPARATOR`] to match the secrets-schema convention.
fn default_redacted_paths(value: &Value, prefix: &str, paths: &mut BTreeSet<ParamPath>) {
    match value {
        Value::String(string_value) if string_value == DEFAULT_REDACTION_OUTPUT => {
            paths.insert(prefix.to_owned());
        }
        Value::Object(map) => {
            for (key, child) in map {
                let child_prefix = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}{FIELD_SEPARATOR}{key}")
                };
                default_redacted_paths(child, &child_prefix, paths);
            }
        }
        // A `Vec<Sensitive<T>>`/`Option<Vec<Sensitive<T>>>` secret (e.g.
        // `ordered_l1_endpoint_urls`, `url_header_list`) serializes to an array of
        // redaction sentinels. The whole field is one schema entry, so recurse into each
        // element with the SAME prefix (no per-element index): any redacted element marks
        // the field's own path.
        Value::Array(items) => {
            for item in items {
                default_redacted_paths(item, prefix, paths);
            }
        }
        _ => {}
    }
}

/// Safety guard for the hand-maintained secrets schema (`CONFIG_SECRETS_SCHEMA_PATH`).
///
/// Walks the serialized default config for `Sensitive<T>` default-redaction sentinels and asserts
/// every such field is declared in the committed schema. This catches a newly added secret field
/// (with the default redactor) that wasn't added to the schema.
///
/// Best-effort by design: the schema is now hand-maintained (the generator binary was retired), so
/// this guard only enforces the subset direction. Gaps it cannot cover:
/// - `Sensitive<T>` fields that attach a custom redactor serialize to that custom string, not the
///   default sentinel, so they are not detected here.
/// - Secret fields enforced only via `#[serde(deserialize_with = ...)]` (no `Sensitive<T>` type)
///   have no serialized marker at all.
/// Such fields, and any unset (`None`) optional secrets, must be kept in the schema by hand.
#[test]
fn secrets_schema_contains_all_default_redacted_fields() {
    let default_config_value = serde_json::to_value(SequencerNodeConfig::default())
        .expect("Should be able to serialize the default config to a JSON value");
    let mut detected_secret_paths = BTreeSet::new();
    default_redacted_paths(&default_config_value, "", &mut detected_secret_paths);

    // Guard against the detector silently regressing to vacuous (e.g. a serialized shape that stops
    // hitting any arm, like the missing array arm this once had). At least the known array-typed L1
    // endpoint secret must be detected; otherwise the subset check below is meaningless.
    assert!(
        detected_secret_paths.contains("base_layer_config.ordered_l1_endpoint_urls"),
        "secret detector found no default-redacted array fields — the recursion is likely not \
         descending into arrays; detected: {detected_secret_paths:?}"
    );

    let committed_secret_paths = private_parameters();
    let missing_paths: Vec<&ParamPath> =
        detected_secret_paths.difference(&committed_secret_paths).collect();
    assert!(
        missing_paths.is_empty(),
        "The following default-redacted secret fields are missing from the committed secrets \
         schema ({CONFIG_SECRETS_SCHEMA_PATH}). Add them by hand: {missing_paths:?}"
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
    let mut config = SequencerNodeConfig {
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
    // `SequencerNodeConfig::default()` does not have internally consistent pointer-group values
    // (those are only reconciled by pointer resolution at load time), so normalize them before
    // exercising the validation_only logic this test targets.
    normalize_pointer_groups(&mut config);
    assert!(config.validate_node_config().is_ok());
}

/// A config assembled directly from `SequencerNodeConfig::default()` is not internally consistent
/// on pointer-group values, so after normalizing those groups it validates `Ok`. This is the
/// "full" positive case: every component is present and every group agrees.
#[test]
fn pointer_groups_consistent_full_config_validates() {
    let mut config = SequencerNodeConfig::default();
    normalize_pointer_groups(&mut config);
    assert!(
        config.validate_node_config().is_ok(),
        "normalized full config should validate: {:?}",
        config.validate_node_config()
    );
}

/// Present-only guard: when only one owner of a pointer group is present (a partial/distributed
/// deployment), the equality check has nothing to compare against and validates `Ok`.
#[test]
fn pointer_groups_single_present_owner_validates() {
    // Only `gateway_config` owns `native_classes_whitelist`/`validate_resource_bounds`; with the
    // batcher and mempool absent there is a single present value, so the group is trivially equal.
    let mut config = SequencerNodeConfig {
        batcher_config: None,
        mempool_config: None,
        ..SequencerNodeConfig::default()
    };
    // Disable the now-absent components so the per-component "set iff running locally" check
    // passes.
    config.components.batcher = ReactiveComponentExecutionConfig::disabled();
    config.components.mempool = ReactiveComponentExecutionConfig::disabled();
    normalize_pointer_groups(&mut config);
    assert!(
        config.validate_node_config().is_ok(),
        "single-owner config should validate: {:?}",
        config.validate_node_config()
    );
}

/// Negative: a uniform shared field (`chain_id`) diverging between two present owners fails.
#[test]
fn pointer_group_chain_id_mismatch_fails() {
    let mut config = SequencerNodeConfig::default();
    normalize_pointer_groups(&mut config);
    // Diverge the gateway's chain_id from everyone else's.
    config.gateway_config.as_mut().unwrap().static_config.chain_info.chain_id = ChainId::Sepolia;
    let err = config.validate_node_config().unwrap_err();
    assert!(format!("{err:?}").contains("chain_id"), "Unexpected error: {err:?}");
}

/// Negative covering the fee-token name asymmetry: the batcher/gateway `eth_fee_token_address` and
/// the state_sync `eth_fee_contract_address` are the same logical value; diverging them fails.
#[test]
fn pointer_group_eth_fee_token_name_asymmetry_mismatch_fails() {
    let mut config = SequencerNodeConfig::default();
    normalize_pointer_groups(&mut config);
    // state_sync stores it under `eth_fee_contract_address`; diverge it from the gateway/batcher.
    config
        .state_sync_config
        .as_mut()
        .unwrap()
        .static_config
        .rpc_config
        .execution_config
        .eth_fee_contract_address = contract_address!("0xdead");
    let err = config.validate_node_config().unwrap_err();
    assert!(format!("{err:?}").contains("eth_fee_token_address"), "Unexpected error: {err:?}");
}

/// Negative: the node-level `validation_only` source disagreeing with the batcher's copy (its lone
/// pointer target, which actually drives batcher behavior) fails. Guards the source-vs-target
/// group.
#[test]
fn pointer_group_validation_only_mismatch_fails() {
    let mut config = SequencerNodeConfig::default();
    normalize_pointer_groups(&mut config);
    // Top-level `validation_only` is false (set by `normalize_pointer_groups`); diverge the
    // batcher's copy. The top-level flag stays false, so `validate_validation_only_config` is a
    // no-op and the equality group is what must catch this.
    config.batcher_config.as_mut().unwrap().static_config.validation_only = true;
    let err = config.validate_node_config().unwrap_err();
    assert!(format!("{err:?}").contains("validation_only"), "Unexpected error: {err:?}");
}
