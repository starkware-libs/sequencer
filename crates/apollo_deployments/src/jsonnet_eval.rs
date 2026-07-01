//! The jsonnet `build()` evaluator, exposed to other crates (the integration-test harness) via the
//! `testing` feature so they can source node configs from jsonnet without pulling jrsonnet into the
//! default/production dependency graph. The test-only parity/applicative helpers live in the
//! sibling `jsonnet_tests` module (compiled only under `test`).

use std::path::PathBuf;

use jrsonnet_evaluator::trace::PathResolver;
use jrsonnet_evaluator::{FileImportResolver, State};
use serde_json::Value;

const JSONNET_DIR: &str = "crates/apollo_deployments/jsonnet";

/// Evaluates `build(layout, <params>)` and returns the per-service config map: service name → that
/// service's fully-assembled `SequencerNodeConfig` as JSON. `params` is a jsonnet expression
/// yielding the `{ chain_params, node_params, replacers }` object — e.g. an inline object literal
/// or `"import 'testing/integration_node.jsonnet'"`. Paths in `params` resolve relative to the
/// jsonnet dir (the evaluator's import root).
pub fn build_service_configs(layout: &str, params: &str) -> serde_json::Map<String, Value> {
    let layout_literal = serde_json::to_string(layout).expect("layout name serializes to JSON");
    let built = eval_jsonnet(
        "build",
        format!("(import 'lib/build.libsonnet').build({layout_literal}, {params})"),
    );
    match built {
        Value::Object(services) => services,
        other => panic!("build({layout}) did not produce a service-map object: {other}"),
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
