use std::path::PathBuf;

use jrsonnet_evaluator::trace::PathResolver;
use jrsonnet_evaluator::{FileImportResolver, State};
use serde_json::Value;
use strum::IntoEnumIterator;

use crate::service::{GetComponentConfigs, NodeService, NodeType};

const JSONNET_DIR: &str = "crates/apollo_deployments/jsonnet";

/// Evaluates `services/<layout>.jsonnet` (the per-layout infra renderer) and returns its JSON.
fn eval_layout_infra(layout: &str) -> Value {
    let state = jsonnet_state();
    let _guard = state.enter();
    let entry = format!("services/{layout}.jsonnet");
    let val = state.import(entry.as_str()).expect("failed to evaluate the layout infra renderer");
    serde_json::to_value(&val).expect("infra config is not serializable")
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
