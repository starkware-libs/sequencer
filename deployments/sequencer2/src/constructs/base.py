import typing

from cdk8s import Names
from constructs import Construct
from imports import k8s
from src.config import constants as const


class BaseConstruct(Construct):
    def __init__(
        self,
        scope: Construct,
        id: str,
        service_topology,
        labels,
        monitoring_endpoint_port,
        node_config,
    ):
        super().__init__(scope, id)
        self.service_topology = service_topology
        self.labels = labels
        self.monitoring_endpoint_port = monitoring_endpoint_port
        self.node_config = node_config

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

    def _get_container_ports(self) -> typing.List[k8s.ContainerPort]:
        return [
            k8s.ContainerPort(container_port=self._get_config_attr(attr[1]))
            for attr in self._get_ports_subset_keys_from_config()
        ]

    def _get_http_probe(
        self,
        success_threshold: int,
        failure_threshold: int,
        period_seconds: int,
        timeout_seconds: int,
        path: str,
    ) -> k8s.Probe:
        return k8s.Probe(
            http_get=k8s.HttpGetAction(
                path=path,
                port=k8s.IntOrString.from_number(self.monitoring_endpoint_port),
            ),
            success_threshold=success_threshold,
            period_seconds=period_seconds,
            failure_threshold=failure_threshold,
            timeout_seconds=timeout_seconds,
        )

    def _get_volume_mounts(self) -> typing.List[k8s.VolumeMount]:
        volume_mounts = [
            (
                k8s.VolumeMount(
                    name=f"{self.node.id}-config",
                    mount_path="/config/sequencer/presets/",
                    read_only=True,
                )
            ),
            (
                k8s.VolumeMount(
                    name=f"{self.node.id}-secret",
                    mount_path=const.SECRETS_MOUNT_PATH,
                    read_only=True,
                )
                if self.service_topology.external_secret is not None
                else None
            ),
            (
                k8s.VolumeMount(
                    name=f"{self.node.id}-data",
                    mount_path="/data",
                    read_only=False,
                )
                if self.service_topology.storage
                else None
            ),
        ]
        return [vm for vm in volume_mounts if vm is not None]

    def _get_volumes(self) -> typing.List[k8s.Volume]:
        volumes = [
            (
                k8s.Volume(
                    name=f"{self.node.id}-config",
                    config_map=k8s.ConfigMapVolumeSource(name=f"{self.node.id}-config"),
                )
            ),
            (
                k8s.Volume(
                    name=f"{self.node.id}-secret",
                    secret=k8s.SecretVolumeSource(
                        secret_name=f"{self.node.id}-secret",
                        default_mode=400,
                        items=[
                            k8s.KeyToPath(
                                key=const.SECRETS_FILE_NAME,
                                path=const.SECRETS_FILE_NAME,
                            )
                        ],
                    ),
                )
                if self.service_topology.external_secret is not None
                else None
            ),
            (
                k8s.Volume(
                    name=f"{self.node.id}-data",
                    persistent_volume_claim=k8s.PersistentVolumeClaimVolumeSource(
                        claim_name=f"{self.node.id}-data", read_only=False
                    ),
                )
                if self.service_topology.storage
                else None
            ),
        ]
        return [v for v in volumes if v is not None]

    def _get_container_resources(self) -> k8s.ResourceRequirements:
        requests_cpu = str(self.service_topology.resources["requests"]["cpu"])
        requests_memory = str(self.service_topology.resources["requests"]["memory"]) + "Gi"
        limits_cpu = str(self.service_topology.resources["limits"]["cpu"])
        limits_memory = str(self.service_topology.resources["limits"]["memory"]) + "Gi"
        return k8s.ResourceRequirements(
            requests={
                "cpu": k8s.Quantity.from_string(requests_cpu),
                "memory": k8s.Quantity.from_string(requests_memory),
            },
            limits={
                "cpu": k8s.Quantity.from_string(limits_cpu),
                "memory": k8s.Quantity.from_string(limits_memory),
            },
        )

    @staticmethod
    def _get_container_env() -> typing.List[k8s.EnvVar]:
        return [
            k8s.EnvVar(name="RUST_LOG", value="debug"),
            k8s.EnvVar(name="RUST_BACKTRACE", value="full"),
            k8s.EnvVar(name="NO_COLOR", value="1"),
        ]

    def _get_container_args(self) -> typing.List[str]:
        args = ["--config_file", "/config/sequencer/presets/config"]
        if self.service_topology.external_secret is not None:
            args.append("--config_file")
            args.append(f"{const.SECRETS_MOUNT_PATH}/{const.SECRETS_FILE_NAME}")
        return args

    def _get_node_selector(self) -> typing.Dict[str, str]:
        if self.service_topology.toleration is not None:
            return {"role": self.service_topology.toleration}
        return None

    def _get_tolerations(self) -> typing.Sequence[k8s.Toleration]:
        if self.service_topology.toleration is not None:
            return [
                k8s.Toleration(
                    key="key",
                    operator="Equal",
                    value=self.service_topology.toleration,
                    effect="NoSchedule",
                ),
            ]
        return None

    def _get_pod_affinity_term(self, topology_key: str) -> k8s.PodAffinityTerm:
        match_labels = {"service": Names.to_label_value(self, include_hash=False)}
        return k8s.PodAffinityTerm(
            label_selector=k8s.LabelSelector(
                match_labels=match_labels,
            ),
            topology_key=topology_key,
            namespace_selector={},
        )

    def _get_weighted_pod_affinity_term(
        self, topology_key: str, weight: int
    ) -> k8s.WeightedPodAffinityTerm:
        return k8s.WeightedPodAffinityTerm(
            weight=weight, pod_affinity_term=self._get_pod_affinity_term(topology_key=topology_key)
        )

    def _get_affinity(self) -> k8s.Affinity:
        if self.service_topology.anti_affinity:
            return k8s.Affinity(
                pod_anti_affinity=k8s.PodAntiAffinity(
                    preferred_during_scheduling_ignored_during_execution=[
                        self._get_weighted_pod_affinity_term(
                            topology_key=const.AFFINITY_ZONE_TOPOLOGY["key"],
                            weight=const.AFFINITY_ZONE_TOPOLOGY["weight"],
                        ),
                        self._get_weighted_pod_affinity_term(
                            topology_key=const.AFFINITY_HOSTNAME_TOPOLOGY["key"],
                            weight=const.AFFINITY_HOSTNAME_TOPOLOGY["weight"],
                        ),
                    ],
                ),
            )
        return None
