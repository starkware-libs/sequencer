import yaml
from typing import Dict, Any
from args import get_arguments
from utils import get_peer_id_from_node_id, make_multi_address


prometheus_service_name: str = "prometheus-service"


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
    """Generate Prometheus configuration YAML."""
    config_data = _get_prometheus_config_data(
        self_scrape, metric_urls, scrape_seconds=1
    )
    return _generate_yaml(config_data)


def get_prometheus_yaml_file(num_nodes: int) -> str:
    """Generate Prometheus ConfigMap YAML."""
    # Generate targets for each network stress test node (metrics on port 2000)
    urls = [
        f"broadcast-network-stress-test-{i}.broadcast-network-stress-test-headless:2000"
        for i in range(num_nodes)
    ]
    # Get the config data structure using existing function
    config_data = _get_prometheus_config_data(
        self_scrape=False, metric_urls=urls, scrape_seconds=5
    )

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
    """Generate Prometheus StatefulSet YAML (named deployment for backward compatibility)."""
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


def get_prometheus_headless_service_yaml_file() -> str:
    """Generate Prometheus Headless Service YAML for StatefulSet."""
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

    return _generate_yaml(data)


def get_network_stress_test_deployment_yaml_file(image_tag: str, args) -> str:
    """Generate Network Stress Test StatefulSet YAML."""
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
        value = getattr(args, arg_name)
        if value is not None:
            env_vars.append({"name": env_name, "value": str(value)})

    # Build the pod spec
    pod_spec = {
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
        pod_spec["tolerations"] = [
            {
                "effect": "NoSchedule",
                "key": "key",
                "operator": "Equal",
                "value": args.node_pool_name,
            }
        ]
        pod_spec["nodeSelector"] = {"role": args.node_pool_role}

    # Add automatic termination after the specified timeout
    # pod_spec["activeDeadlineSeconds"] = args.timeout_seconds

    data = {
        "apiVersion": "apps/v1",
        "kind": "StatefulSet",
        "metadata": {"name": "broadcast-network-stress-test"},
        "spec": {
            "serviceName": "broadcast-network-stress-test-headless",
            "replicas": args.num_nodes,
            "selector": {"matchLabels": {"app": "broadcast-network-stress-test"}},
            "template": {
                "metadata": {"labels": {"app": "broadcast-network-stress-test"}},
                "spec": pod_spec,
            },
        },
    }

    return _generate_yaml(data)


def get_network_stress_test_headless_service_yaml_file() -> str:
    """Generate Network Stress Test headless Service YAML."""
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

    return _generate_yaml(data)


def get_namespace_deletion_job_yaml_file(
    namespace_name: str, delay_seconds: int
) -> str:
    """
    Generate a Kubernetes Job YAML that deletes a specified namespace.

    Args:
        namespace_name: The name of the namespace to delete
        delay_seconds: Number of seconds to wait before deleting the namespace

    Returns:
        YAML string for the namespace deletion job
    """
    job_name = f"delete-{namespace_name}-job"

    # Build the command - add sleep if delay is specified
    command = []
    if delay_seconds > 0:
        command.extend(
            [
                "sh",
                "-c",
                f"echo 'Waiting {delay_seconds} seconds before deleting namespace {namespace_name}'; "
                f"sleep {delay_seconds}; "
                f"kubectl delete namespace {namespace_name} --ignore-not-found=true",
            ]
        )
    else:
        command.extend(
            [
                "kubectl",
                "delete",
                "namespace",
                namespace_name,
                "--ignore-not-found=true",
            ]
        )

    data = {
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {
            "name": job_name,
            "namespace": "default",  # Run the job in default namespace
        },
        "spec": {
            "template": {
                "spec": {
                    "serviceAccountName": f"namespace-deleter-{namespace_name}",  # Requires appropriate RBAC
                    "restartPolicy": "Never",
                    "containers": [
                        {
                            "name": "kubectl-delete",
                            "image": "bitnami/kubectl:latest",
                            "command": command,
                            "resources": {
                                "limits": {"memory": "128Mi", "cpu": "100m"},
                                "requests": {"memory": "64Mi", "cpu": "50m"},
                            },
                        }
                    ],
                }
            },
            "backoffLimit": 3,  # Retry up to 3 times if the job fails
            "ttlSecondsAfterFinished": 300,  # Clean up job after 5 minutes
        },
    }

    return _generate_yaml(data)


def get_namespace_deleter_rbac_yaml_file(namespace_name: str) -> str:
    """
    Generate RBAC resources (ServiceAccount, ClusterRole, ClusterRoleBinding)
    required for the namespace deletion job.

    Args:
        namespace_name: The name of the namespace to be deleted (used in resource names)

    Returns:
        YAML string containing all necessary RBAC resources
    """
    service_account_name = f"namespace-deleter-{namespace_name}"
    cluster_role_name = f"namespace-deleter-{namespace_name}"
    cluster_role_binding_name = f"namespace-deleter-{namespace_name}"

    service_account = {
        "apiVersion": "v1",
        "kind": "ServiceAccount",
        "metadata": {"name": service_account_name, "namespace": "default"},
    }

    cluster_role = {
        "apiVersion": "rbac.authorization.k8s.io/v1",
        "kind": "ClusterRole",
        "metadata": {"name": cluster_role_name},
        "rules": [
            {
                "apiGroups": [""],
                "resources": ["namespaces"],
                "verbs": ["delete", "get", "list"],
            }
        ],
    }

    cluster_role_binding = {
        "apiVersion": "rbac.authorization.k8s.io/v1",
        "kind": "ClusterRoleBinding",
        "metadata": {"name": cluster_role_binding_name},
        "roleRef": {
            "apiGroup": "rbac.authorization.k8s.io",
            "kind": "ClusterRole",
            "name": cluster_role_name,
        },
        "subjects": [
            {
                "kind": "ServiceAccount",
                "name": service_account_name,
                "namespace": "default",
            }
        ],
    }

    # Combine all resources with document separator
    rbac_yaml = _generate_yaml(service_account)
    rbac_yaml += "---\n"
    rbac_yaml += _generate_yaml(cluster_role)
    rbac_yaml += "---\n"
    rbac_yaml += _generate_yaml(cluster_role_binding)

    return rbac_yaml
