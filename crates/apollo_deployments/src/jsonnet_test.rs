use std::collections::{BTreeMap, BTreeSet};

use apollo_config::dumping::SerializeConfig;
use apollo_config::{FIELD_SEPARATOR, IS_NONE_MARK};
use apollo_node_config::config_utils::{config_to_preset, private_parameters};
use apollo_node_config::node_config::{SequencerNodeConfig, CONFIG_POINTERS};
use serde_json::{json, Value};
use strum::IntoEnumIterator;

use crate::deployment_definitions::BASE_APP_CONFIGS_DIR_PATH;
use crate::jsonnet_generation::{
    eval_build_with_expr,
    jsonnet_state,
    overrides_from_sequencer_config,
};
use crate::service::{GetComponentConfigs, NodeService, NodeType, KEYS_TO_BE_REPLACED};
use crate::utils::is_path_prefix;

/// Evaluates `services/<layout>.jsonnet` (the per-layout infra renderer) and returns its JSON.
fn eval_layout_infra(layout: &str) -> Value {
    let state = jsonnet_state();
    let _guard = state.enter();
    let entry = format!("services/{layout}.jsonnet");
    let val = state.import(entry.as_str()).expect("failed to evaluate the layout infra renderer");
    serde_json::to_value(&val).expect("infra config is not serializable")
}

/// Asserts the jsonnet-derived infra of every service of layout `S` matches the Rust source of
/// truth (`<layout>.rs`'s `get_component_configs`).
pub(crate) fn assert_infra_matches_rust<S>()
where
    S: GetComponentConfigs + IntoEnumIterator + Into<NodeService>,
{
    // Derive the layout name (the jsonnet renderer's file stem) from S's NodeType, then evaluate
    // it.
    let some_service: NodeService =
        S::iter().next().expect("a layout has at least one service").into();
    let infra = eval_layout_infra(&NodeType::from(&some_service).to_string());

    let ports_override = None;
    let legacy = S::get_component_configs(ports_override);
    for service in S::iter() {
        let service_name = service.to_string();
        let node_service: NodeService = service.into();
        let legacy_config = legacy
            .get(&node_service)
            .expect("the Rust deployment has no component config for this service");
        let legacy_value =
            serde_json::to_value(legacy_config).expect("ComponentConfig is not serializable");
        let legacy_components =
            legacy_value.as_object().expect("ComponentConfig serializes to an object");
        let jsonnet_components = infra[&service_name]["components"]
            .as_object()
            .expect("jsonnet components is an object");

        assert_eq!(
            without_url_port(jsonnet_components),
            without_url_port(legacy_components),
            "infra config mismatch for service {service_name} (url/port excluded)"
        );
    }
}

/// Asserts that `build(layout, testing_overrides)` produces, for every service of layout `S`, an
/// object that deserializes into `SequencerNodeConfig`.
pub(crate) fn assert_build_deserializes<S>()
where
    S: GetComponentConfigs + IntoEnumIterator + Into<NodeService>,
{
    let some_service: NodeService =
        S::iter().next().expect("a layout has at least one service").into();
    let layout = NodeType::from(&some_service).to_string();
    let built = eval_test_build(&layout);
    let services = built.as_object().expect("build result is a service-keyed object");
    assert!(!services.is_empty(), "build({layout}) produced no services");

    for (service_name, config) in services {
        serde_json::from_value::<SequencerNodeConfig>(config.clone()).unwrap_or_else(|error| {
            panic!(
                "service {service_name} of layout {layout} does not deserialize into \
                 SequencerNodeConfig: {error}"
            )
        });
    }
}

fn eval_test_build(layout: &str) -> Value {
    eval_build_with_expr(layout, "import 'testing/overrides.libsonnet'")
}

/// Asserts the applicative config emitted by jsonnet reproduces the committed `app_configs/*.json`
/// — the deployment's non-overridable value layer (loaded on top of `config_schema.json` at deploy)
/// — for every key those files define. Excludes keys that are overridable, secret, under
/// `components.*`, not jsonnet-representable (> 2^53).
pub fn test_applicative_matches_app_configs() {
    // Applicative side: the single consolidated `node` service carries every component's business
    // config; round-trip through the config struct and render it in the app_configs preset format.
    let built = eval_test_build("consolidated");
    let node = built.get("node").expect("consolidated has a `node` service").clone();
    let parsed: SequencerNodeConfig =
        serde_json::from_value(node).expect("build output deserializes into SequencerNodeConfig");
    let build_preset = config_to_preset(&serde_json::json!(parsed.dump()));
    let build_map = build_preset.as_object().expect("preset is a JSON object");

    let excluded = non_default_paths();
    let is_excluded = |path: &str| {
        is_path_prefix("components", path) || excluded.iter().any(|key| is_path_prefix(key, path))
    };

    let app_config_map = merged_app_configs();

    let mut mismatches = Vec::new();
    for (key, app_config_value) in &app_config_map {
        if is_excluded(key) {
            continue;
        }
        match build_map.get(key) {
            Some(build_value) => {
                if !values_equal(build_value, app_config_value) {
                    mismatches.push(format!(
                        "{key}: applicative={build_value} app_config={app_config_value}"
                    ));
                }
            }
            None => mismatches
                .push(format!("{key}: missing in applicative (app_config={app_config_value})")),
        }
    }

    assert!(
        mismatches.is_empty(),
        "applicative config diverges from app_configs/*.json at {} non-overridable, non-secret \
         keys:\n  {}",
        mismatches.len(),
        mismatches.join("\n  ")
    );
}

