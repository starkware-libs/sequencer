use apollo_metrics::metrics::MetricDetails;
use apollo_network::metrics::{EventType, LABEL_NAME_EVENT_TYPE};
use apollo_network_benchmark::metrics::*;
use serde_json::{json, Value};

use crate::args::STRESS_TEST_NAME;

fn metric_name(metric: &dyn MetricDetails) -> String {
    metric.get_name().to_string()
}

fn rate(metric: &dyn MetricDetails, window: &str) -> String {
    format!("rate({}[{}])", metric.get_name(), window)
}

fn quantile(metric: &dyn MetricDetails, q: &str) -> String {
    format!("{}{{quantile=\"{}\"}}", metric.get_name(), q)
}

fn avg_by_quantile(metric: &dyn MetricDetails) -> String {
    format!("avg({}) by (quantile)", metric.get_name())
}

fn event(event_type: &str) -> String {
    format!("{}{{{LABEL_NAME_EVENT_TYPE}=\"{event_type}\"}}", NETWORK_EVENT_COUNTER.get_name())
}

fn pod_filter() -> String {
    format!("pod=~\"{STRESS_TEST_NAME}.*\"")
}

pub fn get_sections(local: bool) -> Vec<(&'static str, Vec<(String, &'static str)>)> {
    let pod_filter = pod_filter();
    let (cpu_query, cpu_unit, memory_query, memory_unit) = if local {
        (
            format!("sum by (pod) (rate(container_cpu_usage_seconds_total{{{pod_filter}}}[1m]))"),
            "short",
            format!("container_memory_working_set_bytes{{{pod_filter}}}"),
            "bytes",
        )
    } else {
        (
            format!(
                "sum by (pod) (rate(container_cpu_usage_seconds_total{{{pod_filter}}}[5m])) / sum \
                 by (pod) (kube_pod_container_resource_requests_cpu_cores{{{pod_filter}}})"
            ),
            "percentunit",
            format!(
                "sum by (pod) (container_memory_working_set_bytes{{{pod_filter}}}) / sum by (pod) \
                 (kube_pod_container_resource_requests_memory_bytes{{{pod_filter}}})"
            ),
            "percentunit",
        )
    };

    vec![
        (
            "📊 Key Stats",
            vec![
                (metric_name(&NETWORK_CONNECTED_PEERS), "short"),
                (quantile(&RECEIVE_MESSAGE_DELAY_SECONDS, "0.99"), "s"),
                (cpu_query, cpu_unit),
                (memory_query, memory_unit),
                (metric_name(&BROADCAST_MESSAGE_THROUGHPUT), "binBps"),
                (rate(&RECEIVE_MESSAGE_BYTES_SUM, "20s"), "binBps"),
                (metric_name(&RECEIVE_MESSAGE_PENDING_COUNT), "short"),
                ("tokio_workers_count".into(), "short"),
                ("tokio_mean_poll_duration_worker_max".into(), "µs"),
                ("tokio_budget_forced_yield_count".into(), "short"),
            ],
        ),
        (
            "🔍 Performance Comparison",
            vec![
                (metric_name(&BROADCAST_MESSAGE_THROUGHPUT), "binBps"),
                (rate(&RECEIVE_MESSAGE_BYTES_SUM, "20s"), "binBps"),
                (metric_name(&BROADCAST_MESSAGE_HEARTBEAT_MILLIS), "ms"),
                (rate(&BROADCAST_MESSAGE_COUNT, "20s"), "ops"),
                (rate(&RECEIVE_MESSAGE_COUNT, "20s"), "ops"),
            ],
        ),
        (
            "📈 Latency Metrics",
            vec![
                (quantile(&RECEIVE_MESSAGE_DELAY_SECONDS, "0.5"), "s"),
                (quantile(&RECEIVE_MESSAGE_DELAY_SECONDS, "0.95"), "s"),
                (quantile(&RECEIVE_MESSAGE_DELAY_SECONDS, "0.99"), "s"),
                (quantile(&RECEIVE_MESSAGE_DELAY_SECONDS, "0.999"), "s"),
                (avg_by_quantile(&RECEIVE_MESSAGE_DELAY_SECONDS), "s"),
                (quantile(&PING_LATENCY_SECONDS, "0.5"), "s"),
                (quantile(&PING_LATENCY_SECONDS, "0.95"), "s"),
                (quantile(&PING_LATENCY_SECONDS, "0.99"), "s"),
                (quantile(&PING_LATENCY_SECONDS, "0.999"), "s"),
                (avg_by_quantile(&PING_LATENCY_SECONDS), "s"),
            ],
        ),
        (
            "📤 Broadcast Metrics",
            vec![
                (metric_name(&BROADCAST_MESSAGE_HEARTBEAT_MILLIS), "ms"),
                (metric_name(&BROADCAST_MESSAGE_THROUGHPUT), "binBps"),
                (metric_name(&BROADCAST_MESSAGE_BYTES), "bytes"),
                (rate(&BROADCAST_MESSAGE_COUNT, "1m"), "ops"),
                (rate(&BROADCAST_MESSAGE_BYTES_SUM, "1m"), "binBps"),
                (quantile(&BROADCAST_MESSAGE_SEND_DELAY_SECONDS, "0.95"), "s"),
            ],
        ),
        (
            "📥 Receive Metrics",
            vec![
                (metric_name(&RECEIVE_MESSAGE_BYTES), "bytes"),
                (metric_name(&RECEIVE_MESSAGE_PENDING_COUNT), "short"),
                (rate(&RECEIVE_MESSAGE_COUNT, "1m"), "ops"),
                (rate(&RECEIVE_MESSAGE_BYTES_SUM, "1m"), "binBps"),
                (format!("rate({}_count[1m])", RECEIVE_MESSAGE_DELAY_SECONDS.get_name()), "ops"),
            ],
        ),
        (
            "🌐 Network Metrics",
            vec![
                (metric_name(&NETWORK_CONNECTED_PEERS), "short"),
                (metric_name(&NETWORK_BLACKLISTED_PEERS), "short"),
                (metric_name(&NETWORK_ACTIVE_INBOUND_SESSIONS), "short"),
                (metric_name(&NETWORK_ACTIVE_OUTBOUND_SESSIONS), "short"),
                (rate(&NETWORK_STRESS_TEST_SENT_MESSAGES, "1m"), "ops"),
                (rate(&NETWORK_STRESS_TEST_RECEIVED_MESSAGES, "1m"), "ops"),
                (rate(&NETWORK_DROPPED_BROADCAST_MESSAGES, "1m"), "ops"),
                (rate(&NETWORK_EVENT_COUNTER, "1m"), "ops"),
            ],
        ),
        {
            let rate_window = if local { "1m" } else { "5m" };
            let (per_container_cpu_unit, total_cpu_unit, memory_unit) = if local {
                ("short", "short", "bytes")
            } else {
                ("percentunit", "percentunit", "percentunit")
            };

            let mut container_metrics: Vec<(String, &str)> = vec![
                (format!("rate(container_cpu_usage_seconds_total{{{pod_filter}}}[{rate_window}])"), per_container_cpu_unit),
                (format!("sum(rate(container_cpu_usage_seconds_total{{{pod_filter}}}[{rate_window}]))"), total_cpu_unit),
                (format!("container_memory_working_set_bytes{{{pod_filter}}}"), memory_unit),
                (format!("sum(container_memory_working_set_bytes{{{pod_filter}}})"), memory_unit),
                (format!("rate(container_network_receive_bytes_total{{{pod_filter}}}[{rate_window}])"), "binBps"),
                (format!("rate(container_network_transmit_bytes_total{{{pod_filter}}}[{rate_window}])"), "binBps"),
            ];

            if !local {
                container_metrics.extend([
                    (format!(
                        "sum by (pod) (rate(container_cpu_usage_seconds_total{{{pod_filter}}}[{rate_window}])) \
                         / sum by (pod) (kube_pod_container_resource_requests_cpu_cores{{{pod_filter}}})"
                    ), "percentunit"),
                    (format!(
                        "sum by (pod) (container_memory_working_set_bytes{{{pod_filter}}}) \
                         / sum by (pod) (kube_pod_container_resource_requests_memory_bytes{{{pod_filter}}})"
                    ), "percentunit"),
                ]);
            }

            ("📦 Container Metrics (cAdvisor)", container_metrics)
        },
        {
            let event_types: Vec<&'static str> = vec![
                EventType::ConnectionsEstablished.into(),
                EventType::ConnectionsClosed.into(),
                EventType::DialFailure.into(),
                EventType::ListenFailure.into(),
                EventType::ListenError.into(),
                EventType::AddressChange.into(),
                EventType::NewListeners.into(),
                EventType::NewListenAddrs.into(),
                EventType::ExpiredListenAddrs.into(),
                EventType::ListenerClosed.into(),
                EventType::NewExternalAddrCandidate.into(),
                EventType::ExternalAddrConfirmed.into(),
                EventType::ExternalAddrExpired.into(),
                EventType::NewExternalAddrOfPeer.into(),
                EventType::InboundConnectionsHandled.into(),
                EventType::OutboundConnectionsHandled.into(),
                EventType::ConnectionHandlerEvents.into(),
            ];
            (
                "🔔 Network Events",
                event_types.into_iter().map(|label| (event(label), "short")).collect(),
            )
        },
        (
            "🚀 Propeller Protocol",
            vec![
                ("propeller_shards_published".into(), "short"),
                ("propeller_shards_sent".into(), "short"),
                ("propeller_shards_received".into(), "short"),
                ("propeller_shards_forwarded".into(), "short"),
                ("propeller_messages_reconstructed".into(), "short"),
                ("propeller_messages_reconstruction_failed".into(), "short"),
                ("propeller_trees_generated".into(), "short"),
                ("propeller_shards_send_failed".into(), "short"),
                ("propeller_shards_validation_failed".into(), "short"),
                ("propeller_collection_lengths".into(), "short"),
            ],
        ),
        (
            "⚙️ Tokio Runtime",
            vec![
                ("tokio_total_park_count".into(), "short"),
                ("tokio_max_park_count".into(), "short"),
                ("tokio_min_park_count".into(), "short"),
                ("tokio_total_noop_count".into(), "short"),
                ("tokio_max_noop_count".into(), "short"),
                ("tokio_min_noop_count".into(), "short"),
                ("tokio_total_steal_count".into(), "short"),
                ("tokio_max_steal_count".into(), "short"),
                ("tokio_min_steal_count".into(), "short"),
                ("tokio_total_steal_operations".into(), "short"),
                ("tokio_max_steal_operations".into(), "short"),
                ("tokio_min_steal_operations".into(), "short"),
                ("tokio_total_local_schedule_count".into(), "short"),
                ("tokio_max_local_schedule_count".into(), "short"),
                ("tokio_min_local_schedule_count".into(), "short"),
                ("tokio_total_overflow_count".into(), "short"),
                ("tokio_max_overflow_count".into(), "short"),
                ("tokio_min_overflow_count".into(), "short"),
                ("tokio_total_polls_count".into(), "short"),
                ("tokio_max_polls_count".into(), "short"),
                ("tokio_min_polls_count".into(), "short"),
                ("tokio_injection_queue_depth".into(), "short"),
                ("tokio_total_local_queue_depth".into(), "short"),
                ("tokio_max_local_queue_depth".into(), "short"),
                ("tokio_min_local_queue_depth".into(), "short"),
                ("tokio_blocking_queue_depth".into(), "short"),
                ("tokio_workers_count".into(), "short"),
                ("tokio_num_remote_schedules".into(), "short"),
                ("tokio_live_tasks_count".into(), "short"),
                ("tokio_blocking_threads_count".into(), "short"),
                ("tokio_idle_blocking_threads_count".into(), "short"),
                ("tokio_budget_forced_yield_count".into(), "short"),
                ("tokio_io_driver_ready_count".into(), "short"),
                ("tokio_mean_polls_per_park".into(), "short"),
                ("tokio_busy_ratio".into(), "percentunit"),
                ("tokio_elapsed".into(), "µs"),
                ("tokio_total_busy_duration".into(), "µs"),
                ("tokio_max_busy_duration".into(), "µs"),
                ("tokio_min_busy_duration".into(), "µs"),
                ("tokio_mean_poll_duration".into(), "µs"),
                ("tokio_mean_poll_duration_worker_max".into(), "µs"),
                ("tokio_mean_poll_duration_worker_min".into(), "µs"),
            ],
        ),
    ]
}

