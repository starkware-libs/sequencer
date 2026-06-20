use std::collections::{BTreeMap, BTreeSet};

use apollo_config::dumping::SerializeConfig;
use apollo_config::{FIELD_SEPARATOR, IS_NONE_MARK};
use apollo_node_config::config_utils::{config_to_preset, private_parameters};
use apollo_node_config::node_config::{SequencerNodeConfig, CONFIG_POINTERS};
use serde_json::{json, Map, Value};
use strum::IntoEnumIterator;
use tempfile::NamedTempFile;

use crate::deployment_definitions::BASE_APP_CONFIGS_DIR_PATH;
use crate::jsonnet_generation::{
    eval_build_with_expr,
    eval_build_with_overrides,
    eval_overrides_file,
    jsonnet_state,
    overrides_from_sequencer_config,
    service_config_to_preset,
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
    eval_build_with_expr(layout, "import 'testing/overrides.libsonnet'", None)
}

/// Flattens a nested JSON object into a map from dot-separated leaf path to value. Non-empty
/// objects recurse; scalars, arrays, nulls, and empty objects are leaves (the inverse of the
/// generator's nesting, so the round-trip below is well-defined).
fn flatten(value: &Value, prefix: &str, out: &mut BTreeMap<String, Value>) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            for (key, child) in map {
                let path = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
                flatten(child, &path, out);
            }
        }
        leaf => {
            out.insert(prefix.to_owned(), leaf.clone());
        }
    }
}

/// The generator's flat-input path — flatten the nested overrides, run them back through
/// `overrides_from_sequencer_config`, and `build` — reproduces building directly from the nested
/// fixture, over a complete (59-key) override set. (`testing/overrides.libsonnet` encodes `None` as
/// `null`, not the `#is_none` marker; that folding is covered by the mapping's own unit tests.)
pub fn test_generator_flat_input_matches_direct_build() {
    let nested = eval_overrides_file("testing/overrides.libsonnet");
    let mut flat = BTreeMap::new();
    flatten(&nested, "", &mut flat);

    // The mapping inverts the flattening.
    assert_eq!(overrides_from_sequencer_config(&flat), nested);

    // ...so the generator yields the same per-service config as building from the fixture directly.
    assert_eq!(
        eval_build_with_overrides("hybrid", &overrides_from_sequencer_config(&flat)),
        eval_test_build("hybrid"),
    );
}

