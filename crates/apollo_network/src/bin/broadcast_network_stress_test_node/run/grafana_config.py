"""
Grafana dashboard configuration for the broadcast network stress test.
Contains static dashboard JSON and helper functions - ENHANCED WITH STATS ROW AND COMPARISONS.
"""


def get_grafana_dashboard_json() -> str:
    """Generate enhanced Grafana dashboard with stats overview and theoretical vs actual comparisons."""
    return """{
  "id": 1,
  "uid": "broadcast-network-stress-test",
  "title": "Broadcast Network Stress Test - Enhanced Dashboard",
  "tags": ["network", "stress-test", "apollo"],
  "timezone": "browser",
  "panels": [
    {
      "id": 200,
      "title": "📊 Key Metrics Overview",
      "type": "row",
      "gridPos": {"h": 1, "w": 24, "x": 0, "y": 0},
      "collapsed": false
    },
    {
      "id": 201,
      "title": "Connected Peers",
      "type": "stat",
      "targets": [
        {
          "expr": "network_connected_peers",
          "legendFormat": "Connected Peers"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "thresholds": {
            "steps": [
              {"color": "red", "value": null},
              {"color": "green", "value": 1}
            ]
          }
        }
      },
      "gridPos": {"h": 6, "w": 4, "x": 0, "y": 1}
    },
    {
      "id": 202,
      "title": "Message Throughput",
      "type": "stat",
      "targets": [
        {
          "expr": "broadcast_message_theoretical_throughput",
          "legendFormat": "Theoretical"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "Bps"
        }
      },
      "gridPos": {"h": 6, "w": 4, "x": 4, "y": 1}
    },
    {
      "id": 203,
      "title": "Avg Latency",
      "type": "stat",
      "targets": [
        {
          "expr": "rate(receive_message_delay_seconds_sum[1m]) / rate(receive_message_delay_seconds_count[1m])",
          "legendFormat": "Avg Delay"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "s",
          "thresholds": {
            "steps": [
              {"color": "green", "value": null},
              {"color": "yellow", "value": 0.1},
              {"color": "red", "value": 1.0}
            ]
          }
        }
      },
      "gridPos": {"h": 6, "w": 4, "x": 8, "y": 1}
    },
    {
      "id": 204,
      "title": "CPU Usage",
      "type": "stat",
      "targets": [
        {
          "expr": "system_process_cpu_usage_percent",
          "legendFormat": "CPU %"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "percent",
          "thresholds": {
            "steps": [
              {"color": "green", "value": null},
              {"color": "yellow", "value": 70},
              {"color": "red", "value": 90}
            ]
          }
        }
      },
      "gridPos": {"h": 6, "w": 4, "x": 12, "y": 1}
    },
    {
      "id": 205,
      "title": "Memory Usage",
      "type": "stat",
      "targets": [
        {
          "expr": "system_process_memory_usage_bytes",
          "legendFormat": "Memory"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "bytes"
        }
      },
      "gridPos": {"h": 6, "w": 4, "x": 16, "y": 1}
    },
    {
      "id": 206,
      "title": "Negative Delays",
      "type": "stat",
      "targets": [
        {
          "expr": "rate(receive_message_negative_delay_seconds_count[1m])",
          "legendFormat": "Neg Delays/sec"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "thresholds": {
            "steps": [
              {"color": "green", "value": null},
              {"color": "red", "value": 0.001}
            ]
          }
        }
      },
      "gridPos": {"h": 6, "w": 4, "x": 20, "y": 1}
    },
    {
      "id": 300,
      "title": "🔍 Theoretical vs Actual Performance",
      "type": "row",
      "gridPos": {"h": 1, "w": 24, "x": 0, "y": 7},
      "collapsed": false
    },
    {
      "id": 301,
      "title": "Throughput Comparison",
      "type": "timeseries",
      "targets": [
        {
          "expr": "broadcast_message_theoretical_throughput",
          "legendFormat": "Theoretical Throughput"
        },
        {
          "expr": "rate(receive_message_bytes_sum[20s])",
          "legendFormat": "Actual Receive Throughput"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "Bps"
        }
      },
      "gridPos": {"h": 8, "w": 8, "x": 0, "y": 8}
    },
    {
      "id": 302,
      "title": "Message Rate Comparison",
      "type": "timeseries",
      "targets": [
        {
          "expr": "1000 / broadcast_message_theoretical_heartbeat_millis",
          "legendFormat": "Theoretical TPS"
        },
        {
          "expr": "rate(broadcast_message_count[20s])",
          "legendFormat": "Actual Broadcast TPS"
        },
        {
          "expr": "rate(receive_message_count[20s])",
          "legendFormat": "Actual Receive TPS"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "ops"
        }
      },
      "gridPos": {"h": 8, "w": 8, "x": 8, "y": 8}
    },
    {
      "id": 303,
      "title": "Message Size Comparison",
      "type": "timeseries",
      "targets": [
        {
          "expr": "broadcast_message_bytes",
          "legendFormat": "Theoretical Message Size"
        },
        {
          "expr": "receive_message_bytes",
          "legendFormat": "Actual Received Size"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "bytes"
        }
      },
      "gridPos": {"h": 8, "w": 8, "x": 16, "y": 8}
    },
    {
      "id": 100,
      "title": "📤 Broadcast Message Metrics",
      "type": "row",
      "gridPos": {"h": 1, "w": 24, "x": 0, "y": 16},
      "collapsed": false
    },
    {
      "id": 1,
      "title": "Message Throughput Timeseries",
      "type": "timeseries",
      "targets": [
        {
          "expr": "broadcast_message_theoretical_throughput",
          "legendFormat": "Theoretical Throughput"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "Bps"
        }
      },
      "gridPos": {"h": 8, "w": 6, "x": 0, "y": 17}
    },
    {
      "id": 2,
      "title": "Message Size Timeseries",
      "type": "timeseries",
      "targets": [
        {
          "expr": "broadcast_message_bytes",
          "legendFormat": "Message Size (bytes)"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "bytes"
        }
      },
      "gridPos": {"h": 8, "w": 6, "x": 6, "y": 17}
    },
    {
      "id": 3,
      "title": "Heartbeat Interval",
      "type": "timeseries",
      "targets": [
        {
          "expr": "broadcast_message_theoretical_heartbeat_millis",
          "legendFormat": "Heartbeat Interval (ms)"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "ms"
        }
      },
      "gridPos": {"h": 8, "w": 6, "x": 12, "y": 17}
    },
    {
      "id": 4,
      "title": "Broadcast Rates",
      "type": "timeseries",
      "targets": [
        {
          "expr": "rate(broadcast_message_count[20s])",
          "legendFormat": "Messages/sec"
        },
        {
          "expr": "rate(broadcast_message_bytes_sum[20s])",
          "legendFormat": "Bytes/sec"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "ops"
        }
      },
      "gridPos": {"h": 8, "w": 6, "x": 18, "y": 17}
    },
    {
      "id": 101,
      "title": "📥 Receive Message & Latency Metrics",
      "type": "row",
      "gridPos": {"h": 1, "w": 24, "x": 0, "y": 25},
      "collapsed": false
    },
    {
      "id": 5,
      "title": "Message Latency Quantiles",
      "type": "timeseries",
      "targets": [
        {
          "expr": "histogram_quantile(0.5, rate(receive_message_delay_seconds_bucket[1m]))",
          "legendFormat": "50th Percentile (Median)"
        },
        {
          "expr": "histogram_quantile(0.95, rate(receive_message_delay_seconds_bucket[1m]))",
          "legendFormat": "95th Percentile"
        },
        {
          "expr": "histogram_quantile(0.99, rate(receive_message_delay_seconds_bucket[1m]))",
          "legendFormat": "99th Percentile"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "s",
          "thresholds": {
            "steps": [
              {"color": "green", "value": null},
              {"color": "yellow", "value": 0.1},
              {"color": "red", "value": 1.0}
            ]
          }
        }
      },
      "gridPos": {"h": 8, "w": 8, "x": 0, "y": 26}
    },
    {
      "id": 6,
      "title": "Histogram Quantiles (0.5 & 0.99)",
      "type": "timeseries",
      "targets": [
        {
          "expr": "receive_message_delay_seconds{quantile=\\\"0.5\\\"}",
          "legendFormat": "Direct 0.5 Quantile"
        },
        {
          "expr": "receive_message_delay_seconds{quantile=\\\"0.99\\\"}",
          "legendFormat": "Direct 0.99 Quantile"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "s"
        }
      },
      "gridPos": {"h": 8, "w": 8, "x": 8, "y": 26}
    },
    {
      "id": 7,
      "title": "Negative Delay Monitoring",
      "type": "timeseries",
      "targets": [
        {
          "expr": "rate(receive_message_negative_delay_seconds_count[1m])",
          "legendFormat": "Negative Delays/sec"
        },
        {
          "expr": "histogram_quantile(0.5, rate(receive_message_negative_delay_seconds_bucket[1m]))",
          "legendFormat": "Median Negative Delay"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "s",
          "thresholds": {
            "steps": [
              {"color": "green", "value": null},
              {"color": "yellow", "value": 0.001},
              {"color": "red", "value": 0.01}
            ]
          }
        }
      },
      "gridPos": {"h": 8, "w": 8, "x": 16, "y": 26}
    },
    {
      "id": 8,
      "title": "Average Positive Delay",
      "type": "timeseries",
      "targets": [
        {
          "expr": "rate(receive_message_delay_seconds_sum[1m]) / rate(receive_message_delay_seconds_count[1m])",
          "legendFormat": "Average Positive Delay"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "s",
          "thresholds": {
            "steps": [
              {"color": "green", "value": null},
              {"color": "yellow", "value": 0.05},
              {"color": "red", "value": 0.2}
            ]
          }
        }
      },
      "gridPos": {"h": 8, "w": 12, "x": 0, "y": 34}
    },
    {
      "id": 9,
      "title": "Receive Rate",
      "type": "timeseries",
      "targets": [
        {
          "expr": "rate(receive_message_count[20s])",
          "legendFormat": "Receive Rate"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "ops"
        }
      },
      "gridPos": {"h": 8, "w": 12, "x": 12, "y": 34}
    },
    {
      "id": 102,
      "title": "🌐 Network Connection Metrics",
      "type": "row",
      "gridPos": {"h": 1, "w": 24, "x": 0, "y": 42},
      "collapsed": false
    },
    {
      "id": 10,
      "title": "Connected Peers Timeseries",
      "type": "timeseries",
      "targets": [
        {
          "expr": "network_connected_peers",
          "legendFormat": "Connected Peers"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "thresholds": {
            "steps": [
              {"color": "red", "value": null},
              {"color": "green", "value": 1}
            ]
          }
        }
      },
      "gridPos": {"h": 8, "w": 8, "x": 0, "y": 43}
    },
    {
      "id": 11,
      "title": "Network Sessions",
      "type": "timeseries",
      "targets": [
        {
          "expr": "network_active_inbound_sessions",
          "legendFormat": "Inbound Sessions"
        },
        {
          "expr": "network_active_outbound_sessions",
          "legendFormat": "Outbound Sessions"
        }
      ],
      "gridPos": {"h": 8, "w": 8, "x": 8, "y": 43}
    },
    {
      "id": 12,
      "title": "Network Messages",
      "type": "timeseries",
      "targets": [
        {
          "expr": "rate(network_stress_test_sent_messages[1m])",
          "legendFormat": "Sent Messages/sec"
        },
        {
          "expr": "rate(network_stress_test_received_messages[1m])",
          "legendFormat": "Received Messages/sec"
        }
      ],
      "gridPos": {"h": 8, "w": 8, "x": 16, "y": 43}
    },
    {
      "id": 103,
      "title": "💻 System Resource Metrics",
      "type": "row",
      "gridPos": {"h": 1, "w": 24, "x": 0, "y": 51},
      "collapsed": false
    },
    {
      "id": 13,
      "title": "System CPU Usage Timeseries",
      "type": "timeseries",
      "targets": [
        {
          "expr": "system_process_cpu_usage_percent",
          "legendFormat": "CPU Usage %"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "percent",
          "thresholds": {
            "steps": [
              {"color": "green", "value": null},
              {"color": "yellow", "value": 70},
              {"color": "red", "value": 90}
            ]
          }
        }
      },
      "gridPos": {"h": 8, "w": 12, "x": 0, "y": 52}
    },
    {
      "id": 14,
      "title": "System Memory Usage Timeseries",
      "type": "timeseries",
      "targets": [
        {
          "expr": "system_process_memory_usage_bytes",
          "legendFormat": "Memory Usage"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "bytes"
        }
      },
      "gridPos": {"h": 8, "w": 12, "x": 12, "y": 52}
    },
    {
      "id": 19,
      "title": "System Network Throughput",
      "type": "timeseries",
      "targets": [
        {
          "expr": "rate(system_network_bytes_sent_total[1m])",
          "legendFormat": "Bytes Sent/sec"
        },
        {
          "expr": "rate(system_network_bytes_received_total[1m])",
          "legendFormat": "Bytes Received/sec"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "Bps",
          "thresholds": {
            "steps": [
              {"color": "green", "value": null},
              {"color": "yellow", "value": 1000000},
              {"color": "red", "value": 10000000}
            ]
          }
        }
      },
      "gridPos": {"h": 8, "w": 12, "x": 0, "y": 60}
    },
    {
      "id": 20,
      "title": "System Network Current Usage",
      "type": "timeseries",
      "targets": [
        {
          "expr": "system_network_bytes_sent_current",
          "legendFormat": "Current Bytes Sent"
        },
        {
          "expr": "system_network_bytes_received_current",
          "legendFormat": "Current Bytes Received"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "unit": "bytes",
          "thresholds": {
            "steps": [
              {"color": "green", "value": null},
              {"color": "yellow", "value": 1000000},
              {"color": "red", "value": 10000000}
            ]
          }
        }
      },
      "gridPos": {"h": 8, "w": 12, "x": 12, "y": 60}
    },
    {
      "id": 104,
      "title": "🚨 Alerts & Health Monitoring",
      "type": "row",
      "gridPos": {"h": 1, "w": 24, "x": 0, "y": 68},
      "collapsed": false
    },
    {
      "id": 15,
      "title": "Network Connectivity Alert",
      "type": "stat",
      "targets": [
        {
          "expr": "min(network_connected_peers)",
          "legendFormat": "Min Connected Peers"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "thresholds": {
            "steps": [
              {"color": "red", "value": null},
              {"color": "yellow", "value": 1},
              {"color": "green", "value": 2}
            ]
          }
        }
      },
      "options": {
        "colorMode": "background",
        "graphMode": "none",
        "justifyMode": "center"
      },
      "gridPos": {"h": 6, "w": 8, "x": 0, "y": 69}
    },
    {
      "id": 16,
      "title": "Negative Delay Alert",
      "type": "stat",
      "targets": [
        {
          "expr": "rate(receive_message_negative_delay_seconds_count[1m])",
          "legendFormat": "Negative Delays/sec"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "thresholds": {
            "steps": [
              {"color": "green", "value": null},
              {"color": "red", "value": 0.001}
            ]
          }
        }
      },
      "options": {
        "colorMode": "background",
        "graphMode": "none",
        "justifyMode": "center"
      },
      "gridPos": {"h": 6, "w": 8, "x": 8, "y": 69}
    },
    {
      "id": 17,
      "title": "All Nodes Connected",
      "type": "stat",
      "targets": [
        {
          "expr": "count(network_connected_peers >= on() group_left() (count(network_connected_peers) - 1))",
          "legendFormat": "Fully Connected Nodes"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "thresholds": {
            "steps": [
              {"color": "red", "value": null},
              {"color": "green", "value": 1}
            ]
          }
        }
      },
      "options": {
        "colorMode": "background",
        "graphMode": "none",
        "justifyMode": "center"
      },
      "gridPos": {"h": 6, "w": 8, "x": 16, "y": 69}
    },
    {
      "id": 18,
      "title": "Network Errors",
      "type": "timeseries",
      "targets": [
        {
          "expr": "rate(receive_messages_missing_total[1m])",
          "legendFormat": "Missing Messages/sec"
        },
        {
          "expr": "rate(receive_messages_out_of_order_total[1m])",
          "legendFormat": "Out of Order Messages/sec"
        },
        {
          "expr": "rate(receive_messages_duplicate_total[1m])",
          "legendFormat": "Duplicate Messages/sec"
        }
      ],
      "fieldConfig": {
        "defaults": {
          "thresholds": {
            "steps": [
              {"color": "green", "value": null},
              {"color": "yellow", "value": 1},
              {"color": "red", "value": 10}
            ]
          }
        }
      },
      "gridPos": {"h": 8, "w": 24, "x": 0, "y": 75}
    }
  ],
  "time": {
    "from": "now-15m",
    "to": "now"
  },
  "refresh": "5s"
}"""


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
"""


def get_grafana_alerting_rules() -> str:
    """Get Grafana alerting rules for network connectivity."""
    return """{
  "groups": [
    {
      "name": "network_connectivity",
      "orgId": 1,
      "folder": "alerts",
      "rules": [
        {
          "uid": "network_connectivity_alert",
          "title": "Network Connectivity Alert",
          "condition": "B",
          "data": [
            {
              "refId": "A",
              "queryType": "",
              "relativeTimeRange": {
                "from": 300,
                "to": 0
              },
              "model": {
                "expr": "min(network_connected_peers)",
                "interval": "",
                "refId": "A"
              }
            },
            {
              "refId": "B",
              "queryType": "",
              "relativeTimeRange": {
                "from": 0,
                "to": 0
              },
              "model": {
                "conditions": [
                  {
                    "evaluator": {
                      "params": [1],
                      "type": "lt"
                    },
                    "operator": {
                      "type": "and"
                    },
                    "query": {
                      "params": ["A"]
                    },
                    "reducer": {
                      "params": [],
                      "type": "last"
                    },
                    "type": "query"
                  }
                ],
                "refId": "B"
              }
            }
          ],
          "noDataState": "NoData",
          "execErrState": "Alerting",
          "for": "30s",
          "annotations": {
            "description": "One or more nodes have insufficient peer connections. Minimum connected peers: {{ $value }}",
            "runbook_url": "",
            "summary": "Network connectivity issue detected"
          },
          "labels": {
            "severity": "critical",
            "team": "network"
          }
        },
        {
          "uid": "negative_delay_alert",
          "title": "Negative Message Delay Alert",
          "condition": "B",
          "data": [
            {
              "refId": "A",
              "queryType": "",
              "relativeTimeRange": {
                "from": 300,
                "to": 0
              },
              "model": {
                "expr": "rate(receive_message_negative_delay_seconds_count[1m])",
                "interval": "",
                "refId": "A"
              }
            },
            {
              "refId": "B",
              "queryType": "",
              "relativeTimeRange": {
                "from": 0,
                "to": 0
              },
              "model": {
                "conditions": [
                  {
                    "evaluator": {
                      "params": [0.001],
                      "type": "gt"
                    },
                    "operator": {
                      "type": "and"
                    },
                    "query": {
                      "params": ["A"]
                    },
                    "reducer": {
                      "params": [],
                      "type": "last"
                    },
                    "type": "query"
                  }
                ],
                "refId": "B"
              }
            }
          ],
          "noDataState": "NoData",
          "execErrState": "Alerting",
          "for": "30s",
          "annotations": {
            "description": "Negative message delays detected! This indicates clock synchronization issues between nodes. Rate: {{ $value }} negative delays/sec",
            "runbook_url": "",
            "summary": "Clock synchronization issue - negative message delays detected"
          },
          "labels": {
            "severity": "critical",
            "team": "network"
          }
        }
      ]
    }
  ]
}"""
