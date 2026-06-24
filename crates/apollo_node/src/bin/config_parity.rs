//! Local preset/native config-load parity check (uncommitted dev tool).
//!
//! Loads two config artifacts produced by the deployment pipeline for the same node:
//!   - a `preset` artifact (flat dotted-key) via `--config_format preset`
//!   - a `native` artifact (nested) via `--config_format native`
//! Both are loaded with the SAME secrets file as a second `--config_file` so secret
//! values match by construction, then the deserialized `SequencerNodeConfig`s are
//! compared via the derived `PartialEq`.
//!
//! NOTE: this uses `SequencerNodeConfig::load_and_process` (deserialize only), NOT
//! `load_and_validate_config`. `validate_node_config` additionally checks runtime
//! environment invariants (e.g. that `/data/*` paths exist and that component URLs / k8s
//! service DNS resolve) that only hold inside the pod and are irrelevant to deserialization
//! parity; running them locally fails on every config. We compare what the node deserializes.
//!
//! Exit codes: 0 = PARITY: PASS, 1 = configs differ (prints a JSON diff), 2 = a load/
//! deserialize error occurred.

use std::process::exit;

use apollo_node_config::node_config::SequencerNodeConfig;
use serde_json::Value;

struct Args {
    preset_file: String,
    native_file: String,
    secrets_file: String,
}

fn parse_args() -> Args {
    let mut preset_file = None;
    let mut native_file = None;
    let mut secrets_file = None;

    let mut raw_args = std::env::args().skip(1);
    while let Some(flag) = raw_args.next() {
        let value = match raw_args.next() {
            Some(value) => value,
            None => {
                eprintln!("Missing value for argument {flag}");
                exit(2);
            }
        };
        match flag.as_str() {
            "--preset-file" => preset_file = Some(value),
            "--native-file" => native_file = Some(value),
            "--secrets-file" => secrets_file = Some(value),
            other => {
                eprintln!("Unknown argument: {other}");
                eprintln!(
                    "Usage: config_parity --preset-file P --native-file N --secrets-file S"
                );
                exit(2);
            }
        }
    }

    match (preset_file, native_file, secrets_file) {
        (Some(preset_file), Some(native_file), Some(secrets_file)) => {
            Args { preset_file, native_file, secrets_file }
        }
        _ => {
            eprintln!(
                "Usage: config_parity --preset-file P --native-file N --secrets-file S"
            );
            exit(2);
        }
    }
}

fn load_preset(preset_file: &str, secrets_file: &str) -> SequencerNodeConfig {
    let args = vec![
        "config_parity",
        "--config_format",
        "preset",
        "--config_file",
        preset_file,
        "--config_file",
        secrets_file,
    ]
    .into_iter()
    .map(String::from)
    .collect();
    match SequencerNodeConfig::load_and_process(args) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Failed to load PRESET config from {preset_file}: {error}");
            exit(2);
        }
    }
}

fn load_native(native_file: &str, secrets_file: &str) -> SequencerNodeConfig {
    // Native requires the first `--config_file` to be the nested base; later files are
    // flat secret overrides.
    let args = vec![
        "config_parity",
        "--config_format",
        "native",
        "--config_file",
        native_file,
        "--config_file",
        secrets_file,
    ]
    .into_iter()
    .map(String::from)
    .collect();
    match SequencerNodeConfig::load_and_process(args) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("Failed to load NATIVE config from {native_file}: {error}");
            exit(2);
        }
    }
}

/// Prints the top-level keys whose JSON values differ between the two configs.
fn print_diff(preset_config: &SequencerNodeConfig, native_config: &SequencerNodeConfig) {
    let preset_value = serde_json::to_value(preset_config)
        .expect("SequencerNodeConfig should serialize to JSON");
    let native_value = serde_json::to_value(native_config)
        .expect("SequencerNodeConfig should serialize to JSON");

    let empty = serde_json::Map::new();
    let preset_object = preset_value.as_object().unwrap_or(&empty);
    let native_object = native_value.as_object().unwrap_or(&empty);

    let mut all_keys: Vec<&String> =
        preset_object.keys().chain(native_object.keys()).collect();
    all_keys.sort_unstable();
    all_keys.dedup();

    let null = Value::Null;
    for key in all_keys {
        let preset_field = preset_object.get(key).unwrap_or(&null);
        let native_field = native_object.get(key).unwrap_or(&null);
        if preset_field != native_field {
            eprintln!("--- DIFF in top-level field `{key}` ---");
            eprintln!(
                "  preset: {}",
                serde_json::to_string(preset_field).unwrap_or_default()
            );
            eprintln!(
                "  native: {}",
                serde_json::to_string(native_field).unwrap_or_default()
            );
        }
    }
}

fn main() {
    let args = parse_args();
    let preset_config = load_preset(&args.preset_file, &args.secrets_file);
    let native_config = load_native(&args.native_file, &args.secrets_file);

    if preset_config == native_config {
        println!("PARITY: PASS");
        exit(0);
    }

    eprintln!("PARITY: FAIL - loaded SequencerNodeConfig values differ");
    print_diff(&preset_config, &native_config);
    exit(1);
}