/// The generator's flat output is node-loadable, valid, and faithful: for every hybrid service,
/// feeding `service_config_to_preset` (the binary's output) through the node's real loader
/// (`SequencerNodeConfig::load_and_process`, which resolves `CONFIG_POINTERS` + `#is_none`)
/// reconstructs exactly the `SequencerNodeConfig` that `build` produced and passes
/// `validate_node_config` — in particular the cross-member rule that each `<component>_config` is
/// set iff that component runs locally, which is what the per-service-tailored `build` satisfies.
/// Uses `testing/overrides`.
pub fn test_generator_config_is_node_loadable() {
    let built = eval_test_build("hybrid");
    let services = built.as_object().unwrap();
    for (service, config) in services {
        let direct: SequencerNodeConfig = serde_json::from_value(config.clone()).unwrap();
        let preset_file = NamedTempFile::new().unwrap();
        let preset = service_config_to_preset(config);
        std::fs::write(preset_file.path(), serde_json::to_string(&preset).unwrap()).unwrap();
        let dummy_entrypoint = String::new();
        let loaded = SequencerNodeConfig::load_and_process(vec![
            dummy_entrypoint,
            "--config_file".to_string(),
            preset_file.path().to_str().unwrap().to_string(),
        ])
        .unwrap();

        // Set dummy urls to pass the validation, other validations are done too.
        let mut validated = loaded.clone();
        validated.components.set_urls_to_localhost();
        validated.validate_node_config().unwrap();

        if direct != loaded {
            // Find the exact differences between the two configs.
            let mut direct_flat = BTreeMap::new();
            flatten(&serde_json::to_value(&direct).unwrap(), "", &mut direct_flat);
            let mut loaded_flat = BTreeMap::new();
            flatten(&serde_json::to_value(&loaded).unwrap(), "", &mut loaded_flat);
            let diffs: Vec<String> = direct_flat
                .keys()
                .chain(loaded_flat.keys())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .filter(|key| direct_flat.get(*key) != loaded_flat.get(*key))
                .map(|key| {
                    format!(
                        "{key}: direct={:?} loaded={:?}",
                        direct_flat.get(key),
                        loaded_flat.get(key)
                    )
                })
                .collect();
            panic!("service {service} round-trip diffs:\n  {}", diffs.join("\n  "));
        }
    }
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

/// Sentinel string injected at an override path; the suffix carries the dotted path so a value
/// surviving into the applicative output identifies which override the applicative read.
const OVERRIDE_SENTINEL_PREFIX: &str = "__override_sentinel__:";

/// Enforces that the applicative config's override schema agrees with `KEYS_TO_BE_REPLACED`, by
/// *evaluating* the applicative `function(overrides)` with a sentinel placed at every expected
/// override path and checking which sentinels flow into the output — insensitive to the jsonnet's
/// formatting (unlike scanning the source for `overrides.<path>` strings):
/// - A sentinel absent from the output is an override the applicative never reads, i.e.
///   `KEYS_TO_BE_REPLACED` declares a replacer key with no corresponding override read.
/// - If the applicative reads an override path absent from the expected set, jsonnet fails to
///   evaluate (the field is missing, or it indexes a sub-field of a scalar sentinel) — catching an
///   `overrides.<path>` reference with no `KEYS_TO_BE_REPLACED` entry.
pub fn test_keys_to_be_replaced_are_covered_by_override_schema() {
    let expected_paths = expected_override_paths();
    let applicative = eval_applicative_config(&sentinel_overrides(&expected_paths));

    let mut consumed_paths = BTreeSet::new();
    collect_sentinel_paths(&applicative, &mut consumed_paths);

    let unread_paths: Vec<&String> = expected_paths.difference(&consumed_paths).collect();
    assert!(
        unread_paths.is_empty(),
        "KEYS_TO_BE_REPLACED declares override paths the applicative config never reads (remove \
         the replacer key, or read `overrides.<path>` in applicative_config.libsonnet): \
         {unread_paths:#?}"
    );
}

/// The override read-paths the applicative config is expected to read, derived from
/// `KEYS_TO_BE_REPLACED`: an optional config marked `<path>.#is_none` is read whole at `<path>` (so
/// its sub-field replacer keys collapse into that single read), and every other replacer key is
/// read at its full path.
fn expected_override_paths() -> BTreeSet<String> {
    let is_none_suffix = format!("{FIELD_SEPARATOR}{IS_NONE_MARK}");
    let option_roots: BTreeSet<&str> =
        KEYS_TO_BE_REPLACED.iter().filter_map(|key| key.strip_suffix(&is_none_suffix)).collect();

    let mut paths: BTreeSet<String> = option_roots.iter().map(|root| (*root).to_owned()).collect();
    for key in KEYS_TO_BE_REPLACED.iter().copied() {
        let is_marker = key.ends_with(&is_none_suffix);
        let under_option_root = option_roots.iter().any(|root| is_path_prefix(root, key));
        if !is_marker && !under_option_root {
            paths.insert(key.to_owned());
        }
    }
    paths
}

/// Builds a nested `overrides` object placing a path-encoding sentinel string at each dotted path.
fn sentinel_overrides(paths: &BTreeSet<String>) -> Value {
    let mut root = Map::new();
    for path in paths {
        let parts: Vec<&str> = path.split(FIELD_SEPARATOR).collect();
        let (leaf, ancestors) = parts.split_last().expect("a path has at least one segment");
        let mut current = &mut root;
        for ancestor in ancestors {
            current = current
                .entry((*ancestor).to_owned())
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .expect("override path prefix-conflicts with another override path");
        }
        current
            .insert((*leaf).to_owned(), Value::String(format!("{OVERRIDE_SENTINEL_PREFIX}{path}")));
    }
    Value::Object(root)
}

/// Evaluates the applicative `function(overrides)` (`lib/applicative_config.libsonnet`) applied to
/// `overrides`.
fn eval_applicative_config(overrides: &Value) -> Value {
    let overrides_literal = serde_json::to_string(overrides).expect("overrides is serializable");
    let state = jsonnet_state();
    let _guard = state.enter();
    let snippet = format!("(import 'lib/applicative_config.libsonnet')({overrides_literal})");
    let val = state
        .evaluate_snippet("applicative_entry.jsonnet", snippet)
        .expect("applicative_config.libsonnet failed to evaluate");
    // jrsonnet evaluates lazily, so a bad field access surfaces here (when the thunks are forced),
    // not at `evaluate_snippet`. A "no such field" error means the applicative reads an
    // `overrides.<path>` with no KEYS_TO_BE_REPLACED entry, or indexes a sub-field of an optional
    // config marked `.#is_none` (whose sentinel is a scalar).
    serde_json::to_value(&val).unwrap_or_else(|error| {
        panic!(
            "applicative_config.libsonnet read an override path not declared in \
             KEYS_TO_BE_REPLACED (add the key), or indexed a sub-field of a `.#is_none` optional \
             config: {error}"
        )
    })
}

/// Collects the dotted paths encoded in every override sentinel string found anywhere in `value`.
fn collect_sentinel_paths(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::String(string) => {
            if let Some(path) = string.strip_prefix(OVERRIDE_SENTINEL_PREFIX) {
                out.insert(path.to_owned());
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect_sentinel_paths(item, out)),
        Value::Object(map) => map.values().for_each(|child| collect_sentinel_paths(child, out)),
        _ => {}
    }
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
