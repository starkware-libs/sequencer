//! The jsonnet `build()` evaluator, exposed to other crates (the integration-test harness) via the
//! `testing` feature so they can source node configs from jsonnet without pulling jrsonnet into the
//! default/production dependency graph. The test-only parity/applicative helpers live in the
//! sibling `jsonnet_tests` module (compiled only under `test`).

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::str::FromStr;

use apollo_node_config::component_config::ComponentConfig;
use jrsonnet_evaluator::trace::PathResolver;
use jrsonnet_evaluator::{FileImportResolver, State};
use serde_json::Value;

use crate::deployments::consolidated::ConsolidatedNodeServiceName;
use crate::deployments::distributed::DistributedNodeServiceName;
use crate::deployments::hybrid::HybridNodeServiceName;
use crate::service::{NodeService, NodeType};

const JSONNET_DIR: &str = "deployments/sequencer/configs/jsonnet";
const TOPOLOGY_PARAMS: &str = "{ chain_params: import 'testing/chain_params.libsonnet', \
                               node_params: import 'testing/node_params.libsonnet' }";

/// Evaluates `build(<params>)` with the `<layout>` layout folded into
/// `chain_params.mandatory.topology`, and returns the per-service config map: service name → that
/// service's fully-assembled `SequencerNodeConfig` as JSON. `layout` is the layout name, used to
/// import the layout object. `params` is a jsonnet expression yielding the `{ chain_params,
/// node_params }` object — e.g. an inline object literal or `"import
/// 'testing/integration_node.jsonnet'"`. Paths in `params` resolve relative to the jsonnet dir (the
/// evaluator's import root).
pub fn build_service_configs(layout: &str, params: &str) -> serde_json::Map<String, Value> {
    // `layout` is the snake_case layout name (`hybrid`/`consolidated`/`distributed`) — it must
    // match the `lib/layouts/<name>.libsonnet` filename.
    let built = eval_jsonnet(
        "build",
        format!(
            "(import 'lib/build.libsonnet').build({params} + {{ chain_params+: {{ mandatory+: {{ \
             topology: import 'lib/layouts/{layout}.libsonnet' }} }} }})"
        ),
    );
    match built {
        Value::Object(services) => services,
        other => panic!("build({layout}) did not produce a service-map object: {other}"),
    }
}

/// Returns each service's topology as a typed `ComponentConfig` for `node_type`, keyed by
/// `NodeService`.
pub fn build_component_configs(
    node_type: NodeType,
    ports: Option<Vec<u16>>,
) -> HashMap<NodeService, ComponentConfig> {
    let mut services = build_service_configs(&node_type.to_string(), TOPOLOGY_PARAMS);
    if let Some(ports) = ports {
        remap_component_ports(&mut services, ports);
    }
    services
        .into_iter()
        .map(|(service_name, mut service_config)| {
            let node_service = node_service_from_name(node_type, &service_name);
            let components = service_config.get_mut("components").map(Value::take).unwrap();
            let component_config = serde_json::from_value::<ComponentConfig>(components).unwrap();
            (node_service, component_config)
        })
        .collect()
}

/// Replaces `build()`'s baked deploy-time component ports with the runtime-allocated `ports`.
fn remap_component_ports(services: &mut serde_json::Map<String, Value>, ports: Vec<u16>) {
    let component_port = |component: &Value| component.get("port").and_then(Value::as_u64);
    let fixed_ports: BTreeSet<u64> = services
        .values()
        .filter_map(|config| config.get("components").and_then(Value::as_object))
        .flat_map(|components| components.values())
        .filter_map(component_port)
        .filter(|&port| port != 0)
        .collect();
    assert_eq!(
        ports.len(),
        fixed_ports.len(),
        "runtime port count ({}) must equal the distinct baked component ports ({})",
        ports.len(),
        fixed_ports.len()
    );
    let port_map: HashMap<u64, u64> =
        fixed_ports.into_iter().zip(ports.into_iter().map(u64::from)).collect();
    for config in services.values_mut() {
        let Some(components) = config.get_mut("components").and_then(Value::as_object_mut) else {
            continue;
        };
        for component in components.values_mut() {
            if let Some(runtime_port) = component_port(component).and_then(|p| port_map.get(&p)) {
                component["port"] = Value::from(*runtime_port);
            }
        }
    }
}

fn node_service_from_name(node_type: NodeType, name: &str) -> NodeService {
    match node_type {
        NodeType::Consolidated => ConsolidatedNodeServiceName::from_str(name).unwrap().into(),
        NodeType::Hybrid => HybridNodeServiceName::from_str(name).unwrap().into(),
        NodeType::Distributed => DistributedNodeServiceName::from_str(name).unwrap().into(),
    }
}

/// Evaluates a jsonnet `snippet` against a fresh evaluator (stdlib installed, imports resolved
/// relative to the jsonnet dir) and converts the result to a serde `Value`. `context` labels the
/// evaluation in panic messages.
pub(crate) fn eval_jsonnet(context: &str, snippet: String) -> Value {
    let state = jsonnet_state();
    let _guard = state.enter();
    let val = state
        .evaluate_snippet(context.to_owned(), snippet)
        .expect("Failed to evaluate jsonnet snippet.");
    serde_json::to_value(&val).expect("Failed to serialize jsonnet result to Value.")
}

/// A jrsonnet evaluator with the stdlib installed and file imports resolved relative to the jsonnet
/// dir (so the libraries' `std.*` calls and relative `import`s work).
fn jsonnet_state() -> State {
    let mut builder = State::builder();
    builder.context_initializer(jrsonnet_stdlib::ContextInitializer::new(PathResolver::Absolute));
    builder.import_resolver(FileImportResolver::new(vec![PathBuf::from(JSONNET_DIR)]));
    builder.build()
}
