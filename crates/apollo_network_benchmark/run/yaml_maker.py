import json
from typing import Dict, Any
from args import get_env_vars
from utils import get_peer_id_from_node_id, make_multi_address


prometheus_service_name: str = "prometheus-service"


def _generate_json(data: Dict[str, Any]) -> str:
    """
    Generate JSON string from data structure with proper formatting.

    Args:
        data: Dictionary representing the Kubernetes manifest

    Returns:
        Properly formatted JSON string
    """
    try:
        return json.dumps(data, indent=2, ensure_ascii=False)
    except (TypeError, ValueError) as e:
        raise ValueError(f"Failed to generate JSON: {e}")


def _get_prometheus_config_data(
    self_scrape: bool,
    metric_urls: list[str],
    scrape_seconds: int,
) -> Dict[str, Any]:
    """Generate Prometheus configuration data structure."""
    scrape_configs = []

    if self_scrape:
        scrape_configs.append(
            {
                "job_name": "prometheus",
                "static_configs": [{"targets": ["localhost:9090"]}],
            }
        )

    for i, url in enumerate(metric_urls):
        scrape_configs.append(
            {
                "job_name": f"broadcast_network_stress_test_{i}",
                "static_configs": [
                    {
                        "targets": [url],
                        "labels": {
                            "application": "broadcast_network_stress_test_node",
                            "environment": "test",
                        },
                    }
                ],
            }
        )

    return {
        "global": {"scrape_interval": f"{scrape_seconds}s"},
        "scrape_configs": scrape_configs,
    }


def get_prometheus_config(self_scrape: bool, metric_urls: list[str]) -> str:
    """Generate Prometheus configuration JSON."""
    config_data = _get_prometheus_config_data(
        self_scrape, metric_urls, scrape_seconds=1
    )
    return _generate_json(config_data)


def get_prometheus_json_file(num_nodes: int) -> str:
    """Generate Prometheus ConfigMap JSON."""
    # Generate targets for each network stress test node (metrics on port 2000)
    urls = [
        f"broadcast-network-stress-test-{i}.broadcast-network-stress-test-headless:2000"
        for i in range(num_nodes)
    ]
    # Get the config data structure using existing function
    config_data = _get_prometheus_config_data(
        self_scrape=False, metric_urls=urls, scrape_seconds=5
    )

    # For JSON, we can serialize the prometheus config directly as a JSON string
    import yaml

    config_yaml = yaml.dump(
        config_data,
        default_flow_style=False,
        sort_keys=False,
        indent=2,
        width=120,
        allow_unicode=True,
    )

    data = {
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {"name": "prometheus-config"},
        "data": {"prometheus.yml": config_yaml},
    }

    return _generate_json(data)


def get_prometheus_deployment_json_file() -> str:
    """Generate Prometheus StatefulSet JSON (named deployment for backward compatibility)."""
    data = {
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
                        "fsGroup": 65534,  # nobody group - matches Prometheus container user
                        "runAsUser": 65534,  # nobody user
                        "runAsGroup": 65534,  # nobody group
                        "runAsNonRoot": True,
                    },
                    "containers": [
                        {
                            "name": "prometheus",
                            "image": "registry.hub.docker.com/prom/prometheus:main",
                            "imagePullPolicy": "Always",
                            "ports": [{"containerPort": 9090}],
                            "securityContext": {
                                "allowPrivilegeEscalation": False,
                                "readOnlyRootFilesystem": True,
                                "capabilities": {"drop": ["ALL"]},
                            },
                            "volumeMounts": [
                                {
                                    "name": "config-volume",
                                    "mountPath": "/etc/prometheus",
                                },
                                {
                                    "name": "prometheus-data",
                                    "mountPath": "/prometheus",
                                },
                                {
                                    "name": "tmp-volume",
                                    "mountPath": "/tmp",
                                },
                            ],
                            "args": [
                                "--config.file=/etc/prometheus/prometheus.yml",
                                "--storage.tsdb.path=/prometheus",
                                "--web.console.libraries=/etc/prometheus/console_libraries",
                                "--web.console.templates=/etc/prometheus/consoles",
                                "--storage.tsdb.retention.time=15d",
                                "--web.enable-lifecycle",
                                "--web.listen-address=0.0.0.0:9090",
                            ],
                        }
                    ],
                    "volumes": [
                        {
                            "name": "config-volume",
                            "configMap": {"name": "prometheus-config"},
                        },
                        {
                            "name": "tmp-volume",
                            "emptyDir": {},
                        },
                    ],
                },
            },
            "volumeClaimTemplates": [
                {
                    "metadata": {"name": "prometheus-data"},
                    "spec": {
                        "accessModes": ["ReadWriteOnce"],
                        "resources": {"requests": {"storage": "16Gi"}},
                    },
                }
            ],
        },
    }

    return _generate_json(data)


