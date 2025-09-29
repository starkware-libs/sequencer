"""
Grafana dashboard configuration for the broadcast network stress test.
Data-driven approach using SECTIONS and ALERTS variables for maintainable configuration.
"""

# Define sections with their queries and explicit units - much easier to maintain!
SECTIONS = {
    "📊 Key Stats": [
        ("network_connected_peers", "short"),
        (
            "rate(receive_message_delay_seconds_sum[1m]) / rate(receive_message_delay_seconds_count[1m])",
            "s",
        ),
        ("system_process_cpu_usage_percent", "percent"),
        ("system_process_memory_usage_bytes", "bytes"),
        ("broadcast_message_theoretical_throughput", "binBps"),
        ("rate(receive_message_bytes_sum[20s])", "binBps"),
        ("receive_message_negative_delay_seconds_count", "short"),
    ],
    "🔍 Performance Comparison": [
        ("broadcast_message_theoretical_throughput", "binBps"),
        ("rate(receive_message_bytes_sum[20s])", "binBps"),
        ("broadcast_message_theoretical_heartbeat_millis", "ms"),
        ("rate(broadcast_message_count[20s])", "ops"),
        ("rate(receive_message_count[20s])", "ops"),
    ],
    "📈 Latency Metrics": [
        ('receive_message_delay_seconds{quantile="0.5"}', "s"),
        ('receive_message_delay_seconds{quantile="0.95"}', "s"),
        ('receive_message_delay_seconds{quantile="0.99"}', "s"),
        ('receive_message_delay_seconds{quantile="0.999"}', "s"),
        # average latency by quantile in one panel:
        ("avg(receive_message_delay_seconds) by (quantile)", "s"),
        ("receive_message_negative_delay_seconds_count", "short"),
    ],
    "📤 Broadcast Metrics": [
        ("broadcast_message_theoretical_heartbeat_millis", "ms"),
        ("broadcast_message_theoretical_throughput", "binBps"),
        ("broadcast_message_bytes", "bytes"),
        ("rate(broadcast_message_count[1m])", "ops"),
        ("rate(broadcast_message_bytes_sum[1m])", "binBps"),
        (
            "histogram_quantile(0.95, rate(broadcast_message_send_delay_seconds_bucket[1m]))",
            "s",
        ),
    ],
    "📥 Receive Metrics": [
        ("receive_message_bytes", "bytes"),
        ("rate(receive_message_count[1m])", "ops"),
        ("rate(receive_message_bytes_sum[1m])", "binBps"),
        ("rate(receive_message_delay_seconds_count[1m])", "ops"),
        ("rate(receive_message_negative_delay_seconds_count[1m])", "ops"),
        (
            "histogram_quantile(0.95, rate(receive_message_delay_seconds_bucket[1m]))",
            "s",
        ),
        (
            "histogram_quantile(0.95, rate(receive_message_negative_delay_seconds_bucket[1m]))",
            "s",
        ),
    ],
    "🌐 Network Metrics": [
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
    "💻 System Metrics": [
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
    ],
    "🔔 Network Events": [
        ('network_event_counter{event_type="connections_established"}', "short"),
        ('network_event_counter{event_type="connections_closed"}', "short"),
        ('network_event_counter{event_type="dial_failure"}', "short"),
        ('network_event_counter{event_type="listen_failure"}', "short"),
        ('network_event_counter{event_type="listen_error"}', "short"),
        ('network_event_counter{event_type="address_change"}', "short"),
        ('network_event_counter{event_type="new_listeners"}', "short"),
        ('network_event_counter{event_type="new_listen_addrs"}', "short"),
        ('network_event_counter{event_type="expired_listen_addrs"}', "short"),
        ('network_event_counter{event_type="listener_closed"}', "short"),
        ('network_event_counter{event_type="new_external_addr_candidate"}', "short"),
        ('network_event_counter{event_type="external_addr_confirmed"}', "short"),
        ('network_event_counter{event_type="external_addr_expired"}', "short"),
        ('network_event_counter{event_type="new_external_addr_of_peer"}', "short"),
        ('network_event_counter{event_type="inbound_connections_handled"}', "short"),
        ('network_event_counter{event_type="outbound_connections_handled"}', "short"),
        ('network_event_counter{event_type="connection_handler_events"}', "short"),
    ],
}

# Define alerts - easy to add new ones!
ALERTS = [
    "min(network_connected_peers) < 1",
    "rate(receive_message_negative_delay_seconds_count[1m]) > 0.001",
    "system_process_cpu_usage_percent > 90",
    "system_process_memory_usage_bytes > 8000000000",  # 8GB
    "system_process_virtual_memory_usage_bytes > 16000000000",  # 16GB virtual memory
    "rate(network_dropped_broadcast_messages[1m]) > 10",  # High message drop rate
    # "network_reset_total > 5",  # Too many network resets
]


def get_grafana_dashboard_json() -> str:
    """Generate Grafana dashboard using data-driven approach with SECTIONS and ALERTS.

    This approach is much more maintainable - just add queries to SECTIONS or ALERTS
    instead of writing hundreds of lines of JSON configuration.
    """
    panels = []
    panel_id = 100
    y_pos = 0

    # Generate key stats as stat panels
    key_stats_queries = SECTIONS["📊 Key Stats"]
    panels.append(
        {
            "id": panel_id,
            "title": "📊 Key Stats Overview",
            "type": "row",
            "gridPos": {"h": 1, "w": 24, "x": 0, "y": y_pos},
            "collapsed": False,
        }
    )
    panel_id += 1
    y_pos += 1

    # Create stat panels for key metrics (2 per row)
    for i, (query, unit) in enumerate(key_stats_queries):
        x_pos = (i % 4) * 6
        if i > 0 and i % 4 == 0:
            y_pos += 6

        panel_title = _get_panel_title_from_query(query)

        panels.append(
            {
                "id": panel_id,
                "title": panel_title,
                "type": "stat",
                "targets": [{"expr": query, "refId": "A"}],
                "fieldConfig": {
                    "defaults": {
                        "unit": unit,
                        "thresholds": _get_thresholds_from_query(query),
                    }
                },
                "options": {
                    "reduceOptions": {
                        "values": False,
                        "calcs": ["lastNotNull"],
                    },
                    "orientation": "auto",
                    "textMode": "auto",
                    "colorMode": "value",
                    "graphMode": "area",
                },
                "gridPos": {"h": 6, "w": 6, "x": x_pos, "y": y_pos},
            }
        )
        panel_id += 1

    y_pos += 7

    # Generate sections with timeseries panels
    for section_name, queries in SECTIONS.items():
        if section_name == "📊 Key Stats":
            continue  # Already handled above

        # Create panels for this section
        section_panels = []
        section_y_pos = 0

        # Add panels for each query in the section
        for i, (query, unit) in enumerate(queries):
            panel_title = _get_panel_title_from_query(query)

            # Smart layout: 2 panels per row for most sections, 3 for larger sections
            panels_per_row = 3 if len(queries) > 4 else 2
            width = 24 // panels_per_row
            x_pos = (i % panels_per_row) * width
            if i > 0 and i % panels_per_row == 0:
                section_y_pos += 8

            section_panels.append(
                {
                    "id": panel_id,
                    "title": panel_title,
                    "type": "timeseries",
                    "targets": [{"expr": query, "refId": "A"}],
                    "fieldConfig": {"defaults": {"unit": unit}},
                    "options": {
                        "tooltip": {"mode": "single", "sort": "none"},
                        "legend": {
                            "showLegend": True,
                            "displayMode": "list",
                            "placement": "bottom",
                        },
                    },
                    "gridPos": {"h": 8, "w": width, "x": x_pos, "y": section_y_pos},
                }
            )
            panel_id += 1

        # Add section row with nested panels
        panels.append(
            {
                "id": panel_id,
                "title": section_name,
                "type": "row",
                "gridPos": {"h": 1, "w": 24, "x": 0, "y": y_pos},
                "collapsed": True,
                "panels": section_panels,
            }
        )
        panel_id += 1
        y_pos += 1

    # Generate dashboard JSON
    dashboard = {
        "id": 1,
        "uid": "broadcast-network-stress-test",
        "title": "Broadcast Network Stress Test - Data-Driven Dashboard",
        "tags": ["network", "stress-test", "apollo", "data-driven"],
        "timezone": "browser",
        "panels": panels,
        "time": {"from": "now-15m", "to": "now"},
        "refresh": "5s",
    }

    import json

    return json.dumps(dashboard, indent=2)


def _get_panel_title_from_query(query: str) -> str:
    """Extract panel title directly from the query - simple and informative."""
    # Use the query itself, truncated if too long
    return query[:60] + "..." if len(query) > 60 else query


def _get_thresholds_from_query(query: str) -> dict:
    """Get appropriate thresholds for a query."""
    if "cpu" in query:
        return {
            "steps": [
                {"color": "green", "value": None},
                {"color": "yellow", "value": 70},
                {"color": "red", "value": 90},
            ]
        }
    elif "connected_peers" in query:
        return {
            "steps": [{"color": "red", "value": None}, {"color": "green", "value": 1}]
        }
    elif "negative_delay" in query:
        return {
            "steps": [
                {"color": "green", "value": None},
                {"color": "red", "value": 0.001},
            ]
        }
    elif "delay" in query:
        return {
            "steps": [
                {"color": "green", "value": None},
                {"color": "yellow", "value": 0.1},
                {"color": "red", "value": 1.0},
            ]
        }

    # Default thresholds
    return {"steps": [{"color": "green", "value": None}]}


def get_grafana_alerts_json() -> str:
    """Generate Grafana alerting rules from ALERTS list."""
    rules = []

    for i, alert_query in enumerate(ALERTS):
        rule_id = f"stress_test_alert_{i}"
        alert_title = _get_alert_title_from_query(alert_query)

        rules.append(
            {
                "uid": rule_id,
                "title": alert_title,
                "condition": "B",
                "data": [
                    {
                        "refId": "A",
                        "queryType": "",
                        "relativeTimeRange": {"from": 300, "to": 0},
                        "model": {
                            "expr": alert_query.split(" ")[0],  # Extract base query
                            "interval": "",
                            "refId": "A",
                        },
                    },
                    {
                        "refId": "B",
                        "queryType": "",
                        "relativeTimeRange": {"from": 0, "to": 0},
                        "model": {
                            "conditions": [
                                {
                                    "evaluator": {
                                        "params": [
                                            _get_alert_threshold_from_query(alert_query)
                                        ],
                                        "type": _get_alert_operator_from_query(
                                            alert_query
                                        ),
                                    },
                                    "operator": {"type": "and"},
                                    "query": {"params": ["A"]},
                                    "reducer": {"params": [], "type": "last"},
                                    "type": "query",
                                }
                            ],
                            "refId": "B",
                        },
                    },
                ],
                "noDataState": "NoData",
                "execErrState": "Alerting",
                "for": "30s",
                "annotations": {
                    "description": f"Alert triggered: {alert_title}",
                    "summary": alert_title,
                },
                "labels": {"severity": "critical", "team": "network"},
            }
        )

    alert_config = {
        "groups": [
            {
                "name": "stress_test_alerts",
                "orgId": 1,
                "folder": "alerts",
                "rules": rules,
            }
        ]
    }

    import json

    return json.dumps(alert_config, indent=2)


def _get_alert_title_from_query(alert_query: str) -> str:
    """Generate alert title from query."""
    if "connected_peers" in alert_query:
        return "Network Connectivity Alert"
    elif "negative_delay" in alert_query:
        return "Negative Message Delay Alert"
    elif "cpu" in alert_query:
        return "High CPU Usage Alert"
    elif "memory" in alert_query:
        return "High Memory Usage Alert"
    return f"Alert: {alert_query}"


def _get_alert_threshold_from_query(alert_query: str) -> float:
    """Extract threshold value from alert query."""
    parts = alert_query.split()
    for part in parts:
        try:
            return float(part)
        except ValueError:
            continue
    return 1.0


def _get_alert_operator_from_query(alert_query: str) -> str:
    """Extract comparison operator from alert query."""
    if " < " in alert_query:
        return "lt"
    elif " > " in alert_query:
        return "gt"
    elif " <= " in alert_query:
        return "le"
    elif " >= " in alert_query:
        return "ge"
    return "gt"


def get_grafana_datasource_config() -> str:
    """Get Grafana datasource configuration for Prometheus."""
    return """apiVersion: 1

datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://localhost:9090
    isDefault: true
    editable: true
    basicAuth: false
    withCredentials: false
    jsonData:
      httpMethod: GET
      timeInterval: "5s"
"""


def get_grafana_datasource_config_cluster() -> str:
    """Get Grafana datasource configuration for cluster deployment."""
    return """apiVersion: 1

datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://prometheus-service:9090
    isDefault: true
    editable: true
    basicAuth: false
    withCredentials: false
    jsonData:
      httpMethod: GET
      timeInterval: "5s"
"""


def get_grafana_dashboard_provisioning_config() -> str:
    """Get Grafana dashboard provisioning configuration."""
    return """apiVersion: 1

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
"""


def get_grafana_redirect_html() -> str:
    """Get HTML page that redirects to the dashboard."""
    return """<!DOCTYPE html>
<html>
<head>
    <title>Network Stress Test Dashboard</title>
    <meta http-equiv="refresh" content="2;url=/d/broadcast-network-stress-test/broadcast-network-stress-test">
    <style>
        body { 
            font-family: Arial, sans-serif; 
            text-align: center; 
            margin-top: 50px; 
            background-color: #1f1f1f; 
            color: #fff; 
        }
        .loading { 
            font-size: 18px; 
            margin: 20px; 
        }
        .dashboard-link { 
            color: #73bf69; 
            text-decoration: none; 
            font-weight: bold; 
        }
    </style>
</head>
<body>
    <h1>🚀 Network Stress Test Dashboard</h1>
    <div class="loading">Loading dashboard...</div>
    <p>If you're not redirected automatically, 
       <a href="/d/broadcast-network-stress-test/broadcast-network-stress-test" class="dashboard-link">click here</a>
    </p>
</body>
</html>"""


def get_grafana_preferences_json() -> str:
    """Get Grafana preferences to set the default home dashboard."""
    return """{
  "homeDashboardUID": "broadcast-network-stress-test",
  "theme": "dark",
  "timezone": "browser"
}"""


def get_grafana_config() -> str:
    """Get Grafana configuration settings."""
    return """[analytics]
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
"""
