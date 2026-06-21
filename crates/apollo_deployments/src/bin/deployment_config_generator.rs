//! Generates the non-secret deployment config for a layout from `build(layout, overrides)`.
//!
//! Override input comes in one of two mutually exclusive modes:
//! - `--config_file`: the deploy's flat dotted `sequencerConfig` overrides as a single JSON file,
//!   translated into the nested `overrides` shape.
//! - `--overlay` (repeatable): one or more standalone jsonnet "overlay" layers, given in precedence
//!   order (later layers win). Each overlay is evaluated rooted at its OWN directory (so its tree's
//!   relative imports resolve locally), then the evaluated objects are deep-merged in Rust into a
//!   single nested `overrides`. The overlay trees never share an import path with the
//!   apollo_deployments jsonnet dir.
//!
//! Either way, the result evaluates `build(layout, overrides)` and prints each service's config in
//! the node-loadable flat dotted format (the whole service-keyed map, or a single service with
//! `--service`). Secrets are never produced here — they remain a separate mounted file.

use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;

use apollo_deployments::jsonnet_generation::{
    deep_merge_values,
    eval_build_service_with_overrides,
    eval_build_with_overrides,
    eval_overlay_at_path,
    overrides_from_sequencer_config,
    service_config_to_preset,
};
use apollo_deployments::service::NodeType;
use apollo_infra_utils::path::resolve_project_relative_path;
use clap::Parser;
use serde_json::Value;
use strum::IntoEnumIterator;

#[derive(Parser)]
#[command(about = "Generate non-secret sequencer deployment config via build(layout, overrides).")]
struct Args {
    /// Deployment layout: `consolidated`, `hybrid`, or `distributed`.
    #[arg(long)]
    layout: String,
    /// Path to the flat dotted overrides JSON (the deploy's merged `sequencerConfig`). Mutually
    /// exclusive with `--overlay`.
    #[arg(long)]
    config_file: Option<PathBuf>,
    /// Path to an overlay jsonnet layer. Repeat in precedence order (later layers win); the
    /// evaluated layers are deep-merged into the nested `overrides`. Mutually exclusive with
    /// `--config_file`.
    #[arg(long, value_name = "PATH")]
    overlay: Vec<PathBuf>,
    /// When set, print only this service's config instead of the whole service-keyed map.
    #[arg(long)]
    service: Option<String>,
}

fn main() {
    let args = Args::parse();

    let layouts: Vec<String> = NodeType::iter().map(|node_type| node_type.to_string()).collect();
    assert!(
        layouts.contains(&args.layout),
        "unknown layout '{}'; expected one of {layouts:?}",
        args.layout
    );

    // Resolve the overrides relative to the invocation directory, before switching to the project
    // root (where `build`'s jsonnet imports resolve). Exactly one input mode must be supplied:
    // either a single flat `--config_file` or one-or-more jsonnet `--overlay` layers.
    let overrides = match (&args.config_file, args.overlay.as_slice()) {
        (Some(_), [_, ..]) => panic!("Must specify either --config_file or --overlay (not both)"),
        (None, []) => panic!("Must specify at least one of --config_file or --overlay"),
        (Some(config_file), []) => {
            let config_contents = std::fs::read_to_string(config_file).unwrap_or_else(|error| {
                panic!("failed to read overrides file {config_file:?}: {error}")
            });
            let flat_overrides: BTreeMap<String, Value> = serde_json::from_str(&config_contents)
                .expect("overrides file must be a flat dotted JSON object");

            env::set_current_dir(resolve_project_relative_path("").unwrap())
                .expect("Couldn't set working dir.");
            overrides_from_sequencer_config(&flat_overrides)
        }
        (None, overlays) => {
            // Evaluate each overlay rooted at its own directory before the chdir, resolving every
            // path against the invocation dir so each tree's relative imports stay local.
            // Deep-merge the evaluated layers in order (later overlays win) into a
            // single nested `overrides`.
            let overlay_layers: Vec<Value> = overlays
                .iter()
                .map(|overlay| {
                    let overlay_path = overlay.canonicalize().unwrap_or_else(|error| {
                        panic!("failed to resolve overlay path {overlay:?}: {error}")
                    });
                    eval_overlay_at_path(&overlay_path)
                        .unwrap_or_else(|error| panic!("overlay {overlay:?} failed: {error}"))
                })
                .collect();
            let mut merged = Value::Object(serde_json::Map::new());
            for layer in &overlay_layers {
                deep_merge_values(&mut merged, layer);
            }

            env::set_current_dir(resolve_project_relative_path("").unwrap())
                .expect("Couldn't set working dir.");
            merged
        }
    };

    let output = match &args.service {
        // Build only the requested service: a per-service deploy supplies just that service's
        // overrides, so the other services' (absent) override keys must never be forced.
        Some(service) => {
            let service_config =
                eval_build_service_with_overrides(&args.layout, &overrides, service);
            service_config_to_preset(&service_config)
        }
        None => {
            let service_configs = eval_build_with_overrides(&args.layout, &overrides);
            let services =
                service_configs.as_object().expect("build result is a service-keyed object");
            Value::Object(
                services
                    .iter()
                    .map(|(name, config)| (name.clone(), service_config_to_preset(config)))
                    .collect(),
            )
        }
    };

    println!("{}", serde_json::to_string_pretty(&output).expect("config is serializable"));
}
