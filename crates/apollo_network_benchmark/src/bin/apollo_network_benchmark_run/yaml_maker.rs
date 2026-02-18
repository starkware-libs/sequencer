use anyhow::Context;
use apollo_network_benchmark::peer_key::peer_id_from_node_id;
use serde_json::{json, Value};

use crate::args::{
    get_k8s_env_vars,
    NetworkControls,
    ResourceLimits,
    SharedArgs,
    METRIC_PORT_BASE,
    P2P_PORT_BASE,
    STRESS_TEST_NAME,
};
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
    cadvisor_url: Option<&str>,
    kubernetes_cadvisor: bool,
) -> Value {
    let mut scrape_configs = vec![];

    if self_scrape {
        scrape_configs.push(json!({
            "job_name": "prometheus",
            "static_configs": [{"targets": ["localhost:9090"]}]
        }));
    }

    if let Some(url) = cadvisor_url {
        scrape_configs.push(json!({
            "job_name": "cadvisor",
            "static_configs": [{"targets": [url]}],
            "metric_relabel_configs": [{
                "source_labels": ["name"],
                "target_label": "pod"
            }]
        }));
    }

    if kubernetes_cadvisor {
        scrape_configs.push(json!({
            "job_name": "kubelet-cadvisor",
            "scheme": "https",
            "tls_config": {
                "ca_file": "/var/run/secrets/kubernetes.io/serviceaccount/ca.crt"
            },
            "bearer_token_file": "/var/run/secrets/kubernetes.io/serviceaccount/token",
            "kubernetes_sd_configs": [{"role": "node"}],
            "relabel_configs": [
                {
                    "target_label": "__address__",
                    "replacement": "kubernetes.default.svc:443"
                },
                {
                    "source_labels": ["__meta_kubernetes_node_name"],
                    "target_label": "__metrics_path__",
                    "replacement": "/api/v1/nodes/$1/proxy/metrics/cadvisor"
                }
            ]
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
    cadvisor_url: Option<&str>,
) -> anyhow::Result<String> {
    let config_data = get_prometheus_config_data(self_scrape, metric_urls, 1, cadvisor_url, false);
    generate_json(config_data)
}

pub fn get_prometheus_json_file(num_nodes: u32) -> anyhow::Result<String> {
    let urls: Vec<String> = (0..num_nodes)
        .map(|i| format!("{STRESS_TEST_NAME}-{i}.{STRESS_TEST_NAME}-headless:{METRIC_PORT_BASE}",))
        .collect();

    let config_data = get_prometheus_config_data(false, &urls, 5, None, true);

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

pub fn get_network_stress_test_deployment_json_file(
    image_tag: &str,
    args: &SharedArgs,
    resource_limits: &ResourceLimits,
    network_controls: &NetworkControls,
    node_pool_role: &str,
    node_toleration: Option<&str>,
) -> anyhow::Result<String> {
    let bootstrap_nodes: Vec<String> = (0..args.num_nodes)
        .map(|j| {
            let peer_id = peer_id_from_node_id(u64::from(j))?;
            Ok(make_multi_address(
                &format!("/dns/{STRESS_TEST_NAME}-{j}.{STRESS_TEST_NAME}-headless"),
                P2P_PORT_BASE,
                &peer_id,
                args.quic,
            ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let env_vars = get_k8s_env_vars(
        None,
        METRIC_PORT_BASE,
        P2P_PORT_BASE,
        &bootstrap_nodes,
        args,
        network_controls,
    )?;

    let mut pod_spec = json!({
        "subdomain": format!("{STRESS_TEST_NAME}-headless"),
        "restartPolicy": "Never",
        "setHostnameAsFQDN": false,
        "containers": [{
            "name": STRESS_TEST_NAME,
            "image": image_tag,
            "securityContext": {"privileged": true},
            "ports": [
                {"containerPort": METRIC_PORT_BASE, "name": "metrics"},
                {"containerPort": P2P_PORT_BASE, "protocol": "UDP", "name": "p2p-udp"},
                {"containerPort": P2P_PORT_BASE, "protocol": "TCP", "name": "p2p-tcp"}
            ],
            "env": env_vars,
            "resources": {
                "limits": {
                    "memory": &resource_limits.memory_limits,
                    "cpu": &resource_limits.cpu_limits
                },
                "requests": {
                    "memory": &resource_limits.memory_requests,
                    "cpu": &resource_limits.cpu_requests
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
        "metadata": {"name": STRESS_TEST_NAME},
        "spec": {
            "completionMode": "Indexed",
            "completions": args.num_nodes,
            "parallelism": args.num_nodes,
            "template": {
                "metadata": {"labels": {"app": STRESS_TEST_NAME}},
                "spec": pod_spec
            }
        }
    });

    generate_json(data)
}

pub fn get_prometheus_rbac_json(namespace: &str) -> anyhow::Result<String> {
    let rbac_name = format!("prometheus-cadvisor-{}", namespace);
    let data = json!({
        "apiVersion": "v1",
        "kind": "List",
        "items": [
            {
                "apiVersion": "v1",
                "kind": "ServiceAccount",
                "metadata": {"name": "prometheus"}
            },
            {
                "apiVersion": "rbac.authorization.k8s.io/v1",
                "kind": "ClusterRole",
                "metadata": {"name": &rbac_name},
                "rules": [{
                    "apiGroups": [""],
                    "resources": ["nodes", "nodes/proxy"],
                    "verbs": ["get", "list"]
                }]
            },
            {
                "apiVersion": "rbac.authorization.k8s.io/v1",
                "kind": "ClusterRoleBinding",
                "metadata": {"name": &rbac_name},
                "roleRef": {
                    "apiGroup": "rbac.authorization.k8s.io",
                    "kind": "ClusterRole",
                    "name": &rbac_name
                },
                "subjects": [{
                    "kind": "ServiceAccount",
                    "name": "prometheus",
                    "namespace": namespace
                }]
            }
        ]
    });
    generate_json(data)
}

pub fn get_grafana_configmap_json_file() -> anyhow::Result<String> {
    use crate::grafana_config::*;

    let dashboard_json = get_grafana_dashboard_json(false);
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
