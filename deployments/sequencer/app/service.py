import json
import typing

from constructs import Construct
from cdk8s import Names
from imports import k8s
from services import topology, const


class ServiceApp(Construct):
    def __init__(
        self,
        scope: Construct,
        id: str,
        *,
        namespace: str,
        service_topology: topology.ServiceTopology,
    ):
        super().__init__(scope, id)

        self.namespace = namespace
        self.labels = {
            "app": "sequencer",
            "service": Names.to_label_value(self, include_hash=False),
        }
        self.service_topology = service_topology
        self.node_config = service_topology.config.get_config()

        self.config_map = k8s.KubeConfigMap(
            self,
            "configmap",
            metadata=k8s.ObjectMeta(name=f"{self.node.id}-config"),
            data=dict(config=json.dumps(self.service_topology.config.get_config(), indent=2)),
        )

        self.service = k8s.KubeService(
            self,
            "service",
            spec=k8s.ServiceSpec(
                type=const.ServiceType.CLUSTER_IP,
                ports=self._get_service_ports(),
                selector=self.labels,
            ),
        )

        self.deployment = k8s.KubeDeployment(
            self,
            "deployment",
            metadata=k8s.ObjectMeta(labels=self.labels),
            spec=k8s.DeploymentSpec(
                replicas=self.service_topology.replicas,
                selector=k8s.LabelSelector(match_labels=self.labels),
                template=k8s.PodTemplateSpec(
                    metadata=k8s.ObjectMeta(labels=self.labels),
                    spec=k8s.PodSpec(
                        security_context=k8s.PodSecurityContext(fs_group=1000),
                        volumes=self._get_volumes(),
                        containers=[
                            k8s.Container(
                                name=self.node.id,
                                image=self.service_topology.image,
                                image_pull_policy="Always",
                                env=self._get_container_env(),
                                args=const.CONTAINER_ARGS,
                                ports=self._get_container_ports(),
                                startup_probe=self._get_http_probe(),
                                readiness_probe=self._get_http_probe(
                                    path=const.PROBE_MONITORING_READY_PATH
                                ),
                                liveness_probe=self._get_http_probe(),
                                volume_mounts=self._get_volume_mounts(),
                            )
                        ],
                    ),
                ),
            ),
        )

        if self.service_topology.ingress:
            self.ingress = self._get_ingress()

        if self.service_topology.storage is not None:
            self.pvc = self._get_persistent_volume_claim()

        if self.service_topology.autoscale:
            self.hpa = self._get_hpa()

    def _get_hpa(self) -> k8s.KubeHorizontalPodAutoscalerV2:
        return k8s.KubeHorizontalPodAutoscalerV2(
            self,
            "hpa",
            metadata=k8s.ObjectMeta(labels=self.labels),
            spec=k8s.HorizontalPodAutoscalerSpecV2(
                min_replicas=self.service_topology.replicas,
                max_replicas=const.HPA_MAX_REPLICAS,
                scale_target_ref=k8s.CrossVersionObjectReferenceV2(
                    api_version="v1", kind="Deployment", name=self.deployment.name
                ),
                metrics=[
                    k8s.MetricSpecV2(
                        type="Resource",
                        resource=k8s.ResourceMetricSourceV2(
                            name="cpu",
                            target=k8s.MetricTargetV2(type="Utilization", average_utilization=50),
                        ),
                    )
                ],
                behavior=k8s.HorizontalPodAutoscalerBehaviorV2(
                    scale_up=k8s.HpaScalingRulesV2(
                        select_policy="Max",  # Choose the highest scaling policy
                        stabilization_window_seconds=300,
                        policies=[
                            k8s.HpaScalingPolicyV2(
                                type="Pods",
                                value=2,  # Add 2 pods per scaling action
                                period_seconds=60,  # Scaling happens at most once per minute
                            )
                        ],
                    ),
                ),
            ),
        )

    def _get_ingress(self) -> k8s.KubeIngress:
        self.host = f"{self.node.id}.{self.namespace}.sw-dev.io"
        return k8s.KubeIngress(
            self,
            "ingress",
            metadata=k8s.ObjectMeta(
                name=f"{self.node.id}-ingress",
                labels=self.labels,
                annotations={
                    "kubernetes.io/tls-acme": "true",
                    "cert-manager.io/common-name": self.host,
                    "cert-manager.io/issue-temporary-certificate": "true",
                    "cert-manager.io/issuer": "letsencrypt-prod",
                    "acme.cert-manager.io/http01-edit-in-place": "true",
                },
            ),
            spec=k8s.IngressSpec(tls=self._get_ingress_tls(), rules=self._get_ingress_rules()),
        )

    def _get_persistent_volume_claim(self) -> k8s.KubePersistentVolumeClaim:
        return k8s.KubePersistentVolumeClaim(
            self,
            "pvc",
            metadata=k8s.ObjectMeta(name=f"{self.node.id}-data", labels=self.labels),
            spec=k8s.PersistentVolumeClaimSpec(
                storage_class_name=const.PVC_STORAGE_CLASS_NAME,
                access_modes=const.PVC_ACCESS_MODE,
                volume_mode=const.PVC_VOLUME_MODE,
                resources=k8s.ResourceRequirements(
                    requests={
                        "storage": k8s.Quantity.from_string(f"{self.service_topology.storage}Gi")
                    }
                ),
            ),
        )

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

    def _get_service_ports(self) -> typing.List[k8s.ServicePort]:
        return [
            k8s.ServicePort(
                name=attr[0],
                port=self._get_config_attr(attr[1]),
                target_port=k8s.IntOrString.from_number(self._get_config_attr(attr[1])),
            )
            for attr in self._get_ports_subset_keys_from_config()
        ]

    def _get_http_probe(
        self,
        period_seconds: int = const.PROBE_PERIOD_SECONDS,
        failure_threshold: int = const.PROBE_FAILURE_THRESHOLD,
        timeout_seconds: int = const.PROBE_TIMEOUT_SECONDS,
        path: str = const.PROBE_MONITORING_ALIVE_PATH,
    ) -> k8s.Probe:
        port = self._get_config_attr("monitoring_endpoint_config.port")

        return k8s.Probe(
            http_get=k8s.HttpGetAction(
                path=path,
                port=k8s.IntOrString.from_number(port),
            ),
            period_seconds=period_seconds,
            failure_threshold=failure_threshold,
            timeout_seconds=timeout_seconds,
        )

    def _get_volume_mounts(self) -> typing.List[k8s.VolumeMount]:
        volume_mounts = [
            k8s.VolumeMount(
                name=f"{self.node.id}-config",
                mount_path="/config/sequencer/presets/",
                read_only=True,
            ),
            (
                k8s.VolumeMount(name=f"{self.node.id}-data", mount_path="/data", read_only=False)
                if self.service_topology.storage
                else None
            ),
        ]

        return [vm for vm in volume_mounts if vm is not None]

    def _get_volumes(self) -> typing.List[k8s.Volume]:
        return [
            k8s.Volume(
                name=f"{self.node.id}-config",
                config_map=k8s.ConfigMapVolumeSource(name=f"{self.node.id}-config"),
            ),
            k8s.Volume(
                name=f"{self.node.id}-data",
                persistent_volume_claim=k8s.PersistentVolumeClaimVolumeSource(
                    claim_name=f"{self.node.id}-data", read_only=False
                ),
            ),
        ]

    def _get_ingress_rules(self) -> typing.List[k8s.IngressRule]:
        return [
            k8s.IngressRule(
                host=self.host,
                http=k8s.HttpIngressRuleValue(
                    paths=[
                        k8s.HttpIngressPath(
                            path="/monitoring",
                            path_type="Prefix",
                            backend=k8s.IngressBackend(
                                service=k8s.IngressServiceBackend(
                                    name=f"{self.node.id}-service",
                                    port=k8s.ServiceBackendPort(
                                        number=self._get_config_attr(
                                            "monitoring_endpoint_config.port"
                                        )
                                    ),
                                )
                            ),
                        )
                    ]
                ),
            )
        ]

    def _get_ingress_tls(self) -> typing.List[k8s.IngressTls]:
        return [k8s.IngressTls(hosts=[self.host], secret_name=f"{self.node.id}-tls")]

    @staticmethod
    def _get_container_env() -> typing.List[k8s.EnvVar]:
        return [
            k8s.EnvVar(name="RUST_LOG", value="debug"),
            k8s.EnvVar(name="RUST_BACKTRACE", value="full"),
        ]

    @staticmethod
    def _get_node_selector() -> typing.Dict[str, str]:
        return {"role": "sequencer"}

    @staticmethod
    def _get_tolerations() -> typing.Sequence[k8s.Toleration]:
        return [
            k8s.Toleration(key="role", operator="Equal", value="sequencer", effect="NoSchedule"),
        ]
