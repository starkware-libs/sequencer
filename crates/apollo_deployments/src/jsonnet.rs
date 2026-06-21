use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use apollo_config::dumping::SerializeConfig;
use apollo_config::{FIELD_SEPARATOR, IS_NONE_MARK};
use apollo_node_config::config_utils::{config_to_preset, private_parameters};
use apollo_node_config::node_config::{SequencerNodeConfig, CONFIG_POINTERS};
use jrsonnet_evaluator::trace::PathResolver;
use jrsonnet_evaluator::{FileImportResolver, State};
use serde_json::Value;
use strum::IntoEnumIterator;

use crate::deployment_definitions::BASE_APP_CONFIGS_DIR_PATH;
use crate::service::{GetComponentConfigs, NodeService, NodeType, KEYS_TO_BE_REPLACED};
use crate::test_utils::is_path_prefix;

const JSONNET_DIR: &str = "crates/apollo_deployments/jsonnet";
const TESTING_OVERRIDES_PATH: &str = "testing/overrides.libsonnet";
/// Evaluates a jsonnet `snippet` against a fresh evaluator (stdlib installed, imports resolved
/// relative to the jsonnet dir) and converts the result to a serde `Value`. `context` labels the
/// evaluation in panic messages.
fn eval_jsonnet(context: &str, snippet: String) -> Value {
    let state = jsonnet_state();
    let _guard = state.enter();
    let val = state
        .evaluate_snippet(context.to_owned(), snippet)
        .unwrap_or_else(|error| panic!("failed to evaluate {context}: {error}"));
    serde_json::to_value(&val)
        .unwrap_or_else(|error| panic!("{context} is not serializable: {error}"))
}

/// Evaluates `services/<layout>.jsonnet` (the per-layout infra renderer) and returns its JSON.
fn eval_layout_infra(layout: &str) -> Value {
    eval_jsonnet("layout infra", format!("import 'services/{layout}.jsonnet'"))
}

/// A jrsonnet evaluator with the stdlib installed and file imports resolved relative to the jsonnet
/// dir (so the libraries' `std.*` calls and relative `import`s work).
fn jsonnet_state() -> State {
    let mut builder = State::builder();
    builder.context_initializer(jrsonnet_stdlib::ContextInitializer::new(PathResolver::Absolute));
    builder.import_resolver(FileImportResolver::new(vec![PathBuf::from(JSONNET_DIR)]));
    builder.build()
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
        let legacy_config = legacy.get(&node_service).unwrap();
        let legacy_value = serde_json::to_value(legacy_config).unwrap();
        let legacy_components = legacy_value.as_object().unwrap();
        let jsonnet_components = infra[&service_name]["components"].as_object().unwrap();

        assert_eq!(
            without_url_port(jsonnet_components),
            without_url_port(legacy_components),
            "infra config mismatch for service {service_name} (url/port excluded)"
        );
    }
}

/// Evaluates `build(layout, overrides)` and returns its JSON: a map from service name to that
/// service's fully-assembled config.
fn eval_build(layout: &str, overrides: &str) -> Value {
    let layout_literal = serde_json::to_string(layout).unwrap();
    eval_jsonnet(
        "build",
        format!("(import 'lib/build.libsonnet').build({layout_literal}, import '{overrides}')"),
    )
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
    let built = eval_build(&layout, TESTING_OVERRIDES_PATH);
    let services = built.as_object().unwrap();

    // Sanity check: the build result should have at least one service.
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

/// Asserts the applicative config emitted by jsonnet reproduces the committed `app_configs/*.json`
/// for every keys, except keys that are overridable, secret, or under `components.*`.
pub fn test_applicative_matches_app_configs() {
    // Applicative side: the single consolidated `node` service carries every component's business
    // config; round-trip through the config struct and render it in the app_configs preset format.
    let built = eval_build("consolidated", TESTING_OVERRIDES_PATH);
    let node = built.get("node").expect("consolidated has a `node` service").clone();
    let parsed: SequencerNodeConfig = serde_json::from_value(node).unwrap();
    let build_preset = config_to_preset(&serde_json::json!(parsed.dump()));
    let build_map = build_preset.as_object().unwrap();

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
                if build_value != app_config_value {
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
/// override path.
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
    let mut root = serde_json::Map::new();
    for path in paths {
        let parts: Vec<&str> = path.split(FIELD_SEPARATOR).collect();
        let (leaf, ancestors) = parts.split_last().expect("a path has at least one segment");
        let mut current = &mut root;
        for ancestor in ancestors {
            current = current
                .entry((*ancestor).to_owned())
                .or_insert_with(|| Value::Object(serde_json::Map::new()))
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
