use std::collections::HashSet;

use serde_json::Value;

use super::{
    get_chainlink_oracle_rate_out_of_bounds_alert,
    get_chainlink_oracle_stale_feed_alert,
    GUARD_PENDING_DURATION,
    GUARD_SAMPLING_WINDOW,
};
use crate::alert_definitions::get_apollo_alerts;
use crate::alerts::Alert;

/// The reading a registered but never incremented counter produces, which is what a freshly
/// restarted pod exports.
const POST_RESTART_READING: f64 = 0.0;

fn alert_as_json(alert: &Alert) -> Value {
    serde_json::to_value(alert).expect("Alert should serialize.")
}

#[test]
fn guard_alerts_stay_silent_on_a_post_restart_reading() {
    for alert in
        [get_chainlink_oracle_stale_feed_alert(), get_chainlink_oracle_rate_out_of_bounds_alert()]
    {
        let alert_json = alert_as_json(&alert);
        let evaluator = &alert_json["conditions"][0]["evaluator"];
        let comparison_value =
            evaluator["params"][0].as_f64().expect("Condition should compare against a number.");
        assert_eq!(
            evaluator["type"], "gt",
            "{} should fire above a threshold.",
            alert_json["name"]
        );
        assert!(
            POST_RESTART_READING <= comparison_value,
            "{} fires on a pod that has rejected nothing yet.",
            alert_json["name"]
        );
    }
}

#[test]
fn guard_alerts_evaluate_to_zero_before_the_first_trip() {
    // The fallback keeps the value a real zero, not the no-data state, until a guard first trips.
    for alert in
        [get_chainlink_oracle_stale_feed_alert(), get_chainlink_oracle_rate_out_of_bounds_alert()]
    {
        let alert_json = alert_as_json(&alert);
        let expression = alert_json["expr"].as_str().expect("Expression should be a string.");
        assert!(
            expression.ends_with("or vector(0)"),
            "{} must stay defined before its counter exists, got: {expression}.",
            alert_json["name"]
        );
    }
}

#[test]
fn a_lone_guard_trip_cannot_page() {
    // The condition holds true for one sampling window after a trip, so the pending duration must
    // exceed that window or a single trip is enough to page.
    let sampling_window_minutes = GUARD_SAMPLING_WINDOW
        .strip_suffix('m')
        .and_then(|minutes| minutes.parse::<u32>().ok())
        .expect("Sampling window should be expressed in whole minutes.");
    let pending_minutes = GUARD_PENDING_DURATION
        .strip_suffix('m')
        .and_then(|minutes| minutes.parse::<u32>().ok())
        .expect("Pending duration should be expressed in whole minutes.");

    assert!(
        sampling_window_minutes < pending_minutes,
        "sampling window {GUARD_SAMPLING_WINDOW} must stay under the pending duration \
         {GUARD_PENDING_DURATION}, or one trip is enough to page"
    );
}

#[test]
fn the_bounds_guard_outranks_the_freshness_guard() {
    // See `get_chainlink_oracle_rate_out_of_bounds_alert` for why this guard outranks the
    // freshness guard.
    assert_eq!(alert_as_json(&get_chainlink_oracle_stale_feed_alert())["severity"], "p4");
    assert_eq!(alert_as_json(&get_chainlink_oracle_rate_out_of_bounds_alert())["severity"], "p3");
}

#[test]
fn all_chainlink_oracle_alerts_are_registered() {
    let registered_alerts = serde_json::to_value(get_apollo_alerts())
        .expect("Alerts should serialize.")["alerts"]
        .as_array()
        .expect("Alerts should serialize to an array.")
        .iter()
        .map(|alert| alert["name"].as_str().expect("Alert should have a name.").to_string())
        .collect::<HashSet<_>>();

    for alert in
        [get_chainlink_oracle_stale_feed_alert(), get_chainlink_oracle_rate_out_of_bounds_alert()]
    {
        let alert_json = alert_as_json(&alert);
        let name = alert_json["name"].as_str().expect("Alert should have a name.");
        assert!(registered_alerts.contains(name), "{name} is defined but never registered.");
    }
}
