import yaml
from typing import Dict, Any
from args import get_arguments
from utils import get_peer_id_from_node_id, prometheus_service_name


def _generate_yaml(data: Dict[str, Any]) -> str:
    """
    Generate YAML string from data structure with proper formatting.

    Args:
        data: Dictionary representing the Kubernetes manifest

    Returns:
        Properly formatted YAML string
    """
    try:
        return yaml.dump(
            data,
            default_flow_style=False,
            sort_keys=False,
            indent=2,
            width=120,
            allow_unicode=True,
        )
    except yaml.YAMLError as e:
        raise ValueError(f"Failed to generate YAML: {e}")


def _get_prometheus_config_data(
    self_scrape: bool, metric_urls: list[str]
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
                "job_name": f"network_stress_test_{i}",
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
        "global": {"scrape_interval": "5s"},
        "scrape_configs": scrape_configs,
    }


def get_prometheus_config(self_scrape: bool, metric_urls: list[str]) -> str:
    """Generate Prometheus configuration YAML."""
    config_data = _get_prometheus_config_data(self_scrape, metric_urls)
    return _generate_yaml(config_data)


def get_prometheus_yaml_file(num_nodes: int) -> str:
    """Generate Prometheus ConfigMap YAML."""
    # Generate targets for each network stress test node (metrics on port 2000)
    urls = [
        f"network-stress-test-{i}.network-stress-test-headless:2000"
        for i in range(num_nodes)
    ]
    # Get the config data structure using existing function
    config_data = _get_prometheus_config_data(self_scrape=False, metric_urls=urls)

    # Create a LiteralStr class to force literal block scalar representation
    class LiteralStr(str):
        pass

    def literal_str_representer(dumper, data):
        return dumper.represent_scalar("tag:yaml.org,2002:str", data, style="|")

    yaml.add_representer(LiteralStr, literal_str_representer)

    # Convert config data to YAML string and wrap as LiteralStr
    config_yaml = _generate_yaml(config_data)
    literal_config = LiteralStr(config_yaml)

    data = {
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {"name": "prometheus-config"},
        "data": {"prometheus.yml": literal_config},
    }

    return _generate_yaml(data)


def get_prometheus_deployment_yaml_file() -> str:
    """Generate Prometheus Deployment YAML."""
    data = {
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": {"name": "prometheus"},
        "spec": {
            "replicas": 1,
            "selector": {"matchLabels": {"app": "prometheus"}},
            "template": {
                "metadata": {"labels": {"app": "prometheus"}},
                "spec": {
                    "containers": [
                        {
                            "name": "prometheus",
                            "image": "registry.hub.docker.com/prom/prometheus:main",
                            "imagePullPolicy": "Always",
                            "ports": [{"containerPort": 9090}],
                            "volumeMounts": [
                                {
                                    "name": "config-volume",
                                    "mountPath": "/etc/prometheus",
                                }
                            ],
                            "args": ["--config.file=/etc/prometheus/prometheus.yml"],
                        }
                    ],
                    "volumes": [
                        {
                            "name": "config-volume",
                            "configMap": {"name": "prometheus-config"},
                        }
                    ],
                },
            },
        },
    }

    return _generate_yaml(data)


def get_prometheus_service_yaml_file() -> str:
    """Generate Prometheus Service YAML."""
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

    return _generate_yaml(data)


def get_network_stress_test_deployment_yaml_file(image_tag: str, args) -> str:
    """Generate Network Stress Test StatefulSet YAML."""
    # Get command line arguments and convert them to environment variables
    bootstrap_nodes = [
        f"/dns/network-stress-test-{j}.network-stress-test-headless/udp/10000/quic-v1/p2p/{get_peer_id_from_node_id(j)}"
        for j in range(args.num_nodes)
    ]

    arguments = get_arguments(
        id=None,
        metric_port=2000,
        p2p_port=10000,
        bootstrap_nodes=bootstrap_nodes,
        args=args,
    )

    env_vars = []

    # Convert arguments to environment variables
    for name, value in arguments:
        env_name = name[2:].replace("-", "_").upper()
        env_vars.append({"name": env_name, "value": str(value)})

    # Add latency and throughput if provided
    for arg_name, env_name in [("latency", "LATENCY"), ("throughput", "THROUGHPUT")]:
        value = getattr(args, arg_name, None)
        if value is not None:
            env_vars.append({"name": env_name, "value": str(value)})

    data = {
        "apiVersion": "apps/v1",
        "kind": "StatefulSet",
        "metadata": {"name": "network-stress-test"},
        "spec": {
            "serviceName": "network-stress-test-headless",
            "replicas": args.num_nodes,
            "selector": {"matchLabels": {"app": "network-stress-test"}},
            "template": {
                "metadata": {"labels": {"app": "network-stress-test"}},
                "spec": {
                    "containers": [
                        {
                            "name": "network-stress-test",
                            "image": image_tag,
                            "securityContext": {"privileged": True},
                            "ports": [
                                {"containerPort": 2000, "name": "metrics"},
                                {
                                    "containerPort": 10000,
                                    "protocol": "UDP",
                                    "name": "p2p",
                                },
                            ],
                            "env": env_vars,
                        }
                    ]
                },
            },
        },
    }

    return _generate_yaml(data)


def get_network_stress_test_headless_service_yaml_file() -> str:
    """Generate Network Stress Test headless Service YAML."""
    data = {
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {"name": "network-stress-test-headless"},
        "spec": {
            "clusterIP": "None",
            "selector": {"app": "network-stress-test"},
            "ports": [
                {"port": 2000, "targetPort": 2000, "name": "metrics"},
                {"port": 10000, "targetPort": 10000, "protocol": "UDP", "name": "p2p"},
            ],
        },
    }

    return _generate_yaml(data)
