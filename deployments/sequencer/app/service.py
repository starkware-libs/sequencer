import json
import typing

from itertools import chain
from constructs import Construct
from cdk8s import Names, ApiObjectMetadata
from imports import k8s
from imports.com.google import cloud as google

from services import topology, const


class ServiceApp(Construct):
    def __init__(
        self,
        scope: Construct,
        id: str,
        *,
        namespace: str,
        topology: topology.ServiceTopology,
    ):
        super().__init__(scope, id)

        self.namespace = namespace
        self.label = {"app": Names.to_label_value(self, include_hash=False)}
        self.topology = topology
        self.node_config = topology.config.get_config()

        k8s.KubeNamespace(self, "namespace", metadata=k8s.ObjectMeta(name=self.namespace))

        k8s.KubeConfigMap(
            self,
            "configmap",
            metadata=k8s.ObjectMeta(name=f"{self.node.id}-config"),
            data=dict(config=json.dumps(self.topology.config.get_config())),
        )

        k8s.KubeService(
            self,
            "service",
            spec=k8s.ServiceSpec(
                type=const.ServiceType.CLUSTER_IP,
                ports=self._get_service_ports(),
                selector=self.label,
            ),
        )

        k8s.KubeDeployment(
            self,
            "deployment",
            spec=k8s.DeploymentSpec(
                replicas=self.topology.deployment.replicas,
                selector=k8s.LabelSelector(match_labels=self.label),
                template=k8s.PodTemplateSpec(
                    metadata=k8s.ObjectMeta(labels=self.label),
                    spec=k8s.PodSpec(
                        security_context=k8s.PodSecurityContext(fs_group=1000),
                        containers=[
                            k8s.Container(
                                name=self.node.id,
                                image=container.image,
                                # command=["sleep", "infinity"],
                                args=container.args,
                                ports=self._get_container_ports(),
                                startup_probe=self._get_http_probe(),
                                readiness_probe=self._get_http_probe(),
                                liveness_probe=self._get_http_probe(),
                                volume_mounts=[
                                    k8s.VolumeMount(
                                        name=mount.name,
                                        mount_path=mount.mount_path,
                                        read_only=mount.read_only,
                                    )
                                    for mount in container.volume_mounts
                                ],
                            )
                            for container in self.topology.deployment.containers
                        ],
                        volumes=list(
                            chain(
                                (
                                    (
                                        k8s.Volume(
                                            name=f"{self.node.id}-{volume.name}",
                                            config_map=k8s.ConfigMapVolumeSource(
                                                name=f"{self.node.id}-{volume.name}"
                                            ),
                                        )
                                        for volume in self.topology.deployment.configmap_volumes
                                    )
                                    if self.topology.deployment.configmap_volumes is not None
                                    else None
                                ),
                                (
                                    (
                                        k8s.Volume(
                                            name=f"{self.node.id}-{volume.name}",
                                            persistent_volume_claim=k8s.PersistentVolumeClaimVolumeSource(
                                                claim_name=f"{self.node.id}-{volume.name}",
                                                read_only=volume.read_only,
                                            ),
                                        )
                                        for volume in self.topology.deployment.pvc_volumes
                                    )
                                    if self.topology.deployment is not None
                                    else None
                                ),
                            )
                        ),
                    ),
                ),
            ),
        )

        k8s.KubeIngress(
            self,
            "ingress",
            metadata=k8s.ObjectMeta(
                name=f"{self.node.id}-ingress",
                labels=self.label,
                annotations=self.topology.ingress.annotations,
            ),
            spec=k8s.IngressSpec(
                ingress_class_name=self.topology.ingress.class_name,
                tls=[
                    k8s.IngressTls(hosts=tls.hosts, secret_name=f"{self.node.id}-tls")
                    for tls in self.topology.ingress.tls or []
                ],
                rules=[
                    k8s.IngressRule(
                        host=rule.host,
                        http=k8s.HttpIngressRuleValue(
                            paths=[
                                k8s.HttpIngressPath(
                                    path=path.path,
                                    path_type=path.path_type,
                                    backend=k8s.IngressBackend(
                                        service=k8s.IngressServiceBackend(
                                            name=path.backend_service_name,
                                            port=k8s.ServiceBackendPort(
                                                number=path.backend_service_port_number
                                            ),
                                        )
                                    ),
                                )
                                for path in rule.paths or []
                            ]
                        ),
                    )
                    for rule in self.topology.ingress.rules or []
                ],
            ),
        )

        k8s.KubePersistentVolumeClaim(
            self,
            "pvc",
            metadata=k8s.ObjectMeta(name=f"{self.node.id}-data", labels=self.label),
            spec=k8s.PersistentVolumeClaimSpec(
                storage_class_name=self.topology.pvc.storage_class_name,
                access_modes=self.topology.pvc.access_modes,
                volume_mode=self.topology.pvc.volume_mode,
                resources=k8s.ResourceRequirements(
                    requests={"storage": k8s.Quantity.from_string(self.topology.pvc.storage)}
                ),
            ),
        )


    def _get_config_attr(self, attribute):
        config_attr = self.node_config.get(attribute).get('value')
        if config_attr is None:
            assert f'Config attribute "{attribute}" is missing.'
        else:
            return config_attr

    def _get_container_ports(self):
        return [
            k8s.ContainerPort(
                container_port=self._get_config_attr(port)
            ) for port in ["http_server_config.port", "monitoring_endpoint_config.port"]
        ]

    def _get_container_resources(self):
        pass

    def _get_service_ports(self):
        return [
            k8s.ServicePort(
                name=attr.split("_")[0],
                port=self._get_config_attr(attr),
                target_port=k8s.IntOrString.from_number(self._get_config_attr(attr))
            ) for attr in ["http_server_config.port", "monitoring_endpoint_config.port"]
        ]

    def _get_http_probe(
            self,
            period_seconds: int = const.PROBE_PERIOD_SECONDS,
            failure_threshold: int = const.PROBE_FAILURE_THRESHOLD,
            timeout_seconds: int = const.PROBE_TIMEOUT_SECONDS
    ):
        path = "/monitoring/alive"
        # path = self.node_config['monitoring_path'].get("value") # TODO add monitoring path in node_config
        port = self.node_config.get('monitoring_endpoint_config.port').get("value")

        return k8s.Probe(
            http_get=k8s.HttpGetAction(
                path=path,
                port=k8s.IntOrString.from_number(port),
            ),
            period_seconds=period_seconds,
            failure_threshold=failure_threshold,
            timeout_seconds=timeout_seconds,
        )

    def _get_ingress_rules(self):
        pass

    def _get_ingress_paths(self):
        pass

    def _get_ingress_tls(self):
        pass

