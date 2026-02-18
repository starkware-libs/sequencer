use anyhow::Context;
use apollo_network_benchmark::peer_key::peer_id_from_node_id;
use serde_json::{json, Value};

use crate::args::{get_k8s_env_vars, SharedArgs};
use crate::mod_utils::make_multi_address;

pub const PROMETHEUS_DEPLOYMENT_JSON: &str =
    include_str!("../../../config/k8s_prometheus_deployment.json");
pub const PROMETHEUS_SERVICE_JSON: &str =
    include_str!("../../../config/k8s_prometheus_service.json");
pub const PROMETHEUS_HEADLESS_SERVICE_JSON: &str =
    include_str!("../../../config/k8s_prometheus_headless_service.json");
pub const STRESS_TEST_HEADLESS_SERVICE_JSON: &str =
    include_str!("../../../config/k8s_stress_test_headless_service.json");
pub const GRAFANA_DEPLOYMENT_JSON: &str =
    include_str!("../../../config/k8s_grafana_deployment.json");
pub const GRAFANA_SERVICE_JSON: &str = include_str!("../../../config/k8s_grafana_service.json");
pub const GRAFANA_HEADLESS_SERVICE_JSON: &str =
    include_str!("../../../config/k8s_grafana_headless_service.json");

fn generate_json(data: Value) -> anyhow::Result<String> {
    serde_json::to_string_pretty(&data).context("Failed to generate JSON")
}

fn get_prometheus_config_data(
    self_scrape: bool,
    metric_urls: &[String],
    scrape_seconds: u32,
    node_exporter_url: Option<&str>,
) -> Value {
    let mut scrape_configs = vec![];

    if self_scrape {
        scrape_configs.push(json!({
            "job_name": "prometheus",
            "static_configs": [{"targets": ["localhost:9090"]}]
        }));
    }

    if let Some(url) = node_exporter_url {
        scrape_configs.push(json!({
            "job_name": "node_exporter",
            "static_configs": [{"targets": [url]}]
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

pub fn get_prometheus_config(
    self_scrape: bool,
    metric_urls: &[String],
    node_exporter_url: Option<&str>,
) -> anyhow::Result<String> {
    let config_data = get_prometheus_config_data(self_scrape, metric_urls, 1, node_exporter_url);
    generate_json(config_data)
}

pub fn get_prometheus_json_file(num_nodes: u32) -> anyhow::Result<String> {
    let urls: Vec<String> = (0..num_nodes)
        .map(|i| {
            format!(
                "broadcast-network-stress-test-{}.broadcast-network-stress-test-headless:2000",
                i
            )
        })
        .collect();

    let config_data = get_prometheus_config_data(false, &urls, 5, None);

    let config_yaml =
        serde_json::to_string_pretty(&config_data).context("Failed to convert config to string")?;

    let data = json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {"name": "prometheus-config"},
        "data": {"prometheus.yml": config_yaml}
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
) -> anyhow::Result<String> {
    let bootstrap_nodes: Vec<String> = (0..args.num_nodes)
        .map(|j| {
            let peer_id = peer_id_from_node_id(u64::from(j))?;
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
        .collect::<anyhow::Result<Vec<_>>>()?;

    let env_vars = get_k8s_env_vars(None, 2000, 10000, &bootstrap_nodes, args, latency, throughput);

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

    let toleration_key_value = if let Some(toleration_str) = node_toleration {
        let parts: Vec<&str> = toleration_str.split('=').collect();
        anyhow::ensure!(
            parts.len() == 2,
            "Invalid toleration format '{}'. Expected format: key=value",
            toleration_str
        );
        Some((parts[0], parts[1]))
    } else if node_pool_role == "apollo-network-benchmark" {
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

pub fn get_grafana_configmap_json_file() -> anyhow::Result<String> {
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
