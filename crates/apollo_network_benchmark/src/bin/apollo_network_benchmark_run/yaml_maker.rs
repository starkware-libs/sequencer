use serde_json::{json, Value};

use crate::args::{get_env_vars, SharedArgs};
use crate::mod_utils::{get_peer_id_from_node_id, make_multi_address};

pub const PROMETHEUS_SERVICE_NAME: &str = "prometheus-service";

fn generate_json(data: Value) -> Result<String, String> {
    serde_json::to_string_pretty(&data).map_err(|e| format!("Failed to generate JSON: {}", e))
}

fn get_prometheus_config_data(
    self_scrape: bool,
    metric_urls: &[String],
    scrape_seconds: u32,
) -> Value {
    let mut scrape_configs = vec![];

    if self_scrape {
        scrape_configs.push(json!({
            "job_name": "prometheus",
            "static_configs": [{"targets": ["localhost:9090"]}]
        }));
    }

    for (i, url) in metric_urls.iter().enumerate() {
        scrape_configs.push(json!({
            "job_name": format!("broadcast_network_stress_test_{}", i),
            "static_configs": [{
                "targets": [url],
                "labels": {
                    "application": "broadcast_network_stress_test_node",
                    "environment": "test"
                }
            }]
        }));
    }

    json!({
        "global": {"scrape_interval": format!("{}s", scrape_seconds)},
        "scrape_configs": scrape_configs
    })
}

pub fn get_prometheus_config(self_scrape: bool, metric_urls: &[String]) -> Result<String, String> {
    let config_data = get_prometheus_config_data(self_scrape, metric_urls, 1);
    generate_json(config_data)
}

pub fn get_prometheus_json_file(num_nodes: u32) -> Result<String, String> {
    let urls: Vec<String> = (0..num_nodes)
        .map(|i| {
            format!(
                "broadcast-network-stress-test-{}.broadcast-network-stress-test-headless:2000",
                i
            )
        })
        .collect();

    let config_data = get_prometheus_config_data(false, &urls, 5);

    // Convert to YAML string (we'll use serde_yaml for this)
    let config_yaml = serde_yaml::to_string(&config_data)
        .map_err(|e| format!("Failed to convert to YAML: {}", e))?;

    let data = json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {"name": "prometheus-config"},
        "data": {"prometheus.yml": config_yaml}
    });

    generate_json(data)
}

pub fn get_prometheus_deployment_json_file() -> Result<String, String> {
    let data = json!({
        "apiVersion": "apps/v1",
        "kind": "StatefulSet",
        "metadata": {"name": "prometheus"},
        "spec": {
            "replicas": 1,
            "serviceName": "prometheus-headless",
            "selector": {"matchLabels": {"app": "prometheus"}},
            "template": {
                "metadata": {"labels": {"app": "prometheus"}},
                "spec": {
                    "securityContext": {
                        "fsGroup": 65534,
                        "runAsUser": 65534,
                        "runAsGroup": 65534,
                        "runAsNonRoot": true
                    },
                    "containers": [{
                        "name": "prometheus",
                        "image": "registry.hub.docker.com/prom/prometheus:main",
                        "imagePullPolicy": "Always",
                        "ports": [{"containerPort": 9090}],
                        "securityContext": {
                            "allowPrivilegeEscalation": false,
                            "readOnlyRootFilesystem": true,
                            "capabilities": {"drop": ["ALL"]}
                        },
                        "volumeMounts": [
                            {"name": "config-volume", "mountPath": "/etc/prometheus"},
                            {"name": "prometheus-data", "mountPath": "/prometheus"},
                            {"name": "tmp-volume", "mountPath": "/tmp"}
                        ],
                        "args": [
                            "--config.file=/etc/prometheus/prometheus.yml",
                            "--storage.tsdb.path=/prometheus",
                            "--web.console.libraries=/etc/prometheus/console_libraries",
                            "--web.console.templates=/etc/prometheus/consoles",
                            "--storage.tsdb.retention.time=15d",
                            "--web.enable-lifecycle",
                            "--web.listen-address=0.0.0.0:9090"
                        ]
                    }],
                    "volumes": [
                        {"name": "config-volume", "configMap": {"name": "prometheus-config"}},
                        {"name": "tmp-volume", "emptyDir": {}}
                    ]
                }
            },
            "volumeClaimTemplates": [{
                "metadata": {"name": "prometheus-data"},
                "spec": {
                    "accessModes": ["ReadWriteOnce"],
                    "resources": {"requests": {"storage": "16Gi"}}
                }
            }]
        }
    });

    generate_json(data)
}

