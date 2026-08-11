use std::collections::HashSet;
use std::fs;

use apollo_consensus_orchestrator::metrics::{
    L2GasPriceClampBound,
    CONSENSUS_L2_GAS_PRICE,
    LABEL_L2_GAS_PRICE_CLAMP_BOUND,
};
use apollo_infra_utils::path::resolve_project_relative_path;
use apollo_metrics::metric_definitions::POD_LABEL_FILTER;
use apollo_metrics::metrics::MetricQueryName;
use serde_json::Value;

use super::{
    get_l2_gas_price_above_maximum_alert,
    get_l2_gas_price_at_minimum_alert,
    get_l2_gas_price_far_above_minimum_alert,
    get_snip35_fee_target_above_maximum_alert,
};
use crate::alert_definitions::get_apollo_alerts;
use crate::alerts::Alert;

/// The reading every metric these alerts watch produces on a freshly restarted pod: the Prometheus
/// exporter renders a registered-but-unset counter or gauge as 0.
const POST_RESTART_READING: f64 = 0.0;

fn alert_as_json(alert: &Alert) -> Value {
    serde_json::to_value(alert).expect("Alert should serialize.")
}

fn condition_is_satisfied_by(alert_json: &Value, reading: f64) -> bool {
    let evaluator = &alert_json["conditions"][0]["evaluator"];
    let comparison_value = evaluator["params"][0]
        .as_f64()
        .expect("Condition should compare against a concrete value.");
    match evaluator["type"].as_str().expect("Condition should name a comparison operator.") {
        "gt" => reading > comparison_value,
        "lt" => reading < comparison_value,
        operator => panic!("Unexpected comparison operator: {operator}."),
    }
}

#[test]
fn severity_and_pending_duration_match_the_paging_intent() {
    // p2 pages immediately: the ceiling makes the network unusable. The other three are bounded
    // elsewhere, so they can wait for a person to look.
    let expected_paging = [
        (get_l2_gas_price_at_minimum_alert(), "p3", "2m"),
        (get_l2_gas_price_above_maximum_alert(), "p2", "10m"),
        (get_l2_gas_price_far_above_minimum_alert(), "p4", "30m"),
        (get_snip35_fee_target_above_maximum_alert(), "p4", "10m"),
    ];

    for (alert, severity, pending_duration) in expected_paging {
        let alert_json = alert_as_json(&alert);
        let name = &alert_json["name"];
        assert_eq!(alert_json["severity"], severity, "Wrong severity for {name}.");
        assert_eq!(alert_json["for"], pending_duration, "Wrong pending duration for {name}.");
    }
}

#[test]
fn alerts_with_a_zero_baseline_stay_silent_on_a_post_restart_reading() {
    // These three read a counter increase or a 1/0 indicator, for which 0 is the true "nothing
    // observed" reading, so a pod restart must not satisfy their condition.
    for alert in [
        get_l2_gas_price_at_minimum_alert(),
        get_l2_gas_price_above_maximum_alert(),
        get_snip35_fee_target_above_maximum_alert(),
    ] {
        let alert_json = alert_as_json(&alert);
        assert!(
            !condition_is_satisfied_by(&alert_json, POST_RESTART_READING),
            "{} fires on a restarted pod that has decided no block yet.",
            alert_json["name"]
        );
    }
}

#[test]
fn price_threshold_alert_reads_the_gauge_unfiltered() {
    // `max()` ignores a restarting pod's registered 0; a `> 0` filter would empty the vector on a
    // cold start instead and page on NoData.
    let alert_json = alert_as_json(&get_l2_gas_price_far_above_minimum_alert());
    let expression = alert_json["expr"].as_str().expect("Expression should be a string.");
    // Alert expressions drop the pod filter, which Grafana does not substitute when evaluating.
    let gauge_query = CONSENSUS_L2_GAS_PRICE.get_name_with_filter().replace(POD_LABEL_FILTER, "");
    assert_eq!(
        expression,
        format!("max({gauge_query})"),
        "Expression must aggregate the gauge as-is, got: {expression}."
    );
}

#[test]
fn price_threshold_is_supplied_by_the_deployment() {
    // The minimum is per-deployment configuration, so a multiple of it cannot be a constant here.
    let alert = get_l2_gas_price_far_above_minimum_alert();
    let placeholder = "$$$consensus_l2_gas_price_far_above_minimum-comparison_value$$$";
    assert!(alert.get_placeholder_names().contains(placeholder));
    assert_eq!(alert_as_json(&alert)["conditions"][0]["evaluator"]["params"][0], placeholder);
}

#[test]
fn price_threshold_placeholder_is_filled_in_by_the_testnet_overrides() {
    // The placeholder carries no default, so an override file that spells its key differently
    // leaves the deployed rule without a threshold.
    let overrides_path = resolve_project_relative_path(
        "deployments/monitoring/examples/config/alert_overrides_testnet.yaml",
    )
    .expect("Testnet alert overrides should exist.");
    let overrides = fs::read_to_string(&overrides_path).expect("Overrides should be readable.");

    let placeholder = get_l2_gas_price_far_above_minimum_alert()
        .get_placeholder_names()
        .iter()
        .find(|name| name.contains("comparison_value"))
        .expect("Alert should carry a comparison-value placeholder.")
        .clone();
    // Placeholders are wrapped in `$$$`; the override file keys on the bare name.
    let override_key = placeholder.trim_matches('$');

    assert!(
        overrides.lines().any(|line| line.starts_with(&format!("{override_key}:"))),
        "{} is missing the key {override_key}.",
        overrides_path.display()
    );
}

#[test]
fn ceiling_alert_selects_the_maximum_clamp_bound() {
    // The counter is incremented from the node's own comparison against the ceiling, so the alert
    // carries no threshold of its own that could drift away from the enforced bound.
    let alert_json = alert_as_json(&get_l2_gas_price_above_maximum_alert());
    let expression = alert_json["expr"].as_str().expect("Expression should be a string.");
    let maximum_bound: &str = L2GasPriceClampBound::Maximum.into();
    assert!(
        expression.contains(&format!("{LABEL_L2_GAS_PRICE_CLAMP_BOUND}=\"{maximum_bound}\"")),
        "Expression must select the maximum bound, got: {expression}."
    );
    assert_eq!(alert_json["conditions"][0]["evaluator"]["params"][0], 0.0);
}

#[test]
fn all_l2_gas_price_alerts_are_registered() {
    let registered_alerts = serde_json::to_value(get_apollo_alerts())
        .expect("Alerts should serialize.")["alerts"]
        .as_array()
        .expect("Alerts should serialize to an array.")
        .iter()
        .map(|alert| alert["name"].as_str().expect("Alert should have a name.").to_string())
        .collect::<HashSet<_>>();

    for alert in [
        get_l2_gas_price_at_minimum_alert(),
        get_l2_gas_price_above_maximum_alert(),
        get_l2_gas_price_far_above_minimum_alert(),
        get_snip35_fee_target_above_maximum_alert(),
    ] {
        let alert_json = alert_as_json(&alert);
        let name = alert_json["name"].as_str().expect("Alert should have a name.");
        assert!(registered_alerts.contains(name), "{name} is defined but never registered.");
    }
}
