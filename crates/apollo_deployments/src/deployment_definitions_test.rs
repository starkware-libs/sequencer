use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::File;

use alloy::primitives::map::HashSet;
use apollo_infra_utils::dumping::serialize_to_file;
use apollo_infra_utils::path::resolve_project_relative_path;
use apollo_node_config::config_utils::private_parameters;
use serde_json::{to_value, Map, Value};
use strum::IntoEnumIterator;
use tempfile::NamedTempFile;

use crate::deployment_definitions::ComponentConfigInService;
use crate::service::NodeType;
use crate::test_utils::SecretsConfigOverride;

const SECRETS_FOR_TESTING_ENV_PATH: &str =
    "crates/apollo_deployments/resources/testing_secrets.json";

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
        let l1_provider_indicator = all_components.contains(&ComponentConfigInService::L1Provider);
        let l1_gas_price_scraper_indicator =
            all_components.contains(&ComponentConfigInService::L1GasPriceScraper);
        let l1_scraper_indicator = all_components.contains(&ComponentConfigInService::L1Scraper);

        assert_eq!(
            l1_gas_price_provider_indicator, l1_gas_price_scraper_indicator,
            "L1 gas price provider and scraper should either be both enabled or both disabled."
        );
        assert_eq!(
            l1_provider_indicator, l1_scraper_indicator,
            "L1 provider and scraper should either be both enabled or both disabled."
        );
    }
}
