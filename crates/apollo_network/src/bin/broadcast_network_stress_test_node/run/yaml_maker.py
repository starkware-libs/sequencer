import yaml
from typing import Dict, Any
from args import get_env_vars
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

    env_vars = get_env_vars(
        id=None,
        metric_port=2000,
        p2p_port=10000,
        bootstrap_nodes=bootstrap_nodes,
        args=args,
    )

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


def get_grafana_configmap_yaml_file() -> str:
    """Generate Grafana ConfigMap YAML with dashboard and datasource configuration."""
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

    return _generate_yaml(data)


def get_grafana_deployment_yaml_file() -> str:
    """Generate Grafana StatefulSet YAML."""
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

    return _generate_yaml(data)


def get_grafana_service_yaml_file() -> str:
    """Generate Grafana Service YAML."""
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

    return _generate_yaml(data)


def get_grafana_headless_service_yaml_file() -> str:
    """Generate Grafana Headless Service YAML for StatefulSet."""
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

    return _generate_yaml(data)


# def _delay_seconds_to_cron_schedule(delay_seconds: int) -> str:
#     """
#     Convert delay seconds to a cron schedule that runs once at the specified time.

#     Args:
#         delay_seconds: Number of seconds from now to schedule the job

#     Returns:
#         Cron schedule string
#     """
#     from datetime import datetime, timedelta, timezone

#     # Calculate target time in UTC (since Kubernetes CronJobs use UTC)
#     target_time = datetime.now(timezone.utc) + timedelta(seconds=delay_seconds)

#     # Convert to cron format (minute hour day month weekday)
#     return f"{target_time.minute} {target_time.hour} {target_time.day} {target_time.month} *"


# def get_namespace_deletion_job_yaml_file(
#     namespace_name: str, delay_seconds: int
# ) -> str:
#     """
#     Generate a CronJob YAML that deletes a specified namespace after a delay.

#     This is a compatibility wrapper that converts the old Job approach to use CronJob.

#     Args:
#         namespace_name: The name of the namespace to delete
#         delay_seconds: Number of seconds to wait before deleting the namespace

#     Returns:
#         YAML string for the namespace deletion CronJob
#     """
#     # Convert delay to cron schedule
#     schedule = _delay_seconds_to_cron_schedule(delay_seconds)
#     return _get_namespace_deletion_cronjob_yaml_file(namespace_name, schedule)


# def get_namespace_deleter_rbac_yaml_file(namespace_name: str) -> str:
#     """
#     Generate RBAC resources (ServiceAccount, ClusterRole, ClusterRoleBinding)
#     required for the namespace deletion job.

#     Args:
#         namespace_name: The name of the namespace to be deleted (used in resource names)

#     Returns:
#         YAML string containing all necessary RBAC resources
#     """
#     service_account_name = f"namespace-deleter-{namespace_name}"
#     cluster_role_name = f"namespace-deleter-{namespace_name}"
#     cluster_role_binding_name = f"namespace-deleter-{namespace_name}"

#     service_account = {
#         "apiVersion": "v1",
#         "kind": "ServiceAccount",
#         "metadata": {"name": service_account_name, "namespace": "default"},
#     }

#     cluster_role = {
#         "apiVersion": "rbac.authorization.k8s.io/v1",
#         "kind": "ClusterRole",
#         "metadata": {"name": cluster_role_name},
#         "rules": [
#             {
#                 "apiGroups": [""],
#                 "resources": ["namespaces"],
#                 "verbs": [
#                     "delete",
#                     "get",
#                     "list",
#                     "patch",
#                 ],  # Added patch for finalizer removal
#             },
#             {
#                 "apiGroups": ["batch"],
#                 "resources": ["cronjobs"],
#                 "verbs": ["delete"],  # Permission to delete the cronjob after success
#             },
#             {
#                 "apiGroups": ["apps"],
#                 "resources": [
#                     "statefulsets",
#                     "deployments",
#                     "replicasets",
#                     "daemonsets",
#                 ],
#                 "verbs": [
#                     "delete",
#                     "list",
#                     "get",
#                     "patch",
#                 ],  # Need to delete workloads before namespace
#             },
#             {
#                 "apiGroups": [""],
#                 "resources": [
#                     "pods",
#                     "services",
#                     "persistentvolumeclaims",
#                     "configmaps",
#                     "secrets",
#                 ],
#                 "verbs": [
#                     "delete",
#                     "list",
#                     "get",
#                     "patch",
#                 ],  # Need to delete core resources
#             },
#             {
#                 "apiGroups": ["networking.k8s.io"],
#                 "resources": ["ingresses", "networkpolicies"],
#                 "verbs": [
#                     "delete",
#                     "list",
#                     "get",
#                 ],  # Network resources that might block deletion
#             },
#             {
#                 "apiGroups": ["rbac.authorization.k8s.io"],
#                 "resources": ["roles", "rolebindings"],
#                 "verbs": [
#                     "delete",
#                     "list",
#                     "get",
#                 ],  # RBAC resources within the namespace
#             },
#         ],
#     }

