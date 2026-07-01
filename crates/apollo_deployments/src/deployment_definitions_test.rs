use std::collections::{BTreeSet, HashSet};
use std::env;
use std::fs::File;

use apollo_infra_utils::path::resolve_project_relative_path;
use apollo_node_config::config_utils::private_parameters;
use serde_json::{to_value, Map};
use strum::IntoEnumIterator;

use crate::deployment_definitions::ComponentConfigInService;
use crate::deployments::consolidated::ConsolidatedNodeServiceName;
use crate::deployments::distributed::DistributedNodeServiceName;
use crate::deployments::hybrid::HybridNodeServiceName;
use crate::jsonnet::{assert_build_deserializes, assert_infra_matches_rust};
use crate::service::NodeType;
use crate::test_utils::SecretsConfigOverride;

const SECRETS_FOR_TESTING_ENV_PATH: &str =
    "crates/apollo_deployments/resources/testing_secrets.json";

/// Verifies the jsonnet hybrid infra config matches the Rust deployment definitions (hybrid.rs).
#[test]
fn hybrid_infra_matches_rust() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    assert_infra_matches_rust::<HybridNodeServiceName>();
}

/// Verifies the jsonnet consolidated infra config matches the Rust deployment definitions
/// (consolidated.rs).
#[test]
fn consolidated_infra_matches_rust() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    assert_infra_matches_rust::<ConsolidatedNodeServiceName>();
}

/// Verifies the jsonnet distributed infra config matches the Rust deployment definitions
/// (distributed.rs).
#[test]
fn distributed_infra_matches_rust() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    assert_infra_matches_rust::<DistributedNodeServiceName>();
}

/// Verifies build('consolidated', overrides) deserializes into SequencerNodeConfig per service.
#[test]
fn build_consolidated_deserializes_into_node_config() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    assert_build_deserializes::<ConsolidatedNodeServiceName>();
}

/// Verifies build('hybrid', overrides) deserializes into SequencerNodeConfig per service.
#[test]
fn build_hybrid_deserializes_into_node_config() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    assert_build_deserializes::<HybridNodeServiceName>();
}

/// Verifies build('distributed', overrides) deserializes into SequencerNodeConfig per service.
#[test]
fn build_distributed_deserializes_into_node_config() {
    env::set_current_dir(resolve_project_relative_path("").unwrap())
        .expect("Couldn't set working dir.");

    assert_build_deserializes::<DistributedNodeServiceName>();
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
