//! Production-capable jsonnet evaluation for assembling deployment config from
//! `build(layout, overrides)`. Shared by the deployment-config generator and the crate tests.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use apollo_config::dumping::SerializeConfig;
use apollo_config::{FIELD_SEPARATOR, IS_NONE_MARK};
use apollo_node_config::config_utils::config_to_preset;
use apollo_node_config::node_config::{SequencerNodeConfig, CONFIG_POINTERS};
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

/// Evaluates a standalone overlay jsonnet file to a nested JSON `overrides` object, rooted at the
/// overlay's OWN directory: the import resolver points at `overlay_path`'s parent, so the overlay
/// tree's relative imports (e.g. `import '_common.jsonnet'`) resolve locally and NEVER against the
/// apollo_deployments `JSONNET_DIR`. Each overlay layer is its own self-contained jsonnet tree; the
/// layers are merged later, in Rust (see `deep_merge_values`), so the two jsonnet import paths
/// never mix.
///
/// `overlay_path` is read as given (callers pass an absolute path, since the generator evaluates
/// overlays before switching to the project root). Returns the parse/eval error as a `String` with
/// the offending path for context.
pub fn eval_overlay_at_path(overlay_path: &Path) -> Result<Value, String> {
    let parent_dir = overlay_path
        .parent()
        .ok_or_else(|| format!("overlay path {overlay_path:?} has no parent directory"))?;
    let file_name = overlay_path
        .file_name()
        .ok_or_else(|| format!("overlay path {overlay_path:?} has no file name"))?
        .to_str()
        .ok_or_else(|| format!("overlay path {overlay_path:?} is not valid UTF-8"))?;

    let mut builder = State::builder();
    builder.context_initializer(jrsonnet_stdlib::ContextInitializer::new(PathResolver::Absolute));
    builder.import_resolver(FileImportResolver::new(vec![parent_dir.to_path_buf()]));
    let state = builder.build();

    let _guard = state.enter();
    let val = state
        .evaluate_snippet("overlay_entry.jsonnet", format!("import '{file_name}'"))
        .map_err(|error| format!("failed to evaluate overlay at {overlay_path:?}: {error}"))?;
    serde_json::to_value(&val)
        .map_err(|error| format!("overlay at {overlay_path:?} is not serializable: {error}"))
}

/// Recursively deep-merges `right` into `left`, with `right` winning on every leaf. When both sides
/// hold a JSON object at a key, their keys are merged recursively; for every other shape (scalars,
/// arrays, `null`, or an object overwriting a non-object and vice versa) `right`'s value completely
/// replaces `left`'s. In particular, `null` from `right` is a legitimate override value: it fully
/// replaces whatever `left` held at that key (null-wins semantics). Used to combine ordered overlay
/// layers into a single `overrides` object, later layers winning.
pub fn deep_merge_values(left: &mut Value, right: &Value) {
    match (left, right) {
        (Value::Object(left_map), Value::Object(right_map)) => {
            for (key, right_value) in right_map {
                match left_map.get_mut(key) {
                    Some(left_value) => deep_merge_values(left_value, right_value),
                    None => {
                        left_map.insert(key.clone(), right_value.clone());
                    }
                }
            }
        }
        (left_slot, right_value) => *left_slot = right_value.clone(),
    }
}

/// Evaluates `build(layout, overrides)`.
pub fn eval_build_with_overrides(layout: &str, overrides: &Value) -> Value {
    let overrides_literal = serde_json::to_string(overrides).expect("overrides is serializable");
    eval_build_with_expr(layout, &overrides_literal, None)
}

/// Evaluates only `build(layout, overrides)[service]`, so jsonnet forces just that one service's
/// config. A per-service deploy supplies only the keys its own components reference (e.g. the
/// gateway service's overrides carry no `committer_config`), and building just that service means
/// the other services' overrides are never accessed — building the whole map would force them all.
pub fn eval_build_service_with_overrides(layout: &str, overrides: &Value, service: &str) -> Value {
    let overrides_literal = serde_json::to_string(overrides).expect("overrides is serializable");
    eval_build_with_expr(layout, &overrides_literal, Some(service))
}