pub fn get_prometheus_service_json_file() -> Result<String, String> {
    let data = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {"name": PROMETHEUS_SERVICE_NAME},
        "spec": {
            "selector": {"app": "prometheus"},
            "ports": [{"port": 9090, "targetPort": 9090}],
            "type": "ClusterIP"
        }
    });

    generate_json(data)
}

pub fn get_prometheus_headless_service_json_file() -> Result<String, String> {
    let data = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {"name": "prometheus-headless"},
        "spec": {
            "selector": {"app": "prometheus"},
            "ports": [{"port": 9090, "targetPort": 9090}],
            "clusterIP": "None"
        }
    });

    generate_json(data)
}

#[allow(clippy::too_many_arguments)]
pub fn get_network_stress_test_deployment_json_file(
    image_tag: &str,
    args: &SharedArgs,
    cpu_requests: &str,
    memory_requests: &str,
    cpu_limits: &str,
    memory_limits: &str,
    node_pool_role: &str,
    latency: Option<u32>,
    throughput: Option<u32>,
    node_toleration: Option<&str>,
) -> Result<String, String> {
    let bootstrap_nodes: Vec<String> = (0..args.num_nodes)
        .map(|j| {
            let peer_id = get_peer_id_from_node_id(j)?;
            Ok(make_multi_address(
                &format!(
                    "/dns/broadcast-network-stress-test-{}.broadcast-network-stress-test-headless",
                    j
                ),
                10000,
                &peer_id,
                args.quic,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let env_vars = get_env_vars(None, 2000, 10000, &bootstrap_nodes, args, latency, throughput);

    let mut pod_spec = json!({
        "subdomain": "broadcast-network-stress-test-headless",
        "restartPolicy": "Never",
        "setHostnameAsFQDN": false,
        "containers": [{
            "name": "broadcast-network-stress-test",
            "image": image_tag,
            "securityContext": {"privileged": true},
            "ports": [
                {"containerPort": 2000, "name": "metrics"},
                {"containerPort": 10000, "protocol": "UDP", "name": "p2p-udp"},
                {"containerPort": 10000, "protocol": "TCP", "name": "p2p-tcp"}
            ],
            "env": env_vars,
            "resources": {
                "limits": {
                    "memory": memory_limits,
                    "cpu": cpu_limits
                },
                "requests": {
                    "memory": memory_requests,
                    "cpu": cpu_requests
                }
            }
        }],
        "nodeSelector": {"cloud.google.com/gke-nodepool": node_pool_role}
    });

    // Add toleration if specified, or use default for apollo-network-benchmark node pool
    let toleration_key_value = if let Some(toleration_str) = node_toleration {
        let parts: Vec<&str> = toleration_str.split('=').collect();
        if parts.len() != 2 {
            return Err(format!(
                "Invalid toleration format '{}'. Expected format: key=value",
                toleration_str
            ));
        }
        Some((parts[0], parts[1]))
    } else if node_pool_role == "apollo-network-benchmark" {
        // Use default toleration for the default apollo-network-benchmark node pool
        Some(("workload", "apollo-network-benchmark"))
    } else {
        None
    };

    if let Some((key, value)) = toleration_key_value {
        pod_spec["tolerations"] = json!([{
            "key": key,
            "operator": "Equal",
            "value": value,
            "effect": "NoSchedule"
        }]);
    }

    let data = json!({
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {"name": "broadcast-network-stress-test"},
        "spec": {
            "completionMode": "Indexed",
            "completions": args.num_nodes,
            "parallelism": args.num_nodes,
            "template": {
                "metadata": {"labels": {"app": "broadcast-network-stress-test"}},
                "spec": pod_spec
            }
        }
    });

    generate_json(data)
}

pub fn get_network_stress_test_headless_service_json_file() -> Result<String, String> {
    let data = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {"name": "broadcast-network-stress-test-headless"},
        "spec": {
            "clusterIP": "None",
            "selector": {"app": "broadcast-network-stress-test"},
            "ports": [
                {"port": 2000, "targetPort": 2000, "name": "metrics"},
                {"port": 10000, "targetPort": 10000, "protocol": "UDP", "name": "p2p-udp"},
                {"port": 10000, "targetPort": 10000, "protocol": "TCP", "name": "p2p-tcp"}
            ]
        }
    });

    generate_json(data)
}

pub fn get_grafana_configmap_json_file() -> Result<String, String> {
    use crate::grafana_config::*;

    let dashboard_json = get_grafana_dashboard_json("30s");
    let datasource_config = get_grafana_datasource_config("http://prometheus-service:9090");
    let dashboard_config = get_grafana_dashboard_provisioning_config();
    let grafana_config = get_grafana_config();
    let preferences_json = get_grafana_preferences_json();

    let data = json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {"name": "grafana-config"},
        "data": {
            "dashboard.json": dashboard_json,
            "datasource.yml": datasource_config,
            "dashboard_config.yml": dashboard_config,
            "grafana.ini": grafana_config,
            "preferences.json": preferences_json
        }
    });

    generate_json(data)
}

