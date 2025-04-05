import json
import typing

from constructs import Construct
from cdk8s import Names, ApiObjectMetadata
from imports import k8s
from imports.io.external_secrets import (
    ExternalSecretV1Beta1 as ExternalSecret,
    ExternalSecretV1Beta1Spec as ExternalSecretSpec,
    ExternalSecretV1Beta1SpecData as ExternalSecretSpecData,
    ExternalSecretV1Beta1SpecTarget as ExternalSecretSpecTarget,
    ExternalSecretV1Beta1SpecDataRemoteRef as ExternalSecretSpecDataRemoteRef,
    ExternalSecretV1Beta1SpecSecretStoreRef as ExternalSecretSpecSecretStoreRef,
    ExternalSecretV1Beta1SpecSecretStoreRefKind as ExternalSecretSpecSecretStoreRefKind,
    ExternalSecretV1Beta1SpecDataRemoteRefConversionStrategy as ExternalSecretSpecDataRemoteRefConversionStrategy,
)
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

        self.configmap = self._get_configmap()

        self.service = self._get_service()

        if self.service_topology.controller_type == "deployment":
            self.controller = self._get_deployment()
        elif self.service_topology.controller_type == "statefulset":
            self.controller = self._get_statefulset()
        else:
            raise ValueError(f"Unknown controller type: {self.service_topology.controller_type}")

        if self.service_topology.ingress:
            self.service.metadata.add_annotation(
                key="cloud.google.com/neg", value='{"ingress": true}'
            )
            self.ingress = self._get_ingress()

        if self.service_topology.storage is not None:
            self.pvc = self._get_persistent_volume_claim()

        if self.service_topology.autoscale:
            self.hpa = self._get_hpa()

        if self.service_topology.external_secret is not None:
            self.external_secret = self._get_external_secret()

    def _get_configmap(self) -> k8s.KubeConfigMap:
        return k8s.KubeConfigMap(
            self,
            "configmap",
            metadata=k8s.ObjectMeta(name=f"{self.node.id}-config"),
            data=dict(config=json.dumps(self.service_topology.config.get_config(), indent=2)),
        )

    def _get_service(self) -> k8s.KubeService:
        return k8s.KubeService(
            self,
            "service",
            metadata=k8s.ObjectMeta(labels=self.labels),
            spec=k8s.ServiceSpec(
                type=const.K8SServiceType.CLUSTER_IP,
                ports=self._get_service_ports(),
                selector=self.labels,
            ),
        )

    def _get_deployment(self) -> k8s.KubeDeployment:
        return k8s.KubeDeployment(
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
                        tolerations=self._get_tolerations(),
                        node_selector=self._get_node_selector(),
                        containers=[
                            k8s.Container(
                                name=self.node.id,
                                image=self.service_topology.image,
                                image_pull_policy="Always",
                                env=self._get_container_env(),
                                args=self._get_container_args(),
                                ports=self._get_container_ports(),
                                startup_probe=self._get_http_probe(),
                                readiness_probe=self._get_http_probe(
                                    path=const.PROBE_MONITORING_READY_PATH
                                ),
                                liveness_probe=self._get_http_probe(),
                                volume_mounts=self._get_volume_mounts(),
                                resources=self._get_container_resources(),
                            )
                        ],
                    ),
                ),
            ),
        )

    def _get_statefulset(self) -> k8s.KubeStatefulSet:
        return k8s.KubeStatefulSet(
            self,
            "statefulset",
            metadata=k8s.ObjectMeta(labels=self.labels),
            spec=k8s.StatefulSetSpec(
                service_name=f"{self.node.id}-service",
                replicas=self.service_topology.replicas,
                selector=k8s.LabelSelector(match_labels=self.labels),
                template=k8s.PodTemplateSpec(
                    metadata=k8s.ObjectMeta(labels=self.labels),
                    spec=k8s.PodSpec(
                        security_context=k8s.PodSecurityContext(fs_group=1000),
                        volumes=self._get_volumes(),
                        tolerations=self._get_tolerations(),
                        node_selector=self._get_node_selector(),
                        containers=[
                            k8s.Container(
                                name=self.node.id,
                                image=self.service_topology.image,
                                image_pull_policy="Always",
                                env=self._get_container_env(),
                                args=self._get_container_args(),
                                ports=self._get_container_ports(),
                                startup_probe=self._get_http_probe(),
                                readiness_probe=self._get_http_probe(
                                    path=const.PROBE_MONITORING_READY_PATH
                                ),
                                liveness_probe=self._get_http_probe(),
                                volume_mounts=self._get_volume_mounts(),
                                resources=self._get_container_resources(),
                            )
                        ],
                    ),
                ),
            ),
        )

    def _get_external_secret(self) -> ExternalSecret:
        return ExternalSecret(
            self,
            "external-secret",
            metadata=ApiObjectMetadata(labels=self.labels),
            spec=ExternalSecretSpec(
                secret_store_ref=ExternalSecretSpecSecretStoreRef(
                    kind=ExternalSecretSpecSecretStoreRefKind.CLUSTER_SECRET_STORE,
                    name="external-secrets",
                ),
                refresh_interval="1m",
                target=ExternalSecretSpecTarget(
                    name=f"{self.node.id}-secret",
                ),
                data=[
                    ExternalSecretSpecData(
                        secret_key="secrets.json",
                        remote_ref=ExternalSecretSpecDataRemoteRef(
                            key=self.service_topology.external_secret["gcsm_key"],
                            conversion_strategy=ExternalSecretSpecDataRemoteRefConversionStrategy.DEFAULT,
                        ),
                    ),
                ],
            ),
        )

    def _get_hpa(self) -> k8s.KubeHorizontalPodAutoscalerV2:
        return k8s.KubeHorizontalPodAutoscalerV2(
            self,
            "hpa",
            metadata=k8s.ObjectMeta(labels=self.labels),
            spec=k8s.HorizontalPodAutoscalerSpecV2(
                min_replicas=self.service_topology.replicas,
                max_replicas=const.HPA_MAX_REPLICAS,
                scale_target_ref=k8s.CrossVersionObjectReferenceV2(
                    api_version=self.controller.api_version,
                    kind=self.controller.kind,
                    name=self.controller.metadata.name,
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
                    scale_down=k8s.HpaScalingRulesV2(
                        select_policy="Max",
                        stabilization_window_seconds=300,
                        policies=[
                            k8s.HpaScalingPolicyV2(
                                type="Pods",
                                value=2,  # Remove 2 pods per scaling action
                                period_seconds=60,  # Scaling happens at most once per minute
                            )
                        ],
                    ),
                ),
            ),
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
        volume_mounts = (
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
                    mount_path="/etc/secrets",
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
        )

        return [vm for vm in volume_mounts if vm is not None]

    def _get_volumes(self) -> typing.List[k8s.Volume]:
        volumes = (
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
                                key="secrets.json",
                                path="secrets.json",
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
        )

        return [v for v in volumes if v is not None]

    def _get_ingress(self) -> k8s.KubeIngress:
        domain = self.service_topology.ingress["domain"]
        self.host = f"{self.node.id}.{self.namespace}.{domain}"
        ingress_class_name = None
        annotations = {
            "kubernetes.io/tls-acme": "true",
            "cert-manager.io/common-name": self.host,
            "cert-manager.io/issue-temporary-certificate": "true",
            "cert-manager.io/issuer": "letsencrypt-prod",
            "acme.cert-manager.io/http01-edit-in-place": "true",
        }
        tls = self._get_ingress_tls()

        if self.service_topology.ingress["internal"] == True:
            annotations.clear()
            annotations.update({"kubernetes.io/ingress.class": "gce-internal"})
            tls = None

        return k8s.KubeIngress(
            self,
            "ingress",
            metadata=k8s.ObjectMeta(
                name=f"{self.node.id}-ingress",
                labels=self.labels,
                annotations=annotations,
            ),
            spec=k8s.IngressSpec(
                tls=tls,
                rules=self._get_ingress_rules(component=self.service_topology.component),
                ingress_class_name=ingress_class_name,
            ),
        )

    def _get_ingress_rules(self, component: str) -> typing.List[k8s.IngressRule]:
        paths = []
        if component == "http_server" or "consolidated":
            paths.append(self._get_ingress_path("/gateway"))
            if self.service_topology.ingress.get("fgw-nginx-redirect") is not None:
                paths.append(self._get_ingress_path("/feeder-gateway"))
        elif component == "state_sync":
            paths.append(self._get_ingress_path("/state-sync"))
        elif component == "mempool":
            paths.append(self._get_ingress_path("/mempool"))
        elif component == "consensus_manager":
            paths.append(self._get_ingress_path("/consensus-manager"))

        return [
            k8s.IngressRule(
                host=self.host,
                http=k8s.HttpIngressRuleValue(
                    paths=paths,
                ),
            ),
        ]

    def _get_ingress_tls(self) -> typing.List[k8s.IngressTls]:
        return [k8s.IngressTls(hosts=[self.host], secret_name=f"{self.node.id}-tls")]

    def _get_ingress_path(self, path: str) -> k8s.HttpIngressPath:
        backend_name = f"{self.node.id}-service"
        if path == "/gateway":
            port = self._get_config_attr("http_server_config.port")
        elif path == "/feeder-gateway":
            # TODO(Idan): Create Nginx service to redirect feeder-gateway, provide its port.
            port = 8080
            backend_name = "nginx-service"
        elif path == "/mempool":
            port = self._get_config_attr("mempool_p2p_config.network_config.port")
        elif path == "/consensus-manager":
            port = self._get_config_attr("consensus_manager_config.network_config.port")
        else:
            raise ValueError(f"Unknown path: {path}")

        return k8s.HttpIngressPath(
            path=path,
            path_type="Prefix",
            backend=k8s.IngressBackend(
                service=k8s.IngressServiceBackend(
                    name=backend_name,
                    port=k8s.ServiceBackendPort(number=port),
                )
            ),
        )

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
            # TODO(Elin): consider a better way to uncolor app logs, maybe up the stack towards GCP.
            k8s.EnvVar(name="NO_COLOR", value="1"),
        ]

    def _get_container_args(self) -> typing.List[str]:
        args = ["--config_file", "/config/sequencer/presets/config"]
        if self.service_topology.external_secret is not None:
            args.append("--config_file")
            args.append("/etc/secrets/secrets.json")

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
