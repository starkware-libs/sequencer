use std::path::PathBuf;

use jrsonnet_evaluator::trace::PathResolver;
use jrsonnet_evaluator::{FileImportResolver, State};
use serde_json::Value;
use strum::IntoEnumIterator;

use crate::deployments::consolidated::ConsolidatedNodeServiceName;
use crate::deployments::hybrid::HybridNodeServiceName;
use crate::service::{GetComponentConfigs, NodeService, NodeType};

const JSONNET_DIR: &str = "crates/apollo_deployments/jsonnet";

/// Evaluates `services/<layout>.jsonnet` (the per-layout infra renderer) and returns its JSON: a
/// map from service name to that service's infra (`{ components }`). Uses a `FileImportResolver`
/// rooted at the jsonnet dir so the renderer's relative imports resolve. (Snippets can't `import`,
/// so we evaluate the real entry file.)
fn eval_layout_infra(layout: &str) -> Value {
    let mut builder = State::builder();
    // The infra library uses `std.*`, so install the jsonnet stdlib + a file import resolver.
    builder.context_initializer(jrsonnet_stdlib::ContextInitializer::new(PathResolver::Absolute));
    builder.import_resolver(FileImportResolver::new(vec![PathBuf::from(JSONNET_DIR)]));
    let state = builder.build();

    let _guard = state.enter();
    let entry = format!("services/{layout}.jsonnet");
    let val = state.import(entry.as_str()).expect("failed to evaluate the layout infra renderer");
    serde_json::to_value(&val).expect("infra config is not serializable")
}

/// Asserts the jsonnet-derived infra of every service of layout `S` matches the Rust source of
/// truth (`<layout>.rs`'s `get_component_configs`). For each service it compares the full
/// per-component config, minus `url`/`port`: the Rust config leaves those two as deploy-time
/// placeholders (`remote_service` / a placeholder port) that the overlay later replaces, whereas
/// the jsonnet bakes the real layout-constant values. Everything else — execution_mode, local/remote
/// server config, remote client config (including the per-service retries/idle) — must match
/// exactly. This pins the jsonnet derivation to the authoritative Rust definitions so the two cannot
/// silently diverge.
///
/// `S` is the layout's service-name enum (e.g. `HybridNodeServiceName`). The jsonnet renderer
/// (`services/<layout>.jsonnet`) is located by deriving the layout name from `S`'s `NodeType`
/// (snake_case); each service's snake_case `Display` is the jsonnet key and `Into<NodeService>` is
/// the legacy-config map key.
fn assert_infra_matches_rust<S>()
where
    S: GetComponentConfigs + IntoEnumIterator + Into<NodeService>,
{
    // Derive the layout name (the jsonnet renderer's file stem) from S's NodeType, then evaluate it.
    let some_service: NodeService =
        S::iter().next().expect("a layout has at least one service").into();
    let infra = eval_layout_infra(&NodeType::from(&some_service).to_string());

    let legacy = S::get_component_configs(None);
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

/// The jsonnet hybrid infra matches `deployments/hybrid.rs`.
pub fn test_hybrid_infra_matches_rust() {
    assert_infra_matches_rust::<HybridNodeServiceName>();
}

/// The jsonnet consolidated infra matches `deployments/consolidated.rs`.
pub fn test_consolidated_infra_matches_rust() {
    assert_infra_matches_rust::<ConsolidatedNodeServiceName>();
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
