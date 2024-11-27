import json

from itertools import chain
from constructs import Construct
from cdk8s import Names, ApiObjectMetadata
from imports import k8s
from imports.com.google import cloud as google

from services import topology


class ServiceApp(Construct):        
    def __init__(
        self,
        scope: Construct,
        id: str,
        *,
        namespace: str,
        topology: topology.ServiceTopology
    ):
        super().__init__(scope, id)

        self.namespace = namespace
        self.label = {"app": Names.to_label_value(self, include_hash=False)}
        self.topology = topology
        
        self.set_k8s_namespace()

        if topology.service is not None:
            self.set_k8s_service()
        
        if topology.config is not None:
            self.set_k8s_configmap()
            
        if topology.deployment is not None:
            self.set_k8s_deployment()

        if topology.ingress is not None:
            self.set_k8s_ingress()
        
        if topology.pvc is not None:
            self.set_k8s_pvc()

    def set_k8s_namespace(self):
        return k8s.KubeNamespace(
            self,
            "namespace",
            metadata=k8s.ObjectMeta(
                name=self.namespace
            )
        )

    def set_k8s_configmap(self):
        return k8s.KubeConfigMap(
            self,
            "configmap",
            metadata=k8s.ObjectMeta(
                name=f"{self.node.id}-config"
            ),
            data=dict(config=json.dumps(self.topology.config.get())),
        )
        
    def set_k8s_service(self):
        return k8s.KubeService(
                self,
                "service",
                spec=k8s.ServiceSpec(
                    type=self.topology.service.type.value,
                    ports=[
                        k8s.ServicePort(
                            name=port.name,
                            port=port.port,
                            target_port=k8s.IntOrString.from_number(port.container_port),
                        ) for port in self.topology.service.ports
                    ],
                    selector=self.label
                )
            )

    def set_k8s_deployment(self):
        return k8s.KubeDeployment(
            self,
            "deployment",
            spec=k8s.DeploymentSpec(
                replicas=self.topology.deployment.replicas,
                selector=k8s.LabelSelector(match_labels=self.label),
                template=k8s.PodTemplateSpec(
                    metadata=k8s.ObjectMeta(labels=self.label),
                    spec=k8s.PodSpec(
                        security_context=k8s.PodSecurityContext(
                            fs_group=1000
                        ),
                        containers=[
                            k8s.Container(
                                name=f"{self.node.id}-{container.name}",
                                image=container.image,
                                # command=["sleep", "infinity"],
                                args=container.args,
                                ports=[
                                    k8s.ContainerPort(
                                        container_port=port.container_port
                                    ) for port in container.ports
                                ],
                                startup_probe=k8s.Probe(
                                    http_get=k8s.HttpGetAction(
                                        path=container.startup_probe.path,
                                        port=k8s.IntOrString.from_string(container.startup_probe.port) 
                                            if isinstance(container.startup_probe.port, str) 
                                            else k8s.IntOrString.from_number(container.startup_probe.port)
                                    ),
                                    period_seconds=container.startup_probe.period_seconds,
                                    failure_threshold=container.startup_probe.failure_threshold,
                                    timeout_seconds=container.startup_probe.timeout_seconds
                                ) if container.startup_probe is not None else None,

                                readiness_probe=k8s.Probe(
                                    http_get=k8s.HttpGetAction(
                                        path=container.readiness_probe.path,
                                        port=k8s.IntOrString.from_string(container.readiness_probe.port) 
                                            if isinstance(container.readiness_probe.port, str) 
                                            else k8s.IntOrString.from_number(container.readiness_probe.port)
                                    ),
                                    period_seconds=container.readiness_probe.period_seconds,
                                    failure_threshold=container.readiness_probe.failure_threshold,
                                    timeout_seconds=container.readiness_probe.timeout_seconds
                                ) if container.readiness_probe is not None else None,

                                liveness_probe=k8s.Probe(
                                    http_get=k8s.HttpGetAction(
                                        path=container.liveness_probe.path,
                                        port=k8s.IntOrString.from_string(container.liveness_probe.port) 
                                            if isinstance(container.liveness_probe.port, str) 
                                            else k8s.IntOrString.from_number(container.liveness_probe.port)
                                    ),
                                    period_seconds=container.liveness_probe.period_seconds,
                                    failure_threshold=container.liveness_probe.failure_threshold,
                                    timeout_seconds=container.liveness_probe.timeout_seconds
                                ) if container.liveness_probe is not None else None,

                                volume_mounts=[
                                    k8s.VolumeMount(
                                        name=mount.name,
                                        mount_path=mount.mount_path,
                                        read_only=mount.read_only
                                    ) for mount in container.volume_mounts
                                ]
                            ) for container in self.topology.deployment.containers
                        ],
                        volumes=list(
                            chain(
                                (
                                    k8s.Volume(
                                        name=f"{self.node.id}-{volume.name}", 
                                        config_map=k8s.ConfigMapVolumeSource(
                                            name=f"{self.node.id}-{volume.name}"
                                        )
                                    ) for volume in self.topology.deployment.configmap_volumes
                                ) if self.topology.deployment.configmap_volumes is not None else None,
                                (
                                    k8s.Volume(
                                        name=f"{self.node.id}-{volume.name}",
                                        persistent_volume_claim=k8s.PersistentVolumeClaimVolumeSource(
                                            claim_name=f"{self.node.id}-{volume.name}",
                                            read_only=volume.read_only
                                        )
                                    ) for volume in self.topology.deployment.pvc_volumes
                                ) if self.topology.deployment is not None else None
                            )
                        )
                    ),
                ),
            ),
        )

    def set_k8s_ingress(self):
        return k8s.KubeIngress(
            self,
            "ingress",
            metadata=k8s.ObjectMeta(
                name=f"{self.node.id}-ingress",
                labels=self.label,
                annotations=self.topology.ingress.annotations
            ),
            spec=k8s.IngressSpec(
                ingress_class_name=self.topology.ingress.class_name,
                tls=[
                    k8s.IngressTls(
                        hosts=tls.hosts,
                        secret_name=tls.secret_name
                    )
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
                                            )
                                        )
                                    )
                                )
                                for path in rule.paths or []
                            ]
                        )
                    )
                    for rule in self.topology.ingress.rules or []
                ]
            )
        )

    def set_k8s_pvc(self):
        k8s.KubePersistentVolumeClaim(
            self,
            "pvc",
            metadata=k8s.ObjectMeta(
                name=f"{self.node.id}-data",
                labels=self.label
            ),
            spec=k8s.PersistentVolumeClaimSpec(
                storage_class_name=self.topology.pvc.storage_class_name,
                access_modes=self.topology.pvc.access_modes,
                volume_mode=self.topology.pvc.volume_mode,
                resources=k8s.ResourceRequirements(
                    requests={"storage": k8s.Quantity.from_string(self.topology.pvc.storage)}
                )
            )
        )

    def set_k8s_backend_config(self):
        return google.BackendConfig(
            self,
            "backendconfig",
            metadata=ApiObjectMetadata(
                name=f"{self.node.id}-backendconfig",
                labels=self.label
            ),
            spec=google.BackendConfigSpec(
                health_check=google.BackendConfigSpecHealthCheck(
                    check_interval_sec=5,
                    healthy_threshold=10,
                    unhealthy_threshold=5,
                    timeout_sec=5,
                    request_path="/",
                    type="http"
                ),
                iap=google.BackendConfigSpecIap(
                    enabled=True,
                    oauthclient_credentials=google.BackendConfigSpecIapOauthclientCredentials(
                        secret_name=""
                    )
                )
            )
        )