def get_prometheus_service_json_file() -> str:
    """Generate Prometheus Service JSON."""
    data = {
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {"name": prometheus_service_name},
        "spec": {
            "selector": {"app": "prometheus"},
            "ports": [{"port": 9090, "targetPort": 9090}],
            "type": "ClusterIP",
        },
    }

    return _generate_json(data)


def get_prometheus_headless_service_json_file() -> str:
    """Generate Prometheus Headless Service JSON for StatefulSet."""
    data = {
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {"name": "prometheus-headless"},
        "spec": {
            "selector": {"app": "prometheus"},
            "ports": [{"port": 9090, "targetPort": 9090}],
            "clusterIP": "None",
        },
    }

    return _generate_json(data)


def get_network_stress_test_deployment_json_file(image_tag: str, args) -> str:
    """Generate Network Stress Test Indexed Job JSON."""
    # Get command line arguments and convert them to environment variables
    bootstrap_nodes = [
        make_multi_address(
            network_address=f"/dns/broadcast-network-stress-test-{j}.broadcast-network-stress-test-headless",
            port=10000,
            peer_id=get_peer_id_from_node_id(j),
            args=args,
        )
        for j in range(args.num_nodes)
    ]

    env_vars = get_env_vars(
        id=None,
        metric_port=2000,
        p2p_port=10000,
        bootstrap_nodes=bootstrap_nodes,
        args=args,
    )

    # Build the pod spec
    pod_spec = {
        "subdomain": "broadcast-network-stress-test-headless",  # For stable DNS with headless service
        "restartPolicy": "Never",  # Jobs require Never or OnFailure
        "setHostnameAsFQDN": False,  # Keep short hostname
        "containers": [
            {
                "name": "broadcast-network-stress-test",
                "image": image_tag,
                "securityContext": {"privileged": True},
                "ports": [
                    {"containerPort": 2000, "name": "metrics"},
                    {
                        "containerPort": 10000,
                        "protocol": "UDP",
                        "name": "p2p-udp",
                    },
                    {
                        "containerPort": 10000,
                        "protocol": "TCP",
                        "name": "p2p-tcp",
                    },
                ],
                "env": env_vars,
                "resources": {
                    "limits": {
                        "memory": args.memory_limits,
                        "cpu": args.cpu_limits,
                    },
                    "requests": {
                        "memory": args.memory_requests,
                        "cpu": args.cpu_requests,
                    },
                },
            }
        ],
    }

    # Add tolerations and nodeSelector if dedicated node is requested
    if args.dedicated_node_pool:
        # pod_spec["tolerations"] = [
        #     {
        #         "effect": "NoSchedule",
        #         "key": "key",
        #         "operator": "Equal",
        #         "value": args.node_pool_name,
        #     }
        # ]
        pod_spec["nodeSelector"] = {
            "cloud.google.com/gke-nodepool": args.node_pool_role
        }

    data = {
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {"name": "broadcast-network-stress-test"},
        "spec": {
            "completionMode": "Indexed",  # Each pod gets JOB_COMPLETION_INDEX from 0 to completions-1
            "completions": args.num_nodes,  # Total number of indexed pods
            "parallelism": args.num_nodes,  # Run all pods in parallel
            "template": {
                "metadata": {"labels": {"app": "broadcast-network-stress-test"}},
                "spec": pod_spec,
            },
        },
    }

    return _generate_json(data)


def get_network_stress_test_headless_service_json_file() -> str:
    """Generate Network Stress Test headless Service JSON."""
    data = {
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {"name": "broadcast-network-stress-test-headless"},
        "spec": {
            "clusterIP": "None",
            "selector": {"app": "broadcast-network-stress-test"},
            "ports": [
                {"port": 2000, "targetPort": 2000, "name": "metrics"},
                {
                    "port": 10000,
                    "targetPort": 10000,
                    "protocol": "UDP",
                    "name": "p2p-udp",
                },
                {
                    "port": 10000,
                    "targetPort": 10000,
                    "protocol": "TCP",
                    "name": "p2p-tcp",
                },
            ],
        },
    }

    return _generate_json(data)


