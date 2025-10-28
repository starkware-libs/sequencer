import typing

from constructs import Construct
from imports import k8s
from src.config import constants as const


class Service(Construct):
    def __init__(self, scope: Construct, id: str, service_topology, labels, node_config):
        super().__init__(scope, id)

        self.service_topology = service_topology
        self.labels = labels
        self.node_config = node_config

        self.service = self._get_service()

    def _get_service(self) -> k8s.KubeService:
        return k8s.KubeService(
            self,
            "service",
            metadata=k8s.ObjectMeta(
                labels=self.labels,
                annotations=self._get_service_annotations(),
            ),
            spec=k8s.ServiceSpec(
                type=self._get_service_type(),
                ports=self._get_service_ports(),
                selector=self.labels,
            ),
        )

    def _get_service_ports(self) -> typing.List[k8s.ServicePort]:
        return [
            k8s.ServicePort(
                name=attr[0],
                port=self._get_config_attr(attr[1]),
                target_port=k8s.IntOrString.from_number(self._get_config_attr(attr[1])),
            )
            for attr in self._get_ports_subset_keys_from_config()
        ]

    def _get_service_annotations(self) -> typing.Dict[str, str]:
        annotations = {}
        if self.service_topology.k8s_service_config is None:
            return annotations
        if (
            self.service_topology.k8s_service_config.get("internal") is True
            and self._get_service_type() == const.K8SServiceType.LOAD_BALANCER
        ):
            annotations.update(
                {
                    "cloud.google.com/load-balancer-type": "Internal",
                    "networking.gke.io/internal-load-balancer-allow-global-access": "true",
                }
            )
        if self.service_topology.k8s_service_config.get("external_dns_name"):
            annotations.update(
                {
                    "external-dns.alpha.kubernetes.io/hostname": self.service_topology.k8s_service_config[
                        "external_dns_name"
                    ]
                }
            )
        return annotations

    def _get_service_type(self) -> const.K8SServiceType:
        if self.service_topology.k8s_service_config is None:
            return const.K8SServiceType.CLUSTER_IP
        svc_type = self.service_topology.k8s_service_config.get("type")
        if svc_type == "LoadBalancer":
            return const.K8SServiceType.LOAD_BALANCER
        elif svc_type == "NodePort":
            return const.K8SServiceType.NODE_PORT
        elif svc_type == "ClusterIP":
            return const.K8SServiceType.CLUSTER_IP
        else:
            assert False, f"Unknown service type: {svc_type}"

    def _get_config_attr(self, attr: str) -> str | int:
        config_attr = self.node_config.get(attr)
        assert config_attr is not None, f'Config attribute "{attr}" is missing.'

        return config_attr

    def _get_ports_subset_keys_from_config(self) -> typing.List[typing.Tuple[str, str]]:
        ports = []
        for k, v in self.node_config.items():
            if k.endswith(".port") and v != 0:
                if k.startswith("components."):
                    port_name = k.split(".")[1].replace("_", "-")
                elif "rpc_config" in k:
                    port_name = "_".join(k.split(".")[:2]).replace("_", "-")
                else:
                    port_name = k.split(".")[0].replace("_", "-")
            else:
                continue

            ports.append((port_name, k))

        return ports
