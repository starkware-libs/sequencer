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
