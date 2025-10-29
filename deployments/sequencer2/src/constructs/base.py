from constructs import Construct
from imports import k8s

from src.config.schema import CommonConfig, Probe as ProbeConfig, ServiceConfig


class BaseConstruct(Construct):
    def __init__(
        self,
        scope: Construct,
        id: str,
        common_config: CommonConfig,
        service_config: ServiceConfig,
        labels,
        monitoring_endpoint_port,
    ):
        super().__init__(scope, id)
        self.common_config = common_config
        self.service_config = service_config
        self.labels = labels
        self.monitoring_endpoint_port = monitoring_endpoint_port

    def _get_container_ports(self) -> list[k8s.ContainerPort]:
        ports = []
        if self.service_config.service and self.service_config.service.ports:
            for p in self.service_config.service.ports:
                ports.append(
                    k8s.ContainerPort(container_port=p.port, name=p.name, protocol=p.protocol)
                )
        return ports

    def _get_http_probe(self, probe_config: ProbeConfig) -> k8s.Probe:
        if not probe_config or not probe_config.enabled:
            return None

        # Find the port number from the service definition
        port_number = self.monitoring_endpoint_port
        if self.service_config.service and self.service_config.service.ports:
            # Assuming the first port is the target for the probe if not specified otherwise
            port_number = self.service_config.service.ports[0].port

        return k8s.Probe(
            http_get=k8s.HttpGetAction(
                path=probe_config.path,
                port=k8s.IntOrString.from_number(port_number),
                scheme=probe_config.probeScheme,
            ),
            period_seconds=probe_config.periodSeconds,
            failure_threshold=probe_config.failureThreshold,
            success_threshold=probe_config.successThreshold,
            timeout_seconds=probe_config.timeoutSeconds,
        )

    def _get_volume_mounts(self) -> list[k8s.VolumeMount]:
        volume_mounts = []
        if self.service_config.persistentVolume and self.service_config.persistentVolume.enabled:
            volume_mounts.append(
                k8s.VolumeMount(
                    name=f"{self.service_config.name}-data",
                    mount_path=self.service_config.persistentVolume.mountPath,
                )
            )
        # TODO: Add back configmap and secret mounts when they are properly implemented
        return volume_mounts

    def _get_volumes(self) -> list[k8s.Volume]:
        volumes = []
        if self.service_config.persistentVolume and self.service_config.persistentVolume.enabled:
            pvc_name = (
                self.service_config.persistentVolume.existingClaim
                if self.service_config.persistentVolume.existingClaim
                else f"{self.service_config.name}-data"
            )
            volumes.append(
                k8s.Volume(
                    name=f"{self.service_config.name}-data",
                    persistent_volume_claim=k8s.PersistentVolumeClaimVolumeSource(
                        claim_name=pvc_name
                    ),
                )
            )
        # TODO: Add back configmap and secret volumes
        return volumes

    def _get_container_resources(self) -> k8s.ResourceRequirements:
        if not self.service_config.resources:
            return None
        requests = self.service_config.resources.requests
        limits = self.service_config.resources.limits
        return k8s.ResourceRequirements(
            requests=(
                {
                    "cpu": k8s.Quantity.from_string(str(requests.cpu)),
                    "memory": k8s.Quantity.from_string(requests.memory),
                }
                if requests
                else None
            ),
            limits=(
                {
                    "cpu": k8s.Quantity.from_string(str(limits.cpu)),
                    "memory": k8s.Quantity.from_string(limits.memory),
                }
                if limits
                else None
            ),
        )

    def _get_container_env(self) -> list[k8s.EnvVar]:
        env = []
        for e in self.service_config.env:
            # The pydantic model can handle different structures, this is a simple example
            if "name" in e and "value" in e:
                env.append(k8s.EnvVar(name=e["name"], value=str(e["value"])))
        return env

    def _get_node_selector(self) -> dict[str, str]:
        return self.service_config.nodeSelector

    def _get_tolerations(self) -> list[k8s.Toleration]:
        tolerations = []
        for t in self.service_config.tolerations:
            tolerations.append(
                k8s.Toleration(
                    key=t.get("key"),
                    operator=t.get("operator"),
                    value=t.get("value"),
                    effect=t.get("effect"),
                )
            )
        return tolerations

    def _get_affinity(self) -> k8s.Affinity:
        return (
            k8s.Affinity.from_json(self.service_config.affinity)
            if self.service_config.affinity
            else None
        )
