import json
from cdk8s import Names
from constructs import Construct

from src.constructs.configmap import ConfigMapConstruct
from src.constructs.deployment import DeploymentConstruct
from src.constructs.hpa import HpaConstruct
from src.constructs.ingress import IngressConstruct
from src.constructs.monitoring import PodMonitoringConstruct
from src.constructs.secret import SecretConstruct
from src.constructs.service import ServiceConstruct
from src.constructs.statefulset import StatefulSetConstruct
from src.constructs.volume import VolumeConstruct
from src.constructs.backendconfig import BackendConfigConstruct
from src.config.schema import ServiceConfig, CommonConfig


class NodeConstruct(Construct):
    def __init__(
        self,
        scope: Construct,
        id: str,
        *,
        namespace: str,
        service_config: ServiceConfig,
        common_config: CommonConfig,
        monitoring: bool,
    ):
        super().__init__(scope, id)

        self.namespace = namespace
        self.monitoring = monitoring
        self.service_config = service_config
        self.common_config = common_config

        self.labels = {
            "app": "sequencer",
            "service": Names.to_label_value(self, include_hash=False),
        }

        # The service config can directly expose a dict for configmaps or paths
        # Example: node_config could be JSON/YAML loaded by DeploymentConfig
        self.node_config = self._load_service_config_paths()
        self.monitoring_endpoint_port = self._get_monitoring_endpoint_port()

        self.config_map = ConfigMapConstruct(self, "configmap", self.node_config)

        self.service = ServiceConstruct(
            self,
            "service",
            self.service_config,
            self.labels,
            self.node_config,
        )

        controller_type = (
            "statefulset"
            if getattr(self.service_config.statefulSet, "enabled", False)
            else "deployment"
        )

        if self.service_config.statefulSet.enabled:
            self.controller = StatefulSetConstruct(
                self,
                "statefulset",
                common_config=self.common_config,
                service_config=self.service_config,
                labels=self.labels,
                monitoring_endpoint_port=self.monitoring_endpoint_port,
            )
        else:
            self.controller = DeploymentConstruct(
                self,
                "deployment",
                common_config=self.common_config,
                service_config=self.service_config,
                labels=self.labels,
                monitoring_endpoint_port=self.monitoring_endpoint_port,
            )
        
        if self.service_config.backendConfig.enabled:
            self.backend_config = BackendConfigConstruct(
                self,
                "backend-config",
                common_config=self.common_config,
                service_config=self.service_config,
                labels=self.labels,
                monitoring_endpoint_port=self.monitoring_endpoint_port,
            )

        if getattr(self.service_config, "ingress", None) and self.service_config.ingress.enabled:
            self.service.service.metadata.add_annotation(
                key="cloud.google.com/neg", value='{"ingress": true}'
            )
            self.ingress = IngressConstruct(
                self,
                "ingress",
                self.service_config,
                self.labels,
                self.namespace,
                self.monitoring_endpoint_port,
            )
            self.service.service.metadata.add_annotation(
                key="cloud.google.com/backend-config",
                value=json.dumps({"default": f"{Names.to_label_value(self)}-backend-config"}),
            )

        if getattr(self.service_config, "persistentVolume", None) and self.service_config.persistentVolume.enabled:
            self.pvc = VolumeConstruct(self, "pvc", self.service_config, self.labels)

        if getattr(self.service_config, "hpa", None) and self.service_config.hpa.enabled:
            k8s_controller = (
                self.controller.deployment
                if controller_type == "deployment"
                else self.controller.statefulset
            )
            self.hpa = HpaConstruct(
                self, "hpa", self.labels, self.service_config, k8s_controller
            )

        if getattr(self.service_config, "external_secret", None):
            self.external_secret = SecretConstruct(
                self, "external-secret", self.service_config, self.labels
            )

        if self.monitoring:
            self.podmonitoring = PodMonitoringConstruct(
                self, "pod-monitoring", self.labels, self.monitoring_endpoint_port
            )

    def _get_monitoring_endpoint_port(self) -> int:
        """Find the 'monitoring' port from the service config."""
        ports = getattr(self.service_config.service, "ports", [])
        for port in ports:
            if port.name == "monitoring":
                return port.port
        raise ValueError(f"No 'monitoring' port defined for service {self.service_config.name}")

    
    def _load_service_config_paths(self) -> dict:
        """Load or construct config values from the ServiceConfig."""
        config_paths = getattr(self.service_config, "configPaths", [])
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
