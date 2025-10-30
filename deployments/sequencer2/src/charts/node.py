import json

from cdk8s import Chart, Names
from constructs import Construct

from src.config.schema import CommonConfig, ServiceConfig
from src.constructs.backendconfig import BackendConfigConstruct
from src.constructs.configmap import ConfigMapConstruct
from src.constructs.deployment import DeploymentConstruct
from src.constructs.hpa import HpaConstruct
from src.constructs.ingress import IngressConstruct
from src.constructs.monitoring import PodMonitoringConstruct
from src.constructs.secret import SecretConstruct
from src.constructs.service import ServiceConstruct
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
        common_config: CommonConfig,
    ):
        super().__init__(scope, name, disable_resource_name_hashes=True, namespace=namespace)

        self.monitoring = monitoring
        self.service_config = service_config
        self.common_config = common_config

        # Create labels dictionary (avoid conflict with Chart.labels property)
        labels = {
            "app": "sequencer",
            "service": Names.to_label_value(self, include_hash=False),
        }

        # Load config paths and determine monitoring port
        node_config = self._load_service_config_paths(service_config)
        monitoring_endpoint_port = self._get_monitoring_endpoint_port(service_config)

        # Create ConfigMap
        self.config_map = ConfigMapConstruct(
            self, "configmap", common_config, service_config, labels, monitoring_endpoint_port
        )

        # Create Service
        self.service = ServiceConstruct(
            self,
            "service",
            self.service_config,
            labels,
            node_config,
        )

        # Create Controller (Deployment or StatefulSet)
        controller_type = (
            "statefulset"
            if getattr(self.service_config.statefulSet, "enabled", False)
            else "deployment"
        )

        if self.service_config.statefulSet and self.service_config.statefulSet.enabled:
            self.controller = StatefulSetConstruct(
                self,
                "statefulset",
                common_config=self.common_config,
                service_config=self.service_config,
                labels=labels,
                monitoring_endpoint_port=monitoring_endpoint_port,
            )
        else:
            self.controller = DeploymentConstruct(
                self,
                "deployment",
                common_config=self.common_config,
                service_config=self.service_config,
                labels=labels,
                monitoring_endpoint_port=monitoring_endpoint_port,
            )

        # Create BackendConfig if enabled
        if self.service_config.backendConfig and self.service_config.backendConfig.enabled:
            self.backend_config = BackendConfigConstruct(
                self,
                "backend-config",
                common_config=self.common_config,
                service_config=self.service_config,
                labels=labels,
                monitoring_endpoint_port=monitoring_endpoint_port,
            )

        # Create Ingress if enabled
        if getattr(self.service_config, "ingress", None) and self.service_config.ingress.enabled:
            self.service.service.metadata.add_annotation(
                key="cloud.google.com/neg", value='{"ingress": true}'
            )
            self.ingress = IngressConstruct(
                self,
                "ingress",
                common_config,
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
        if (
            getattr(self.service_config, "persistentVolume", None)
            and self.service_config.persistentVolume.enabled
        ):
            self.pvc = VolumeConstruct(self, "pvc", self.service_config, labels)

        # Create HPA if enabled
        if getattr(self.service_config, "hpa", None) and self.service_config.hpa.enabled:
            k8s_controller = (
                self.controller.deployment
                if controller_type == "deployment"
                else self.controller.statefulset
            )
            self.hpa = HpaConstruct(self, "hpa", labels, self.service_config, k8s_controller)

        # Create ExternalSecret if configured
        if getattr(self.service_config, "external_secret", None):
            self.external_secret = SecretConstruct(
                self, "external-secret", self.service_config, labels
            )

        # Create PodMonitoring if enabled
        if self.monitoring:
            self.podmonitoring = PodMonitoringConstruct(
                self, "pod-monitoring", labels, monitoring_endpoint_port
            )

    @staticmethod
    def _get_monitoring_endpoint_port(service_config: ServiceConfig) -> int:
        """Find the 'monitoring' port from the service config."""
        ports = getattr(service_config.service, "ports", [])
        for port in ports:
            if port.name == "monitoring":
                return port.port
        raise ValueError(f"No 'monitoring' port defined for service {service_config.name}")

    @staticmethod
    def _load_service_config_paths(service_config: ServiceConfig) -> dict:
        """Load or construct config values from the ServiceConfig."""
        config_paths = getattr(service_config, "configPaths", [])
        result = {}
        for path in config_paths:
            # Example: load JSON or YAML config files if needed
            # or just record file paths for other constructs
            result[path] = {"path": path}
        return result

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
