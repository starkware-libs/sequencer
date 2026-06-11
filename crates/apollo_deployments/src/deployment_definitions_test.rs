use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::env;
use std::fs::File;

use apollo_infra_utils::dumping::serialize_to_file;
use apollo_infra_utils::path::resolve_project_relative_path;
use apollo_node_config::config_utils::private_parameters;
use serde_json::{to_value, Map, Value};
use strum::IntoEnumIterator;
use tempfile::NamedTempFile;

use crate::deployment_definitions::ComponentConfigInService;
use crate::jsonnet::test_hybrid_infra_matches_rust;
use crate::service::{NodeType, KEYS_TO_BE_REPLACED};
use crate::test_utils::SecretsConfigOverride;

const SECRETS_FOR_TESTING_ENV_PATH: &str =
    "crates/apollo_deployments/resources/testing_secrets.json";

const APPLICATIVE_CONFIG_JSONNET_PATH: &str =
    "crates/apollo_deployments/jsonnet/lib/applicative_config.libsonnet";

/// Verifies the jsonnet hybrid infra config matches the Rust deployment definitions (hybrid.rs).
#[test]
fn hybrid_infra_matches_rust() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    test_hybrid_infra_matches_rust();
}

/// Test that the deployment file is up to date.
#[test]
fn deployment_files_are_up_to_date() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    for node_type in NodeType::iter() {
        node_type.test_dump_service_component_configs(None);
        for node_service in node_type.all_service_names() {
            node_service.test_dump_node_service_replacer_app_config_files();
        }
    }
}

/// Test that the deployment file is up to date.
#[test]
fn replacer_config_entries_are_in_config() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    for node_type in NodeType::iter() {
        node_type.test_all_replacers_are_accounted_for();
    }
}

/// Collects the overrided keys from the applicative-config jsonnet, i.e all keys that are
/// referenced as `overrides.<path>`.
fn override_schema_keys(jsonnet_source: &str) -> HashSet<String> {
    const MARKER: &str = "overrides.";
    let is_key_char = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.';
    jsonnet_source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        // The text following the (single) `overrides.` occurrence on the line, if any.
        .filter_map(|line| line.split_once(MARKER))
        // A key is the leading run of identifier/dot characters following the marker.
        .filter_map(|(_, after_marker)| {
            let key = after_marker.split(|c| !is_key_char(c)).next().unwrap_or("");
            let key = key.trim_end_matches('.');
            (!key.is_empty()).then(|| key.to_string())
        })
        .collect()
}

/// Enforces the invariant tying the Rust replacer set to the jsonnet override schema, both ways:
/// (1) every key `f` in `KEYS_TO_BE_REPLACED` is covered by exactly one override key `t` that is a
/// path-prefix of it (`t == f`, or `f` starts with `t.`); and (2) every `overrides.<path>` the
/// applicative reads corresponds to some `KEYS_TO_BE_REPLACED` entry (one is a path-prefix of the
/// other) — so no non-overridable (constant) value is read from `overrides`.
#[test]
fn keys_to_be_replaced_are_covered_by_override_schema() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    let jsonnet_source = std::fs::read_to_string(APPLICATIVE_CONFIG_JSONNET_PATH)
        .expect("Failed to read applicative_config.libsonnet");
    let override_keys = override_schema_keys(&jsonnet_source);

    let is_path_prefix = |prefix: &str, key: &str| {
        key == prefix || key.strip_prefix(prefix).is_some_and(|rest| rest.starts_with('.'))
    };

    let mut uncovered_keys: Vec<&str> = Vec::new();
    let mut ambiguous_keys: Vec<(&str, Vec<String>)> = Vec::new();
    for replacer_key in KEYS_TO_BE_REPLACED.iter().copied() {
        let covering_overrides: Vec<String> = override_keys
            .iter()
            .filter(|override_key| is_path_prefix(override_key, replacer_key))
            .cloned()
            .collect();
        match covering_overrides.len() {
            1 => {}                                 // Covered by exactly one override.
            0 => uncovered_keys.push(replacer_key), // Not covered by any override.
            _ => ambiguous_keys.push((replacer_key, covering_overrides)), /* Covered by more than
                                                      * one override. */
        }
    }

    // Reverse direction: every `overrides.<path>` the applicative reads must correspond to an
    // overridable key — otherwise a constant is being read from `overrides` per-environment.
    let non_overridable_refs: Vec<&str> = override_keys
        .iter()
        .filter(|override_key| {
            !KEYS_TO_BE_REPLACED
                .iter()
                .any(|key| is_path_prefix(override_key, key) || is_path_prefix(key, override_key))
        })
        .map(String::as_str)
        .collect();

    assert!(
        uncovered_keys.is_empty() && ambiguous_keys.is_empty() && non_overridable_refs.is_empty(),
        "The applicative-config override schema drifted from KEYS_TO_BE_REPLACED.\nReplacer keys \
         with no covering override (add an `overrides.<path>` that is a prefix): \
         {uncovered_keys:#?}\nReplacer keys covered by more than one override (the override \
         schema is ambiguous): {ambiguous_keys:#?}\n`overrides.<path>` references not in \
         KEYS_TO_BE_REPLACED (make them constants, or add the key to KEYS_TO_BE_REPLACED): \
         {non_overridable_refs:#?}"
    );
}