def get_grafana_configmap_json_file() -> str:
    """Generate Grafana ConfigMap JSON with dashboard and datasource configuration."""
    from grafana_config import (
        get_grafana_dashboard_json,
        get_grafana_datasource_config_cluster,
        get_grafana_dashboard_provisioning_config,
        get_grafana_alerts_json,
        get_grafana_config,
        get_grafana_preferences_json,
    )

    dashboard_json = get_grafana_dashboard_json()
    datasource_config = get_grafana_datasource_config_cluster()
    dashboard_config = get_grafana_dashboard_provisioning_config()
    alerting_rules = get_grafana_alerts_json()
    grafana_config = get_grafana_config()
    preferences_json = get_grafana_preferences_json()

    data = {
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {"name": "grafana-config"},
        "data": {
            "dashboard.json": dashboard_json,
            "datasource.yml": datasource_config,
            "dashboard_config.yml": dashboard_config,
            "alerting_rules.json": alerting_rules,
            "grafana.ini": grafana_config,
            "preferences.json": preferences_json,
        },
    }

    return _generate_json(data)


def get_grafana_deployment_json_file() -> str:
    """Generate Grafana StatefulSet JSON."""
    data = {
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
                        "fsGroup": 472,  # grafana group
                        "runAsUser": 472,  # grafana user
                        "runAsGroup": 472,  # grafana group
                        "runAsNonRoot": True,
                    },
                    "containers": [
                        {
                            "name": "grafana",
                            "image": "grafana/grafana:latest",
                            "imagePullPolicy": "Always",
                            "ports": [{"containerPort": 3000}],
                            "env": [
                                {
                                    "name": "GF_PATHS_CONFIG",
                                    "value": "/etc/grafana/grafana.ini",
                                },
                            ],
                            "securityContext": {
                                "allowPrivilegeEscalation": False,
                                "readOnlyRootFilesystem": False,
                                "capabilities": {"drop": ["ALL"]},
                            },
                            "volumeMounts": [
                                {
                                    "name": "grafana-config",
                                    "mountPath": "/etc/grafana/grafana.ini",
                                    "subPath": "grafana.ini",
                                },
                                {
                                    "name": "grafana-config",
                                    "mountPath": "/etc/grafana/provisioning/datasources/datasource.yml",
                                    "subPath": "datasource.yml",
                                },
                                {
                                    "name": "grafana-config",
                                    "mountPath": "/etc/grafana/provisioning/dashboards/dashboard_config.yml",
                                    "subPath": "dashboard_config.yml",
                                },
                                {
                                    "name": "grafana-config",
                                    "mountPath": "/etc/grafana/provisioning/dashboards/dashboard.json",
                                    "subPath": "dashboard.json",
                                },
                                {
                                    "name": "grafana-data",
                                    "mountPath": "/var/lib/grafana",
                                },
                            ],
                        }
                    ],
                    "volumes": [
                        {
                            "name": "grafana-config",
                            "configMap": {"name": "grafana-config"},
                        }
                    ],
                },
            },
            "volumeClaimTemplates": [
                {
                    "metadata": {"name": "grafana-data"},
                    "spec": {
                        "accessModes": ["ReadWriteOnce"],
                        "resources": {"requests": {"storage": "8Gi"}},
                    },
                }
            ],
        },
    }

    return _generate_json(data)


def get_grafana_service_json_file() -> str:
    """Generate Grafana Service JSON."""
    data = {
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {"name": "grafana-service"},
        "spec": {
            "selector": {"app": "grafana"},
            "ports": [{"port": 3000, "targetPort": 3000}],
            "type": "ClusterIP",
        },
    }

    return _generate_json(data)


def get_grafana_headless_service_json_file() -> str:
    """Generate Grafana Headless Service JSON for StatefulSet."""
    data = {
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {"name": "grafana-headless"},
        "spec": {
            "selector": {"app": "grafana"},
            "ports": [{"port": 3000, "targetPort": 3000}],
            "clusterIP": "None",
        },
    }

    return _generate_json(data)