fn get_panel_title_from_query(query: &str) -> String {
    match query.get(..60) {
        Some(prefix) if query.len() > 60 => format!("{prefix}..."),
        _ => query.to_string(),
    }
}

fn get_description_from_query(query: &str) -> Option<&'static str> {
    if query.contains("container_cpu_usage_seconds_total") {
        Some(
            "CPU usage in cores (CPU-seconds per second). 1.0 = one core fully utilized, 0.5 = \
             half a core, 3.2 = 3.2 cores worth of work. Compare with tokio_workers_count to \
             gauge saturation.",
        )
    } else if query.contains("mean_poll_duration_worker_max") {
        Some(
            "Mean poll duration of the busiest tokio worker thread. Healthy: <100µs. Warning: \
             >1ms (tasks may be blocking the event loop). Critical: >10ms (event loop is stalled, \
             expect latency spikes and timeouts).",
        )
    } else if query.contains("budget_forced_yield_count") {
        Some(
            "Times the tokio coop budget forced a task to yield. Non-zero means tasks are doing \
             too much work in a single poll without hitting an .await — a direct consequence of \
             long poll times.",
        )
    } else {
        None
    }
}

fn get_thresholds_from_query(query: &str) -> Value {
    if query.contains("kube_pod_container_resource_requests") {
        // Production-style utilization ratio (0.0-1.0).
        json!({
            "steps": [
                {"color": "green", "value": null},
                {"color": "yellow", "value": 0.6},
                {"color": "red", "value": 0.8}
            ]
        })
    } else if query.contains("container_cpu_usage_seconds_total") {
        // Raw CPU cores (local deployment).
        json!({
            "steps": [
                {"color": "green", "value": null},
                {"color": "yellow", "value": 2},
                {"color": "red", "value": 3}
            ]
        })
    } else if query.contains("mean_poll_duration_worker_max") {
        // Units are µs. >1000µs (1ms) is concerning, >10000µs (10ms) is critical.
        json!({
            "steps": [
                {"color": "green", "value": null},
                {"color": "yellow", "value": 1000},
                {"color": "red", "value": 10000}
            ]
        })
    } else if query.contains("budget_forced_yield_count") {
        // Any non-zero value indicates tasks doing too much work per poll.
        json!({
            "steps": [
                {"color": "green", "value": null},
                {"color": "red", "value": 1}
            ]
        })
    } else if query.contains("connected_peers") {
        json!({
            "steps": [
                {"color": "red", "value": null},
                {"color": "green", "value": 1}
            ]
        })
    } else if query.contains("slow_poll_ratio") || query.contains("long_delay_ratio") {
        json!({
            "steps": [
                {"color": "green", "value": null},
                {"color": "yellow", "value": 0.1},
                {"color": "red", "value": 0.3}
            ]
        })
    } else if query.contains("delay")
        || query.contains("poll_duration")
        || query.contains("scheduled_duration")
    {
        json!({
            "steps": [
                {"color": "green", "value": null},
                {"color": "yellow", "value": 0.1},
                {"color": "red", "value": 1.0}
            ]
        })
    } else {
        json!({
            "steps": [{"color": "green", "value": null}]
        })
    }
}

