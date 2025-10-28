import json

from cdk8s import Names
from constructs import Construct

from src.constructs.configmap import ConfigMapConstruct
from src.constructs.deployment import Deployment
from src.constructs.hpa import HpaConstruct
from src.constructs.ingress import Ingress
from src.constructs.monitoring import PodMonitoringConstruct
from src.constructs.secret import Secret
from src.constructs.service import Service
from src.constructs.statefulset import StatefulSet
from src.constructs.volume import Volume
from src.config import topology


class ServiceApp(Construct):
    def __init__(
        self,
        scope: Construct,
        id: str,
        *,
        namespace: str,
        service_topology: topology.ServiceTopology,
        monitoring: bool,
    ):
        super().__init__(scope, id)

        self.namespace = namespace
        self.monitoring = monitoring
        self.labels = {
            "app": "sequencer",
            "service": Names.to_label_value(self, include_hash=False),
        }
        self.service_topology = service_topology
        self.node_config = service_topology.config.load()
        self.monitoring_endpoint_port = self._get_config_attr("monitoring_endpoint_config.port")

        self.config_map = ConfigMapConstruct(self, "configmap", self.node_config)

        self.service = Service(
            self,
            "service",
            self.service_topology,
            self.labels,
            self.node_config,
        )

        if self.service_topology.controller == "deployment":
            self.controller = Deployment(
                self,
                "deployment",
                self.service_topology,
                self.labels,
                self.monitoring_endpoint_port,
                self.node_config,
            )
        elif self.service_topology.controller == "statefulset":
            self.controller = StatefulSet(
                self,
                "statefulset",
                self.service_topology,
                self.labels,
                self.monitoring_endpoint_port,
                self.node_config,
            )
        else:
            raise ValueError(f"Unknown controller type: {self.service_topology.controller}")

        if self.service_topology.ingress is not None:
            self.service.service.metadata.add_annotation(
                key="cloud.google.com/neg", value='{"ingress": true}'
            )
            self.ingress = Ingress(
                self,
                "ingress",
                self.service_topology,
                self.labels,
                self.namespace,
                self.monitoring_endpoint_port,
            )
            self.service.service.metadata.add_annotation(
                key="cloud.google.com/backend-config",
                value=json.dumps(
                    {
                        "default": f"{self.node.id}-backend-config",
                    }
                ),
            )

        if self.service_topology.storage is not None:
            self.pvc = Volume(self, "pvc", self.service_topology, self.labels)

        if self.service_topology.autoscale:
            k8s_controller = (
                self.controller.deployment
                if self.service_topology.controller == "deployment"
                else self.controller.statefulset
            )
            self.hpa = HpaConstruct(
                self, "hpa", self.labels, self.service_topology, k8s_controller
            )

        if self.service_topology.external_secret is not None:
            self.external_secret = Secret(
                self, "external-secret", self.service_topology, self.labels
            )

        if self.monitoring:
            self.podmonitoring = PodMonitoringConstruct(
                self, "pod-monitoring", self.labels, self.monitoring_endpoint_port
            )

    def _get_config_attr(self, attr: str) -> str | int:
        config_attr = self.node_config.get(attr)
        assert config_attr is not None, f'Config attribute "{attr}" is missing.'

        return config_attr