#     cluster_role_binding = {
#         "apiVersion": "rbac.authorization.k8s.io/v1",
#         "kind": "ClusterRoleBinding",
#         "metadata": {"name": cluster_role_binding_name},
#         "roleRef": {
#             "apiGroup": "rbac.authorization.k8s.io",
#             "kind": "ClusterRole",
#             "name": cluster_role_name,
#         },
#         "subjects": [
#             {
#                 "kind": "ServiceAccount",
#                 "name": service_account_name,
#                 "namespace": "default",
#             }
#         ],
#     }

#     # Combine all resources with document separator
#     rbac_yaml = _generate_yaml(service_account)
#     rbac_yaml += "---\n"
#     rbac_yaml += _generate_yaml(cluster_role)
#     rbac_yaml += "---\n"
#     rbac_yaml += _generate_yaml(cluster_role_binding)

#     return rbac_yaml


# def _get_namespace_deletion_cronjob_yaml_file(
#     namespace_name: str, schedule: str
# ) -> str:
#     """
#     Generate a CronJob YAML that deletes a specified namespace at a scheduled time.

#     This CronJob will delete itself after every successful execution. Whether the
#     namespace exists and gets deleted, or doesn't exist at all, the CronJob will
#     remove itself completely after determining the task is complete.

#     Args:
#         namespace_name: The name of the namespace to delete
#         schedule: Cron schedule expression (e.g., "0 2 * * *" for 2 AM daily)

#     Returns:
#         YAML string for the self-deleting namespace deletion CronJob that removes itself on every success
#     """
#     cronjob_name = f"rm-{namespace_name}"

#     # Simple deletion script without sleep
#     deletion_script = f"""#!/bin/bash
# set -euo pipefail

# echo "$(date) Starting scheduled deletion of namespace {namespace_name}"

# # Check if namespace exists
# if ! kubectl get namespace {namespace_name} >/dev/null 2>&1; then
#     echo "$(date) Namespace {namespace_name} does not exist"
#     # Delete the CronJob since the task is already complete (namespace doesn't exist)
#     echo "$(date) Deleting CronJob {cronjob_name} since namespace is already gone"
#     kubectl delete cronjob {cronjob_name} --timeout=30s || echo "$(date) Warning: Failed to delete CronJob"
#     exit 0
# fi

# # Try normal deletion with timeout first
# echo "$(date) Attempting normal deletion with 5 minute timeout"
# if timeout 300s kubectl delete namespace {namespace_name}; then
#     echo "$(date) Namespace {namespace_name} deleted successfully"
#     # Delete the CronJob after successful completion since task is complete
#     echo "$(date) Deleting CronJob {cronjob_name} since task is complete"
#     kubectl delete cronjob {cronjob_name} --timeout=30s || echo "$(date) Warning: Failed to delete CronJob"
#     exit 0
# fi

# # If that failed, remove finalizers and try again
# echo "$(date) Normal deletion timed out, removing finalizers"
# kubectl patch namespace {namespace_name} -p '{{"metadata":{{"finalizers":null}}}}' --timeout=30s || true
# kubectl delete namespace {namespace_name} --ignore-not-found=true --timeout=60s

# echo "$(date) Namespace deletion completed"

# # Delete the CronJob after successful completion since it's a one-time task
# echo "$(date) Deleting CronJob {cronjob_name} since task is complete"
# kubectl delete cronjob {cronjob_name} --timeout=30s || echo "$(date) Warning: Failed to delete CronJob"
# """

#     data = {
#         "apiVersion": "batch/v1",
#         "kind": "CronJob",
#         "metadata": {
#             "name": cronjob_name,
#             "namespace": "default",
#         },
#         "spec": {
#             "schedule": schedule,
#             "timeZone": "UTC",  # Specify timezone for the schedule
#             "jobTemplate": {
#                 "spec": {
#                     "template": {
#                         "spec": {
#                             "serviceAccountName": f"namespace-deleter-{namespace_name}",
#                             "restartPolicy": "OnFailure",
#                             "containers": [
#                                 {
#                                     "name": "kubectl-delete",
#                                     "image": "registry.hub.docker.com/rancher/kubectl:v1.32.9",
#                                     "command": ["sh", "-c", deletion_script],
#                                     "resources": {
#                                         "limits": {"memory": "128Mi", "cpu": "100m"},
#                                         "requests": {"memory": "64Mi", "cpu": "50m"},
#                                     },
#                                 }
#                             ],
#                         }
#                     },
#                     "backoffLimit": 3,  # Retry up to 3 times if the job fails
#                     "activeDeadlineSeconds": 900,  # 15 minute timeout per job run
#                 }
#             },
#             "successfulJobsHistoryLimit": 1,  # Keep only 1 successful run
#             "failedJobsHistoryLimit": 3,  # Keep last 3 failed runs for debugging
#         },
#     }

#     return _generate_yaml(data)
