use std::collections::BTreeSet;

use apollo_config::behavior_mode::BehaviorMode;
use apollo_config::dumping::SerializeConfig;
use apollo_config::ParamPath;
use apollo_infra::component_client::RemoteClientConfig;
use apollo_infra::component_server::{LocalServerConfig, RemoteServerConfig};
use apollo_infra_utils::dumping::serialize_to_file_test;
use apollo_reverts::RevertConfig;
use apollo_state_sync_config::config::{StateSyncConfig, StateSyncStaticConfig};
use apollo_storage::{StorageConfig, StorageScope};
use blockifier::blockifier::config::NativeClassesWhitelist;
use rstest::rstest;
use starknet_api::contract_address;
use starknet_api::core::ChainId;
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

/// Overwrites every present target of each multi-target `CONFIG_POINTERS` group with a single,
/// consistent value, mirroring what pointer resolution does at load time. Lets a config assembled
/// directly from `SequencerNodeConfig::default()` satisfy the cross-component equality invariant.
fn normalize_pointer_groups(config: &mut SequencerNodeConfig) {
    let chain_id = ChainId::Mainnet;
    let eth_fee_token_address = contract_address!("0x1");
    let strk_fee_token_address = contract_address!("0x2");
    let max_cpu_time: u64 = 600;

    config.validation_only = false;
    if let Some(sierra_compiler) = config.sierra_compiler_config.as_mut() {
        sierra_compiler.max_cpu_time = max_cpu_time;
    }
    if let Some(batcher) = config.batcher_config.as_mut() {
        let static_config = &mut batcher.static_config;
        static_config.block_builder_config.chain_info.chain_id = chain_id.clone();
        static_config.storage.db_config.chain_id = chain_id.clone();
        let fee_token_addresses =
            &mut static_config.block_builder_config.chain_info.fee_token_addresses;
        fee_token_addresses.eth_fee_token_address = eth_fee_token_address;
        fee_token_addresses.strk_fee_token_address = strk_fee_token_address;
        static_config.contract_class_manager_config.native_compiler_config.max_cpu_time =
            max_cpu_time;
        static_config.pre_confirmed_cende_config.recorder_url =
            "https://recorder_url".parse().unwrap();
        static_config.block_builder_config.versioned_constants_overrides = None;
        static_config.validation_only = false;
        batcher.dynamic_config.native_classes_whitelist = NativeClassesWhitelist::All;
    }
    if let Some(class_manager) = config.class_manager_config.as_mut() {
        class_manager
            .static_config
            .class_storage_config
            .class_hash_storage_config
            .db_config
            .chain_id = chain_id.clone();
    }
    if let Some(consensus_manager) = config.consensus_manager_config.as_mut() {
        consensus_manager
            .consensus_manager_config
            .static_config
            .storage_config
            .db_config
            .chain_id = chain_id.clone();
        consensus_manager.context_config.static_config.chain_id = chain_id.clone();
        consensus_manager.network_config.chain_id = chain_id.clone();
        consensus_manager.context_config.static_config.behavior_mode = BehaviorMode::Starknet;
        consensus_manager.cende_config.recorder_url = "https://recorder_url".parse().unwrap();
        consensus_manager.revert_config = RevertConfig::default();
    }
    if let Some(gateway) = config.gateway_config.as_mut() {
        gateway.static_config.chain_info.chain_id = chain_id.clone();
        let fee_token_addresses = &mut gateway.static_config.chain_info.fee_token_addresses;
        fee_token_addresses.eth_fee_token_address = eth_fee_token_address;
        fee_token_addresses.strk_fee_token_address = strk_fee_token_address;
        gateway.static_config.contract_class_manager_config.native_compiler_config.max_cpu_time =
            max_cpu_time;
        gateway.static_config.stateful_tx_validator_config.validate_resource_bounds = true;
        gateway.static_config.stateless_tx_validator_config.validate_resource_bounds = true;
        gateway.static_config.stateful_tx_validator_config.versioned_constants_overrides = None;
        gateway.dynamic_config.native_classes_whitelist = NativeClassesWhitelist::All;
    }
    if let Some(l1_events_scraper) = config.l1_events_scraper_config.as_mut() {
        l1_events_scraper.chain_id = chain_id.clone();
    }
    if let Some(l1_gas_price_scraper) = config.l1_gas_price_scraper_config.as_mut() {
        l1_gas_price_scraper.chain_id = chain_id.clone();
    }
    if let Some(mempool) = config.mempool_config.as_mut() {
        mempool.static_config.recorder_url = "https://recorder_url".parse().unwrap();
        mempool.static_config.validate_resource_bounds = true;
        mempool.static_config.behavior_mode = BehaviorMode::Starknet;
    }
    if let Some(mempool_p2p) = config.mempool_p2p_config.as_mut() {
        mempool_p2p.network_config.chain_id = chain_id.clone();
    }
    if let Some(state_sync) = config.state_sync_config.as_mut() {
        let static_config = &mut state_sync.static_config;
        static_config.storage_config.db_config.chain_id = chain_id.clone();
        if let Some(network_config) = static_config.network_config.as_mut() {
            network_config.chain_id = chain_id.clone();
        }
        static_config.rpc_config.chain_id = chain_id.clone();
        static_config.rpc_config.execution_config.eth_fee_contract_address = eth_fee_token_address;
        static_config.rpc_config.execution_config.strk_fee_contract_address =
            strk_fee_token_address;
        static_config.revert_config = RevertConfig::default();
    }
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