// TODO(Tsabary): consider adding a test that loads a config and validates it; the challenge will be
// to replace the values in a meaningful manner. Consider using the system test yaml files for that.

// Test that each there are no duplicate config entries.
#[test]
fn duplicate_config_entries() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    // Create a dummy secrets value and dump it as a config file.
    let secrets_file = NamedTempFile::new().unwrap();
    let secrets_file_path = secrets_file.path().to_str().unwrap();
    let secrets_config_override = SecretsConfigOverride::default();
    serialize_to_file(&to_value(&secrets_config_override).unwrap(), secrets_file_path);

    for node_type in NodeType::iter() {
        for node_service in node_type.all_service_names() {
            let deployment_file_path = node_service.replacer_deployment_file_path();
            let deployment_file = File::open(deployment_file_path).unwrap();

            let mut application_config_files: Vec<String> =
                serde_json::from_reader(deployment_file)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                    .unwrap();

            // Add the secrets config file path to the config load command.
            application_config_files.push(secrets_file_path.to_string());

            let mut key_to_files: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            for application_config_file in &application_config_files {
                let file = File::open(application_config_file).unwrap();
                let json_map: Map<String, Value> = serde_json::from_reader(file)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                    .unwrap();

                for key in json_map.keys() {
                    key_to_files
                        .entry(key.clone())
                        .or_default()
                        .insert(application_config_file.to_string());
                }
            }

            // Report duplicated keys
            let mut has_duplicates = false;
            for (key, files) in &key_to_files {
                if files.len() > 1 {
                    has_duplicates = true;
                    println!(
                        "For node type {node_type} the key '{key}' was found in files: {files:?}"
                    );
                }
            }
            assert!(!has_duplicates, "Found duplicate keys in service config files.");
        }
    }
}

/// Test that the private values in the apollo node config schema match the secrets config override
/// schema.
#[test]
fn secrets_config_and_private_parameters_config_schema_compatibility() {
    let secrets_config_override = SecretsConfigOverride::default();
    let secrets_provided_by_config = to_value(&secrets_config_override)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let secrets_required_by_schema = private_parameters();

    let only_in_config: BTreeSet<_> =
        secrets_provided_by_config.difference(&secrets_required_by_schema).collect();
    let only_in_schema: BTreeSet<_> =
        secrets_required_by_schema.difference(&secrets_provided_by_config).collect();

    if !(only_in_config.is_empty() && only_in_schema.is_empty()) {
        panic!(
            "Secrets config override schema mismatch:\nSecrets provided by config: \
             {secrets_provided_by_config:?}\nSecrets required by schema: \
             {secrets_required_by_schema:?}\nOnly in config: {only_in_config:?}\nOnly in schema: \
             {only_in_schema:?}"
        );
    }

    let secrets_for_testing_file_path =
        &resolve_project_relative_path(SECRETS_FOR_TESTING_ENV_PATH).unwrap();
    let secrets_for_testing: BTreeSet<_> = (serde_json::from_reader::<_, Map<String, _>>(
        File::open(secrets_for_testing_file_path).unwrap(),
    )
    .unwrap())
    .keys()
    .cloned()
    .collect();

    let only_in_secrets_for_testing: BTreeSet<_> =
        secrets_for_testing.difference(&secrets_required_by_schema).collect();
    let only_in_schema: BTreeSet<_> =
        secrets_required_by_schema.difference(&secrets_for_testing).collect();

    if !(only_in_secrets_for_testing.is_empty() && only_in_schema.is_empty()) {
        panic!(
            "Secrets for testing and schema mismatch:\nSecrets for testing: \
             {secrets_provided_by_config:?}\nSecrets required by schema: \
             {secrets_required_by_schema:?}\nOnly in testing: \
             {only_in_secrets_for_testing:?}\nOnly in schema: {only_in_schema:?}"
        );
    }
}

#[test]
fn l1_components_state_consistency() {
    for node_type in NodeType::iter() {
        let all_components: HashSet<ComponentConfigInService> = node_type
            .all_service_names()
            .iter()
            .flat_map(|node_service| node_service.get_components_in_service())
            .collect();

        let l1_gas_price_provider_indicator =
            all_components.contains(&ComponentConfigInService::L1GasPriceProvider);
        let l1_events_provider_indicator =
            all_components.contains(&ComponentConfigInService::L1EventsProvider);
        let l1_gas_price_scraper_indicator =
            all_components.contains(&ComponentConfigInService::L1GasPriceScraper);
        let l1_scraper_indicator =
            all_components.contains(&ComponentConfigInService::L1EventsScraper);

        assert_eq!(
            l1_gas_price_provider_indicator, l1_gas_price_scraper_indicator,
            "L1 gas price provider and scraper should either be both enabled or both disabled."
        );
        assert_eq!(
            l1_events_provider_indicator, l1_scraper_indicator,
            "L1 provider and scraper should either be both enabled or both disabled."
        );
    }
}
