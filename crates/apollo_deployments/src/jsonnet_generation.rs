//! Production-capable jsonnet evaluation for assembling deployment config from
//! `build(layout, overrides)`. Shared by the deployment-config generator and the crate tests.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use apollo_config::{FIELD_SEPARATOR, IS_NONE_MARK};
use jrsonnet_evaluator::trace::PathResolver;
use jrsonnet_evaluator::{FileImportResolver, State};
use serde_json::{Map, Value};

use crate::utils::is_path_prefix;

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

// TODO(Nimrod): Remove this once overrides are provided with a nested JSON object.
/// Translates a flat dotted config map into a nested JSON object, folding the `#is_none` dump
/// markers along the way.
pub fn overrides_from_sequencer_config(flat: &BTreeMap<String, Value>) -> Value {
    let is_none_suffix = format!("{FIELD_SEPARATOR}{IS_NONE_MARK}");
    let none_paths: BTreeSet<&str> = flat
        .iter()
        .filter(|(_, value)| *value == &Value::Bool(true))
        .filter_map(|(key, _)| key.strip_suffix(&is_none_suffix))
        .collect();
    // `#is_none: false` marks an active (`Some`) option; its prefix must stay present so the
    // consumer sees `Some`, even when the option carries no sub-field overrides (materialized
    // as `{}` below).
    let some_paths: BTreeSet<&str> = flat
        .iter()
        .filter(|(_, value)| *value == &Value::Bool(false))
        .filter_map(|(key, _)| key.strip_suffix(&is_none_suffix))
        .collect();

    let mut leaves: BTreeMap<&str, Value> = BTreeMap::new();
    for (key, value) in flat {
        // `#is_none` markers are folded into the `null`/`{}` below, never emitted themselves.
        if key.ends_with(&is_none_suffix) {
            continue;
        }
        // Drop the placeholder value and children of a `None` option (replaced by `null` below).
        if none_paths.iter().any(|none_path| is_path_prefix(none_path, key)) {
            continue;
        }
        leaves.insert(key, value.clone());
    }
    for none_path in &none_paths {
        leaves.insert(*none_path, Value::Null);
    }
    // An active option with no sub-field overrides (and not also marked `None`) becomes an empty
    // object, so it deserializes to `Some(default)` rather than being an absent field.
    for some_path in some_paths {
        if !none_paths.contains(some_path)
            && !leaves.keys().any(|leaf| is_path_prefix(some_path, leaf))
        {
            leaves.insert(some_path, Value::Object(Map::new()));
        }
    }

    let mut root = Map::new();
    for (path, value) in leaves {
        insert_nested(&mut root, path, value);
    }
    Value::Object(root)
}

/// Inserts `value` at the dot-separated `path` within `root`, creating intermediate objects.
fn insert_nested(root: &mut Map<String, Value>, path: &str, value: Value) {
    let mut current = root;
    let mut parts = path.split(FIELD_SEPARATOR).peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            current.insert(part.to_owned(), value);
            return;
        }
        current = current
            .entry(part.to_owned())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .unwrap_or_else(|| panic!("override path '{path}' conflicts with a non-object value"));
    }
}