/// Renders one service's nested `build` output in the node-loadable flat dotted dump/preset format
/// — the format `load_and_process_config` ingests, and the same shape today's ConfigMap uses (so
/// the generator's output is a drop-in for the assembled config). Round-trips through
/// `SequencerNodeConfig`.
pub fn service_config_to_preset(service_config: &Value) -> Value {
    let parsed: SequencerNodeConfig = serde_json::from_value(service_config.clone())
        .expect("service config deserializes into SequencerNodeConfig");
    let mut preset = config_to_preset(&serde_json::json!(parsed.dump()));
    unresolve_pointers(&mut preset);
    preset
}

// TODO(Nimrod): Remove this (and its `service_config_to_preset` call) once the config-pointer
// mechanism is removed.
/// Jsonnet `build` writes config-pointer values at every pointing path (e.g. `chain_id` at each
/// `…chain_info.chain_id`), but the node's loader resolves each pointer from a single target key.
/// Rewrite the resolved form back to the target form (drop the pointing paths, emit the target) so
/// the generator's output is node-loadable.
fn unresolve_pointers(preset: &mut Value) {
    let map = preset.as_object_mut().expect("preset is a JSON object");
    for ((target, _param), pointing_paths) in CONFIG_POINTERS.iter() {
        // A pointing path inside a `None` option carries that field's (ignored) default rather than
        // the resolved value, so take the target value from an active (non-`None`) pointing path;
        // fall back to any path if all are under `None` (the value is unused anyway).
        let value = pointing_paths
            .iter()
            .filter(|path| !is_under_none_option(map, path))
            .find_map(|path| map.get(path.as_str()).cloned())
            .or_else(|| pointing_paths.iter().find_map(|path| map.get(path.as_str()).cloned()));
        for path in pointing_paths {
            map.remove(path.as_str());
        }
        if let Some(value) = value {
            map.insert(target.clone(), value);
        }
    }
}

// TODO(Nimrod): Remove with `unresolve_pointers` once the config-pointer mechanism is removed (P4).
/// True if `path` lies under an `Option` set to `None` (some ancestor has `<ancestor>.#is_none:
/// true`).
fn is_under_none_option(preset: &Map<String, Value>, path: &str) -> bool {
    let parts: Vec<&str> = path.split(FIELD_SEPARATOR).collect();
    (1..parts.len()).any(|end| {
        let ancestor = parts[..end].join(FIELD_SEPARATOR);
        preset.get(&format!("{ancestor}{FIELD_SEPARATOR}{IS_NONE_MARK}"))
            == Some(&Value::Bool(true))
    })
}

/// Evaluates a nested-overrides jsonnet/JSON file (relative to the jsonnet dir) to JSON.
pub fn eval_overrides_file(overrides_path: &str) -> Value {
    let state = jsonnet_state();
    let _guard = state.enter();
    let val = state
        .evaluate_snippet("overrides_entry.jsonnet", format!("import '{overrides_path}'"))
        .expect("overrides file failed to evaluate");
    serde_json::to_value(&val).expect("overrides is not serializable")
}

/// Evaluates `build(layout, <overrides_expr>)`, where `overrides_expr` is a jsonnet expression for
/// the overrides (a file `import` or an inlined JSON literal).
pub(crate) fn eval_build_with_expr(
    layout: &str,
    overrides_expr: &str,
    service: Option<&str>,
) -> Value {
    let state = jsonnet_state();
    let _guard = state.enter();
    let layout_literal = serde_json::to_string(layout).expect("layout is serializable");
    let index = match service {
        Some(service) => {
            format!("[{}]", serde_json::to_string(service).expect("service is serializable"))
        }
        None => String::new(),
    };
    let snippet =
        format!("(import 'lib/build.libsonnet').build({layout_literal}, {overrides_expr}){index}");
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

// TODO(Nimrod): Remove with `overrides_from_sequencer_config` once overrides are provided as a
// nested JSON object.
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