pub fn get_grafana_deployment_json_file() -> Result<String, String> {
    let data = json!({
        "apiVersion": "apps/v1",
        "kind": "StatefulSet",
        "metadata": {"name": "grafana"},
        "spec": {
            "replicas": 1,
            "serviceName": "grafana-headless",
            "selector": {"matchLabels": {"app": "grafana"}},
            "template": {
                "metadata": {"labels": {"app": "grafana"}},
                "spec": {
                    "securityContext": {
                        "fsGroup": 472,
                        "runAsUser": 472,
                        "runAsGroup": 472,
                        "runAsNonRoot": true
                    },
                    "containers": [{
                        "name": "grafana",
                        "image": "grafana/grafana:latest",
                        "imagePullPolicy": "Always",
                        "ports": [{"containerPort": 3000}],
                        "env": [{
                            "name": "GF_PATHS_CONFIG",
                            "value": "/etc/grafana/grafana.ini"
                        }],
                        "securityContext": {
                            "allowPrivilegeEscalation": false,
                            "readOnlyRootFilesystem": false,
                            "capabilities": {"drop": ["ALL"]}
                        },
                        "volumeMounts": [
                            {"name": "grafana-config", "mountPath": "/etc/grafana/grafana.ini", "subPath": "grafana.ini"},
                            {"name": "grafana-config", "mountPath": "/etc/grafana/provisioning/datasources/datasource.yml", "subPath": "datasource.yml"},
                            {"name": "grafana-config", "mountPath": "/etc/grafana/provisioning/dashboards/dashboard_config.yml", "subPath": "dashboard_config.yml"},
                            {"name": "grafana-config", "mountPath": "/etc/grafana/provisioning/dashboards/dashboard.json", "subPath": "dashboard.json"},
                            {"name": "grafana-data", "mountPath": "/var/lib/grafana"}
                        ]
                    }],
                    "volumes": [{
                        "name": "grafana-config",
                        "configMap": {"name": "grafana-config"}
                    }]
                }
            },
            "volumeClaimTemplates": [{
                "metadata": {"name": "grafana-data"},
                "spec": {
                    "accessModes": ["ReadWriteOnce"],
                    "resources": {"requests": {"storage": "8Gi"}}
                }
            }]
        }
    });

    generate_json(data)
}

pub fn get_grafana_service_json_file() -> Result<String, String> {
    let data = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {"name": "grafana-service"},
        "spec": {
            "selector": {"app": "grafana"},
            "ports": [{"port": 3000, "targetPort": 3000}],
            "type": "ClusterIP"
        }
    });

    generate_json(data)
}

pub fn get_grafana_headless_service_json_file() -> Result<String, String> {
    let data = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {"name": "grafana-headless"},
        "spec": {
            "selector": {"app": "grafana"},
            "ports": [{"port": 3000, "targetPort": 3000}],
            "clusterIP": "None"
        }
    });

    generate_json(data)
}
