import json

from cdk8s import Chart, Names
from constructs import Construct

from src.config.schema import ServiceConfig
from src.constructs.backendconfig import BackendConfigConstruct
from src.constructs.configmap import ConfigMapConstruct
from src.constructs.deployment import DeploymentConstruct
from src.constructs.externalsecret import ExternalSecretConstruct
from src.constructs.hpa import HpaConstruct
from src.constructs.ingress import IngressConstruct
from src.constructs.networkpolicy import NetworkPolicyConstruct
from src.constructs.poddisruptionbudget import PodDisruptionBudgetConstruct
from src.constructs.podmonitoring import PodMonitoringConstruct
from src.constructs.priorityclass import PriorityClassConstruct
from src.constructs.rbac import RbacConstruct
from src.constructs.secret import SecretConstruct
from src.constructs.service import ServiceConstruct
from src.constructs.serviceaccount import ServiceAccountConstruct
from src.constructs.statefulset import StatefulSetConstruct
from src.constructs.volume import VolumeConstruct


class SequencerNodeChart(Chart):
    def __init__(
        self,
        scope: Construct,
        name: str,
        namespace: str,
        monitoring: bool,
        service_config: ServiceConfig,
    ):
        super().__init__(scope, name, disable_resource_name_hashes=True, namespace=namespace)

        self.monitoring = monitoring
        self.service_config = service_config

        # Create labels dictionary from service config + service name
        # Base labels from common.yaml (metaLabels) - now merged into service_config
        labels = dict(service_config.metaLabels) if service_config.metaLabels else {}
        # Add service label (dynamic per service)
        labels["service"] = f"sequencer-{service_config.name}"

        # Determine monitoring port
        monitoring_endpoint_port = self._get_monitoring_endpoint_port(service_config)

        # Create ConfigMap
        self.config_map = ConfigMapConstruct(
            self, "configmap", service_config, labels, monitoring_endpoint_port
        )

        # Create ServiceAccount if enabled
        if self.service_config.serviceAccount and self.service_config.serviceAccount.enabled:
            self.service_account = ServiceAccountConstruct(
                self,
                "service-account",
                service_config=self.service_config,
                labels=labels,
                monitoring_endpoint_port=monitoring_endpoint_port,
            )

        # Create Service
        self.service = ServiceConstruct(
            self,
            "service",
            service_config=self.service_config,
            labels=labels,
            monitoring_endpoint_port=monitoring_endpoint_port,
        )

        # Create Controller (Deployment or StatefulSet)
        if self.service_config.statefulSet and self.service_config.statefulSet.enabled:
            self.controller = StatefulSetConstruct(
                self,
                "statefulset",
                service_config=self.service_config,
                labels=labels,
                monitoring_endpoint_port=monitoring_endpoint_port,
            )
        else:
            self.controller = DeploymentConstruct(
                self,
                "deployment",
                service_config=self.service_config,
                labels=labels,
                monitoring_endpoint_port=monitoring_endpoint_port,
            )

        # Create BackendConfig if enabled
        if self.service_config.backendConfig and self.service_config.backendConfig.enabled:
            self.backend_config = BackendConfigConstruct(
                self,
                "backend-config",
                service_config=self.service_config,
                labels=labels,
                monitoring_endpoint_port=monitoring_endpoint_port,
            )

        # Create Ingress if enabled
        if self.service_config.ingress and self.service_config.ingress.enabled:
            self.service.service.metadata.add_annotation(
                key="cloud.google.com/neg", value='{"ingress": true}'
            )
            self.ingress = IngressConstruct(
                self,
                "ingress",
                self.service_config,
                labels,
                namespace,
                monitoring_endpoint_port,
            )
            self.service.service.metadata.add_annotation(
                key="cloud.google.com/backend-config",
                value=json.dumps({"default": f"{Names.to_label_value(self)}-backend-config"}),
            )

        # Create PersistentVolumeClaim if enabled
        if self.service_config.persistentVolume and self.service_config.persistentVolume.enabled:
            self.pvc = VolumeConstruct(
                self,
                "pvc",
                service_config=self.service_config,
                labels=labels,
                monitoring_endpoint_port=monitoring_endpoint_port,
            )

        # Create HPA if enabled
        if self.service_config.hpa and self.service_config.hpa.enabled:
            k8s_controller = (
                self.controller.deployment
                if hasattr(self.controller, "deployment")
                else self.controller.statefulset
            )
            self.hpa = HpaConstruct(
                self,
                "hpa",
                service_config=self.service_config,
                labels=labels,
                monitoring_endpoint_port=monitoring_endpoint_port,
                controller=k8s_controller,
            )

        # Create Secret if enabled
        if self.service_config.secret and self.service_config.secret.enabled:
            self.secret = SecretConstruct(
                self,
                "secret",
                service_config=self.service_config,
                labels=labels,
                monitoring_endpoint_port=monitoring_endpoint_port,
            )

        # Create ExternalSecret if configured
        if self.service_config.externalSecret and self.service_config.externalSecret.enabled:
            self.external_secret = ExternalSecretConstruct(
                self,
                "external-secret",
                service_config=self.service_config,
                labels=labels,
                monitoring_endpoint_port=monitoring_endpoint_port,
            )

        # Create PodMonitoring if enabled (checks both common and service config)
        # The construct will merge common and service configs internally
        self.pod_monitoring = PodMonitoringConstruct(
            self,
            "pod-monitoring",
            service_config=self.service_config,
            labels=labels,
            monitoring_endpoint_port=monitoring_endpoint_port,
        )

        # Create PodDisruptionBudget if enabled
        if (
            self.service_config.podDisruptionBudget
            and self.service_config.podDisruptionBudget.enabled
        ):
            self.pod_disruption_budget = PodDisruptionBudgetConstruct(
                self,
                "pod-disruption-budget",
                service_config=self.service_config,
                labels=labels,
                monitoring_endpoint_port=monitoring_endpoint_port,
            )

        # Create NetworkPolicy if enabled
        if self.service_config.networkPolicy and self.service_config.networkPolicy.enabled:
            self.network_policy = NetworkPolicyConstruct(
                self,
                "network-policy",
                service_config=self.service_config,
                labels=labels,
                monitoring_endpoint_port=monitoring_endpoint_port,
            )

        # Create PriorityClass if enabled
        if self.service_config.priorityClass and self.service_config.priorityClass.enabled:
            self.priority_class = PriorityClassConstruct(
                self,
                "priority-class",
                service_config=self.service_config,
                labels=labels,
                monitoring_endpoint_port=monitoring_endpoint_port,
            )

        # Create RBAC resources if enabled
        if self.service_config.rbac and self.service_config.rbac.enabled:
            self.rbac = RbacConstruct(
                self,
                "rbac",
                service_config=self.service_config,
                labels=labels,
                monitoring_endpoint_port=monitoring_endpoint_port,
            )

    @staticmethod
    def _get_monitoring_endpoint_port(service_config: ServiceConfig) -> int:
        """Find the 'monitoring' port from the service config (common ports are merged in)."""
        # Ports from common config are already merged into service_config.service.ports
        all_ports = []

        # Get all ports (common ports are already merged in by the merger)
        if service_config.service and service_config.service.ports:
            all_ports.extend(service_config.service.ports)

        if not all_ports:
            raise ValueError(
                f"No ports defined for service {service_config.name}. "
                f"Please define at least one port in service.ports or common.service.ports."
            )

        # First, look for exact "monitoring-endpoint" port (prioritized)
        for port in all_ports:
            if port.name and port.name.lower() == "monitoring-endpoint":
                return port.port

        # Then, look for any port with "monitoring" in the name (case-insensitive)
        for port in all_ports:
            if port.name and "monitoring" in port.name.lower():
                return port.port

        # If not found, provide helpful error message with available ports
        available_names = [p.name for p in all_ports if p.name]
        raise ValueError(
            f"No 'monitoring' port defined for service {service_config.name}. "
            f"Available ports: {available_names}. "
            f"Please add a port with 'monitoring' in the name (e.g., 'monitoring' or 'monitoring-endpoint')."
        )

    @staticmethod
    def _load_service_config_paths(service_config: ServiceConfig) -> dict:
        """Load or construct config values from the ServiceConfig."""
        # For configList, we can't extract the paths here as they're in a JSON file
        # This method may not be applicable for configList format
        return {}

    def _get_nested_attr(self, obj: dict, dotted_key: str):
        """Traverse nested dict keys like 'a.b.c'."""
        parts = dotted_key.split(".")
        val = obj
        for p in parts:
            if isinstance(val, dict) and p in val:
                val = val[p]
            else:
                raise KeyError(f"Key '{dotted_key}' not found in node_config")
        return val