/// Merges every base `app_configs/<component>.json` (skipping the derived `replacer_*` files) into
/// a single flat dotted-key map.
fn merged_app_configs() -> BTreeMap<String, Value> {
    let mut app_config_map: BTreeMap<String, Value> = BTreeMap::new();
    for entry in std::fs::read_dir(BASE_APP_CONFIGS_DIR_PATH).expect("app_configs dir exists") {
        let path = entry.expect("readable dir entry").path();
        let is_json = path.extension().is_some_and(|extension| extension == "json");
        let is_replacer = path.file_name().unwrap().to_string_lossy().starts_with("replacer_");
        if !is_json || is_replacer {
            continue;
        }
        let contents = std::fs::read_to_string(&path).expect("app_config file is readable");
        let object: serde_json::Map<String, Value> =
            serde_json::from_str(&contents).expect("app_config is a JSON object");
        app_config_map.extend(object);
    }
    app_config_map
}

// TODO(Nimrod): Remove this by making the deserialization go through u64 rather than f64.
/// Numeric-tolerant JSON equality: two numbers compare by value (so `12` equals `12.0`), everything
/// else compares structurally.
fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left_number), Value::Number(right_number)) => {
            left_number.as_f64() == right_number.as_f64()
        }
        _ => left == right,
    }
}

/// The config paths that are overridable or secrets or passed as pointers.
fn non_default_paths() -> BTreeSet<String> {
    // An optional config is marked overridable/secret as `<path>.#is_none`; the override replaces
    // the whole option, so exclude the `<path>` subtree (not just the marker).
    let is_none_suffix = format!("{FIELD_SEPARATOR}{IS_NONE_MARK}");
    let insert_with_option_root = |paths: &mut BTreeSet<String>, key: &str| {
        paths.insert(key.to_string());
        if let Some(option_root) = key.strip_suffix(&is_none_suffix) {
            paths.insert(option_root.to_string());
        }
    };

    let mut paths = BTreeSet::new();
    for key in KEYS_TO_BE_REPLACED.iter() {
        insert_with_option_root(&mut paths, key);
    }
    for ((target_path, _param), pointing_paths) in CONFIG_POINTERS.iter() {
        paths.insert(target_path.clone());
        paths.extend(pointing_paths.iter().cloned());
    }
    for key in private_parameters() {
        insert_with_option_root(&mut paths, &key);
    }
    paths
}

/// Clones a `components` map with `url` and `port` removed from each component object — the two
/// fields the Rust config leaves as deploy-time placeholders, so they can't be compared against the
/// jsonnet's baked-in real values.
fn without_url_port(components: &serde_json::Map<String, Value>) -> serde_json::Map<String, Value> {
    components
        .iter()
        .map(|(name, config)| {
            let mut config = config.clone();
            if let Some(object) = config.as_object_mut() {
                object.remove("url");
                object.remove("port");
            }
            (name.clone(), config)
        })
        .collect()
}

fn flat(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
    pairs.iter().map(|(key, value)| (key.to_string(), value.clone())).collect()
}

#[test]
fn overrides_nest_and_fold_none() {
    let input = flat(&[
        ("chain_id", json!("SN_SEPOLIA")),
        ("batcher_config.dynamic_config.n_concurrent_txs", json!(100)),
        // `None` scalar option: marker `true` + placeholder value -> `null`.
        ("consensus_manager_config.network_config.advertised_multiaddr", json!("")),
        ("consensus_manager_config.network_config.advertised_multiaddr.#is_none", json!(true)),
        // `Some` scalar option: marker `false` + value -> the value.
        ("consensus_manager_config.network_config.bootstrap_peer_multiaddr", json!("/dns/x")),
        ("consensus_manager_config.network_config.bootstrap_peer_multiaddr.#is_none", json!(false)),
        // `None` struct option: marker `true` + materialized children -> `null`, children dropped.
        ("batcher_config.static_config.first_block_with_partial_block_hash.#is_none", json!(true)),
        ("batcher_config.static_config.first_block_with_partial_block_hash.block_number", json!(0)),
        (
            "batcher_config.static_config.first_block_with_partial_block_hash.block_hash",
            json!("0x0"),
        ),
        // `Some` struct option with no sub-field overrides: marker `false`, no children -> `{}`
        // (present, so it deserializes to `Some(default)` rather than an absent field).
        ("state_sync_config.static_config.p2p_sync_client_config.#is_none", json!(false)),
        ("components.batcher.port", json!(55000)),
        ("components.class_manager.url", json!("sequencer-core-service")),
    ]);

    let expected = json!({
        "chain_id": "SN_SEPOLIA",
        "batcher_config": {
            "dynamic_config": { "n_concurrent_txs": 100 },
            "static_config": { "first_block_with_partial_block_hash": null },
        },
        "consensus_manager_config": {
            "network_config": {
                "advertised_multiaddr": null,
                "bootstrap_peer_multiaddr": "/dns/x",
            },
        },
        "state_sync_config": { "static_config": { "p2p_sync_client_config": {} } },
        "components": {
            "batcher": { "port": 55000 },
            "class_manager": { "url": "sequencer-core-service" },
        },
    });

    assert_eq!(overrides_from_sequencer_config(&input), expected);
}

#[test]
fn overrides_none_option_does_not_collapse_prefix_sibling() {
    // A `None` option must not collapse a sibling whose name it merely string-prefixes.
    let input = flat(&[
        ("a.b.range_check.#is_none", json!(true)),
        ("a.b.range_check", json!(123)),
        ("a.b.range_check96", json!(456)),
    ]);
    let expected = json!({ "a": { "b": { "range_check": null, "range_check96": 456 } } });
    assert_eq!(overrides_from_sequencer_config(&input), expected);
}
