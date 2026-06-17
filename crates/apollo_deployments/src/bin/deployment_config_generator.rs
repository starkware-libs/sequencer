//! Generates the non-secret deployment config for a layout from `build(layout, overrides)`.
//!
//! Reads the deploy's flat dotted `sequencerConfig` overrides from a JSON file, translates them
//! into the nested `overrides` shape, evaluates `build(layout, overrides)`, and prints each
//! service's config in the node-loadable flat dotted format (the whole service-keyed map, or a
//! single service with `--service`). Secrets are never produced here — they remain a separate
//! mounted file.

use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;

use apollo_deployments::jsonnet_generation::{
    eval_build_service_with_overrides,
    eval_build_with_overrides,
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
    /// Path to the flat dotted overrides JSON (the deploy's merged `sequencerConfig`).
    #[arg(long)]
    config_file: PathBuf,
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

    // Read the overrides relative to the invocation directory, before switching to the project root
    // (where `build`'s jsonnet imports resolve).
    let config_contents = std::fs::read_to_string(&args.config_file).unwrap_or_else(|error| {
        panic!("failed to read overrides file {:?}: {error}", args.config_file)
    });
    let flat_overrides: BTreeMap<String, Value> = serde_json::from_str(&config_contents)
        .expect("overrides file must be a flat dotted JSON object");

    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    let overrides = overrides_from_sequencer_config(&flat_overrides);

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
