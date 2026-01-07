use serde_json::{json, Value};

// Define sections with their queries and explicit units
pub fn get_sections() -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    vec![
        (
            "📊 Key Stats",
            vec![
                ("network_connected_peers", "short"),
                ("receive_message_delay_seconds{quantile=\"0.99\"}", "s"),
                ("system_process_cpu_usage_percent", "percent"),
                ("system_process_memory_usage_bytes", "bytes"),
                ("broadcast_message_theoretical_throughput", "binBps"),
                ("rate(receive_message_bytes_sum[20s])", "binBps"),
                ("receive_message_pending_count", "short"),
                ("receive_message_negative_delay_seconds_count", "short"),
            ],
        ),
        (
            "🔍 Performance Comparison",
            vec![
                ("broadcast_message_theoretical_throughput", "binBps"),
                ("rate(receive_message_bytes_sum[20s])", "binBps"),
                ("broadcast_message_theoretical_heartbeat_millis", "ms"),
                ("rate(broadcast_message_count[20s])", "ops"),
                ("rate(receive_message_count[20s])", "ops"),
            ],
        ),
        (
            "📈 Latency Metrics",
            vec![
                ("receive_message_delay_seconds{quantile=\"0.5\"}", "s"),
                ("receive_message_delay_seconds{quantile=\"0.95\"}", "s"),
                ("receive_message_delay_seconds{quantile=\"0.99\"}", "s"),
                ("receive_message_delay_seconds{quantile=\"0.999\"}", "s"),
                ("avg(receive_message_delay_seconds) by (quantile)", "s"),
                ("receive_message_negative_delay_seconds_count", "short"),
                ("ping_latency_seconds{quantile=\"0.5\"}", "s"),
                ("ping_latency_seconds{quantile=\"0.95\"}", "s"),
                ("ping_latency_seconds{quantile=\"0.99\"}", "s"),
                ("ping_latency_seconds{quantile=\"0.999\"}", "s"),
                ("avg(ping_latency_seconds) by (quantile)", "s"),
            ],
        ),
        (
            "📤 Broadcast Metrics",
            vec![
                ("broadcast_message_theoretical_heartbeat_millis", "ms"),
                ("broadcast_message_theoretical_throughput", "binBps"),
                ("broadcast_message_bytes", "bytes"),
                ("rate(broadcast_message_count[1m])", "ops"),
                ("rate(broadcast_message_bytes_sum[1m])", "binBps"),
                (
                    "histogram_quantile(0.95, \
                     rate(broadcast_message_send_delay_seconds_bucket[1m]))",
                    "s",
                ),
            ],
        ),
        (
            "📥 Receive Metrics",
            vec![
                ("receive_message_bytes", "bytes"),
                ("receive_message_pending_count", "short"),
                ("rate(receive_message_count[1m])", "ops"),
                ("rate(receive_message_bytes_sum[1m])", "binBps"),
                ("rate(receive_message_delay_seconds_count[1m])", "ops"),
                ("rate(receive_message_negative_delay_seconds_count[1m])", "ops"),
                ("histogram_quantile(0.95, rate(receive_message_delay_seconds_bucket[1m]))", "s"),
                (
                    "histogram_quantile(0.95, \
                     rate(receive_message_negative_delay_seconds_bucket[1m]))",
                    "s",
                ),
            ],
        ),
        (
            "🌐 Network Metrics",
            vec![
                ("network_connected_peers", "short"),
                ("network_blacklisted_peers", "short"),
                ("network_active_inbound_sessions", "short"),
                ("network_active_outbound_sessions", "short"),
                ("rate(network_stress_test_sent_messages[1m])", "ops"),
                ("rate(network_stress_test_received_messages[1m])", "ops"),
                ("network_reset_total", "short"),
                ("rate(network_dropped_broadcast_messages[1m])", "ops"),
                ("rate(network_event_counter[1m])", "ops"),
            ],
        ),
        (
            "💻 System Metrics",
            vec![
                ("system_process_cpu_usage_percent", "percent"),
                ("system_process_memory_usage_bytes", "bytes"),
                ("system_process_virtual_memory_usage_bytes", "bytes"),
                ("system_total_memory_bytes", "bytes"),
                ("system_available_memory_bytes", "bytes"),
                ("system_used_memory_bytes", "bytes"),
                ("system_cpu_count", "short"),
                ("rate(system_network_bytes_sent_total[1m])", "binBps"),
                ("rate(system_network_bytes_received_total[1m])", "binBps"),
                ("system_network_bytes_sent_current", "binBps"),
                ("system_network_bytes_received_current", "binBps"),
                ("system_tcp_retransmit_rate_percent", "percent"),
                ("system_tcp_segments_out", "short"),
                ("system_tcp_segments_retrans", "short"),
            ],
        ),
        (
            "🔔 Network Events",
            vec![
                ("network_event_counter{event_type=\"connections_established\"}", "short"),
                ("network_event_counter{event_type=\"connections_closed\"}", "short"),
                ("network_event_counter{event_type=\"dial_failure\"}", "short"),
                ("network_event_counter{event_type=\"listen_failure\"}", "short"),
                ("network_event_counter{event_type=\"listen_error\"}", "short"),
                ("network_event_counter{event_type=\"address_change\"}", "short"),
                ("network_event_counter{event_type=\"new_listeners\"}", "short"),
                ("network_event_counter{event_type=\"new_listen_addrs\"}", "short"),
                ("network_event_counter{event_type=\"expired_listen_addrs\"}", "short"),
                ("network_event_counter{event_type=\"listener_closed\"}", "short"),
                ("network_event_counter{event_type=\"new_external_addr_candidate\"}", "short"),
                ("network_event_counter{event_type=\"external_addr_confirmed\"}", "short"),
                ("network_event_counter{event_type=\"external_addr_expired\"}", "short"),
                ("network_event_counter{event_type=\"new_external_addr_of_peer\"}", "short"),
                ("network_event_counter{event_type=\"inbound_connections_handled\"}", "short"),
                ("network_event_counter{event_type=\"outbound_connections_handled\"}", "short"),
                ("network_event_counter{event_type=\"connection_handler_events\"}", "short"),
            ],
        ),
        (
            "⚙️ Tokio Runtime",
            vec![
                ("tokio_total_park_count", "short"),
                ("tokio_max_park_count", "short"),
                ("tokio_min_park_count", "short"),
                ("tokio_total_noop_count", "short"),
                ("tokio_max_noop_count", "short"),
                ("tokio_min_noop_count", "short"),
                ("tokio_total_steal_count", "short"),
                ("tokio_max_steal_count", "short"),
                ("tokio_min_steal_count", "short"),
                ("tokio_total_steal_operations", "short"),
                ("tokio_max_steal_operations", "short"),
                ("tokio_min_steal_operations", "short"),
                ("tokio_total_local_schedule_count", "short"),
                ("tokio_max_local_schedule_count", "short"),
                ("tokio_min_local_schedule_count", "short"),
                ("tokio_total_overflow_count", "short"),
                ("tokio_max_overflow_count", "short"),
                ("tokio_min_overflow_count", "short"),
                ("tokio_total_polls_count", "short"),
                ("tokio_max_polls_count", "short"),
                ("tokio_min_polls_count", "short"),
                ("tokio_injection_queue_depth", "short"),
                ("tokio_total_local_queue_depth", "short"),
                ("tokio_max_local_queue_depth", "short"),
                ("tokio_min_local_queue_depth", "short"),
                ("tokio_blocking_queue_depth", "short"),
                ("tokio_workers_count", "short"),
                ("tokio_num_remote_schedules", "short"),
                ("tokio_live_tasks_count", "short"),
                ("tokio_blocking_threads_count", "short"),
                ("tokio_idle_blocking_threads_count", "short"),
                ("tokio_budget_forced_yield_count", "short"),
                ("tokio_io_driver_ready_count", "short"),
                ("tokio_mean_polls_per_park", "short"),
                ("tokio_busy_ratio", "percentunit"),
                ("tokio_elapsed", "µs"),
                ("tokio_total_busy_duration", "µs"),
                ("tokio_max_busy_duration", "µs"),
                ("tokio_min_busy_duration", "µs"),
                ("tokio_mean_poll_duration", "µs"),
                ("tokio_mean_poll_duration_worker_max", "µs"),
                ("tokio_mean_poll_duration_worker_min", "µs"),
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

    // Generate key stats as stat panels
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

    // Create stat panels for key metrics (4 per row)
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

    // Generate sections with timeseries panels
    for (section_name, queries) in sections.iter().skip(1) {
        let mut section_panels = vec![];
        let mut section_y_pos = 0;

        // Add panels for each query in the section
        for (i, (query, unit)) in queries.iter().enumerate() {
            let panel_title = get_panel_title_from_query(query);

            // Smart layout: 2 panels per row for most sections, 3 for larger sections
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

        // Add section row with nested panels
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

    // Generate dashboard JSON
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

/// Returns Grafana datasource configuration with the specified Prometheus URL
pub fn get_grafana_datasource_config(prometheus_url: &str) -> String {
    format!(
        r#"apiVersion: 1

datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: {}
    isDefault: true
    editable: true
    basicAuth: false
    withCredentials: false
    jsonData:
      httpMethod: GET
      timeInterval: "5s"
"#,
        prometheus_url
    )
}

pub fn get_grafana_dashboard_provisioning_config() -> &'static str {
    r#"apiVersion: 1

providers:
  - name: 'default'
    orgId: 1
    folder: ''
    type: file
    disableDeletion: false
    updateIntervalSeconds: 10
    allowUiUpdates: true
    options:
      path: /etc/grafana/provisioning/dashboards
    folderUid: ""
    folderId: null
"#
}

pub fn get_grafana_config() -> &'static str {
    r#"[analytics]
reporting_enabled = false
check_for_updates = false

[security]
admin_user = admin
admin_password = admin
allow_sign_up = false
disable_gravatar = true

[auth.anonymous]
enabled = true
org_name = Main Org.
org_role = Admin
hide_version = true

[dashboards]
default_home_dashboard_path = ""

[database]
type = sqlite3
path = grafana.db

[session]
provider = memory

[users]
default_theme = dark
allow_sign_up = false
allow_org_create = false
auto_assign_org = true
auto_assign_org_role = Admin

[unified_alerting]
enabled = false

[alerting]
enabled = false

[auth]
disable_login_form = true
disable_signout_menu = true

[server]
serve_from_sub_path = false
http_port = 3000
protocol = http

[log]
mode = console
level = info

[security]
disable_initial_admin_creation = true
cookie_secure = false
cookie_samesite = lax
"#
}

pub fn get_grafana_preferences_json() -> &'static str {
    r#"{
  "homeDashboardUID": "broadcast-network-stress-test",
  "theme": "dark",
  "timezone": "browser"
}"#
}
