use apollo_metrics::metrics::MetricDetails;
use apollo_network_benchmark::metrics::*;
use serde_json::{json, Value};

fn m(metric: &dyn MetricDetails) -> String {
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

fn histogram_quantile_rate(metric: &dyn MetricDetails, q: &str, window: &str) -> String {
    format!("histogram_quantile({}, rate({}_bucket[{}]))", q, metric.get_name(), window)
}

fn event(event_type: &str) -> String {
    format!("{}{{event_type=\"{}\"}}", NETWORK_EVENT_COUNTER.get_name(), event_type)
}

pub fn get_sections() -> Vec<(&'static str, Vec<(String, &'static str)>)> {
    vec![
        (
            "📊 Key Stats",
            vec![
                (m(&NETWORK_CONNECTED_PEERS), "short"),
                (quantile(&RECEIVE_MESSAGE_DELAY_SECONDS, "0.99"), "s"),
                ("rate(node_cpu_seconds_total{mode!=\"idle\"}[1m])".into(), "percentunit"),
                ("node_memory_MemTotal_bytes - node_memory_MemAvailable_bytes".into(), "bytes"),
                (m(&BROADCAST_MESSAGE_THROUGHPUT), "binBps"),
                (rate(&RECEIVE_MESSAGE_BYTES_SUM, "20s"), "binBps"),
                (m(&RECEIVE_MESSAGE_PENDING_COUNT), "short"),
                ("receive_message_negative_delay_seconds_count".into(), "short"),
            ],
        ),
        (
            "🔍 Performance Comparison",
            vec![
                (m(&BROADCAST_MESSAGE_THROUGHPUT), "binBps"),
                (rate(&RECEIVE_MESSAGE_BYTES_SUM, "20s"), "binBps"),
                (m(&BROADCAST_MESSAGE_HEARTBEAT_MILLIS), "ms"),
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
                ("receive_message_negative_delay_seconds_count".into(), "short"),
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
                (m(&BROADCAST_MESSAGE_HEARTBEAT_MILLIS), "ms"),
                (m(&BROADCAST_MESSAGE_THROUGHPUT), "binBps"),
                (m(&BROADCAST_MESSAGE_BYTES), "bytes"),
                (rate(&BROADCAST_MESSAGE_COUNT, "1m"), "ops"),
                (rate(&BROADCAST_MESSAGE_BYTES_SUM, "1m"), "binBps"),
                (histogram_quantile_rate(&BROADCAST_MESSAGE_SEND_DELAY_SECONDS, "0.95", "1m"), "s"),
            ],
        ),
        (
            "📥 Receive Metrics",
            vec![
                (m(&RECEIVE_MESSAGE_BYTES), "bytes"),
                (m(&RECEIVE_MESSAGE_PENDING_COUNT), "short"),
                (rate(&RECEIVE_MESSAGE_COUNT, "1m"), "ops"),
                (rate(&RECEIVE_MESSAGE_BYTES_SUM, "1m"), "binBps"),
                (rate(&RECEIVE_MESSAGE_DELAY_SECONDS, "1m"), "ops"),
                ("rate(receive_message_negative_delay_seconds_count[1m])".into(), "ops"),
                (histogram_quantile_rate(&RECEIVE_MESSAGE_DELAY_SECONDS, "0.95", "1m"), "s"),
                (
                    "histogram_quantile(0.95, \
                     rate(receive_message_negative_delay_seconds_bucket[1m]))"
                        .into(),
                    "s",
                ),
            ],
        ),
        (
            "🌐 Network Metrics",
            vec![
                (m(&NETWORK_CONNECTED_PEERS), "short"),
                (m(&NETWORK_BLACKLISTED_PEERS), "short"),
                (m(&NETWORK_ACTIVE_INBOUND_SESSIONS), "short"),
                (m(&NETWORK_ACTIVE_OUTBOUND_SESSIONS), "short"),
                (rate(&NETWORK_STRESS_TEST_SENT_MESSAGES, "1m"), "ops"),
                (rate(&NETWORK_STRESS_TEST_RECEIVED_MESSAGES, "1m"), "ops"),
                ("network_reset_total".into(), "short"),
                (rate(&NETWORK_DROPPED_BROADCAST_MESSAGES, "1m"), "ops"),
                (rate(&NETWORK_EVENT_COUNTER, "1m"), "ops"),
            ],
        ),
        (
            "💻 System Metrics (node_exporter)",
            vec![
                ("node_memory_MemTotal_bytes".into(), "bytes"),
                ("node_memory_MemAvailable_bytes".into(), "bytes"),
                ("node_memory_MemTotal_bytes - node_memory_MemAvailable_bytes".into(), "bytes"),
                ("count(node_cpu_seconds_total{mode=\"idle\"})".into(), "short"),
                ("rate(node_cpu_seconds_total{mode!=\"idle\"}[1m])".into(), "percentunit"),
                ("rate(node_network_transmit_bytes_total[1m])".into(), "binBps"),
                ("rate(node_network_receive_bytes_total[1m])".into(), "binBps"),
                (
                    "rate(node_netstat_Tcp_RetransSegs[1m]) / rate(node_netstat_Tcp_OutSegs[1m])"
                        .into(),
                    "percentunit",
                ),
                ("node_netstat_Tcp_OutSegs".into(), "short"),
                ("node_netstat_Tcp_RetransSegs".into(), "short"),
            ],
        ),
        (
            "🔔 Network Events",
            vec![
                (event("connections_established"), "short"),
                (event("connections_closed"), "short"),
                (event("dial_failure"), "short"),
                (event("listen_failure"), "short"),
                (event("listen_error"), "short"),
                (event("address_change"), "short"),
                (event("new_listeners"), "short"),
                (event("new_listen_addrs"), "short"),
                (event("expired_listen_addrs"), "short"),
                (event("listener_closed"), "short"),
                (event("new_external_addr_candidate"), "short"),
                (event("external_addr_confirmed"), "short"),
                (event("external_addr_expired"), "short"),
                (event("new_external_addr_of_peer"), "short"),
                (event("inbound_connections_handled"), "short"),
                (event("outbound_connections_handled"), "short"),
                (event("connection_handler_events"), "short"),
            ],
        ),
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
    if query.len() > 60 { format!("{}...", &query[..60]) } else { query.to_string() }
}

fn get_thresholds_from_query(query: &str) -> Value {
    if query.contains("cpu") {
        json!({
            "steps": [
                {"color": "green", "value": null},
                {"color": "yellow", "value": 70},
                {"color": "red", "value": 90}
            ]
        })
    } else if query.contains("connected_peers") {
        json!({
            "steps": [
                {"color": "red", "value": null},
                {"color": "green", "value": 1}
            ]
        })
    } else if query.contains("negative_delay") {
        json!({
            "steps": [
                {"color": "green", "value": null},
                {"color": "red", "value": 0.001}
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

pub fn get_grafana_dashboard_json(refresh_rate: &str) -> String {
    let sections = get_sections();
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

    for (i, (query, unit)) in key_stats_queries.iter().enumerate() {
        let x_pos = (i % 4) * 6;
        if i > 0 && i % 4 == 0 {
            y_pos += 6;
        }

        let panel_title = get_panel_title_from_query(query);

        panels.push(json!({
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
        }));
        panel_id += 1;
    }

    y_pos += 7;

    for (section_name, queries) in sections.iter().skip(1) {
        let mut section_panels = vec![];
        let mut section_y_pos = 0;

        for (i, (query, unit)) in queries.iter().enumerate() {
            let panel_title = get_panel_title_from_query(query);

            let panels_per_row = if queries.len() > 4 { 3 } else { 2 };
            let width = 24 / panels_per_row;
            let x_pos = (i % panels_per_row) * width;
            if i > 0 && i % panels_per_row == 0 {
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
        "uid": "broadcast-network-stress-test",
        "title": "Broadcast Network Stress Test - Data-Driven Dashboard",
        "tags": ["network", "stress-test", "apollo", "data-driven"],
        "timezone": "browser",
        "panels": panels,
        "time": {"from": "now-15m", "to": "now"},
        "refresh": refresh_rate
    });

    serde_json::to_string_pretty(&dashboard).unwrap()
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
