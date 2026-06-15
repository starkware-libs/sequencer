//! Production-capable jsonnet evaluation for assembling deployment config from
//! `build(layout, overrides)`. Shared by the deployment-config generator and the crate tests.

use std::path::PathBuf;

use jrsonnet_evaluator::trace::PathResolver;
use jrsonnet_evaluator::{FileImportResolver, State};
use serde_json::Value;

const JSONNET_DIR: &str = "crates/apollo_deployments/jsonnet";

/// A jrsonnet evaluator with the stdlib installed and file imports resolved relative to the jsonnet
/// dir (so the libraries' `std.*` calls and relative `import`s work).
pub(crate) fn jsonnet_state() -> State {
    let mut builder = State::builder();
    builder.context_initializer(jrsonnet_stdlib::ContextInitializer::new(PathResolver::Absolute));
    builder.import_resolver(FileImportResolver::new(vec![PathBuf::from(JSONNET_DIR)]));
    builder.build()
}

/// Evaluates `build(layout, overrides)` and returns its JSON: a map from service name to that
/// service's fully-assembled config.
pub fn eval_build(layout: &str, overrides: &str) -> Value {
    let state = jsonnet_state();
    let _guard = state.enter();
    let layout_literal = serde_json::to_string(layout).expect("layout is serializable");
    let snippet =
        format!("(import 'lib/build.libsonnet').build({layout_literal}, import '{overrides}')");
    let val = state
        .evaluate_snippet("build_entry.jsonnet", snippet)
        .expect("build.libsonnet failed to evaluate");
    serde_json::to_value(&val).expect("build result is not serializable")
}