pub fn get_grafana_dashboard_json(local: bool) -> String {
    let refresh_rate = if local { "5s" } else { "30s" };
    let sections = get_sections(local);
    let mut panels = vec![];
    let mut panel_id = 100;
    let mut y_pos = 0;

    let key_stats_queries = &sections[0].1;

    panels.push(json!({
        "id": panel_id,
        "title": "📊 Key Stats Overview",
        "type": "row",
        "gridPos": {"h": 1, "w": 24, "x": 0, "y": y_pos},
        "collapsed": false
    }));
    panel_id += 1;
    y_pos += 1;

    for (panel_index, (query, unit)) in key_stats_queries.iter().enumerate() {
        let x_pos = (panel_index % 4) * 6;
        if panel_index > 0 && panel_index % 4 == 0 {
            y_pos += 6;
        }

        let panel_title = get_panel_title_from_query(query);
        let description = get_description_from_query(query);

        let mut panel = json!({
            "id": panel_id,
            "title": panel_title,
            "type": "stat",
            "targets": [{"expr": query, "refId": "A"}],
            "fieldConfig": {
                "defaults": {
                    "unit": unit,
                    "thresholds": get_thresholds_from_query(query)
                }
            },
            "options": {
                "reduceOptions": {
                    "values": false,
                    "calcs": ["lastNotNull"]
                },
                "orientation": "auto",
                "textMode": "auto",
                "colorMode": "value",
                "graphMode": "area"
            },
            "gridPos": {"h": 6, "w": 6, "x": x_pos, "y": y_pos}
        });
        if let Some(desc) = description {
            panel["description"] = json!(desc);
        }
        panels.push(panel);
        panel_id += 1;
    }

    y_pos += 7;

    for (section_name, queries) in sections.iter().skip(1) {
        let mut section_panels = vec![];
        let mut section_y_pos = 0;

        for (panel_index, (query, unit)) in queries.iter().enumerate() {
            let panel_title = get_panel_title_from_query(query);

            let panels_per_row = if queries.len() > 4 { 3 } else { 2 };
            let width = 24 / panels_per_row;
            let x_pos = (panel_index % panels_per_row) * width;
            if panel_index > 0 && panel_index % panels_per_row == 0 {
                section_y_pos += 8;
            }

            section_panels.push(json!({
                "id": panel_id,
                "title": panel_title,
                "type": "timeseries",
                "targets": [{"expr": query, "refId": "A"}],
                "fieldConfig": {"defaults": {"unit": unit}},
                "options": {
                    "tooltip": {"mode": "single", "sort": "none"},
                    "legend": {
                        "showLegend": true,
                        "displayMode": "list",
                        "placement": "bottom"
                    }
                },
                "gridPos": {"h": 8, "w": width, "x": x_pos, "y": section_y_pos}
            }));
            panel_id += 1;
        }

        panels.push(json!({
            "id": panel_id,
            "title": section_name,
            "type": "row",
            "gridPos": {"h": 1, "w": 24, "x": 0, "y": y_pos},
            "collapsed": true,
            "panels": section_panels
        }));
        panel_id += 1;
        y_pos += 1;
    }

    let dashboard = json!({
        "id": 1,
        "uid": STRESS_TEST_NAME,
        "title": "Broadcast Network Stress Test - Data-Driven Dashboard",
        "tags": ["network", "stress-test", "apollo", "data-driven"],
        "timezone": "browser",
        "panels": panels,
        "time": {"from": "now-15m", "to": "now"},
        "refresh": refresh_rate
    });

    serde_json::to_string_pretty(&dashboard)
        .expect("dashboard is a serde_json::Value built in-process, serialization cannot fail")
}

const DATASOURCE_TEMPLATE: &str = include_str!("../../../config/datasource.yml");

pub fn get_grafana_datasource_config(prometheus_url: &str) -> String {
    DATASOURCE_TEMPLATE.replace("__PROMETHEUS_URL__", prometheus_url)
}

pub fn get_grafana_dashboard_provisioning_config() -> &'static str {
    include_str!("../../../config/dashboard_config.yml")
}

pub fn get_grafana_config() -> &'static str {
    include_str!("../../../config/grafana.ini")
}

pub fn get_grafana_preferences_json() -> &'static str {
    include_str!("../../../config/preferences.json")
}
