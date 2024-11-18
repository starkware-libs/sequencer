import json

from typing import Optional, List
from constructs import Construct
from cdk8s import Names, ApiObjectMetadata
from imports import k8s
from imports.com.google import cloud as google

from services.objects import *



class Service(Construct):        
    def __init__(
        self,
        scope: Construct,
        id: str,
        *,
        image: str,
        replicas: int = 1,
        namespace: Optional[str] = None,
        service_type: Optional[ServiceType] = None,
        port_mappings: Optional[Sequence[PortMapping]] = None,
        deployment: Optional[bool] = False,
        statefulset: Optional[bool] = False,
        config: Optional[Config] = None,
        health_check: Optional[HealthCheck] = None,
        pvc: Optional[PersistentVolumeClaim] = None,
        ingress: Optional[Ingress] = None,
        args: Optional[List[str]] = None
    ):
        super().__init__(scope, id)

        self.namespace = namespace
        self.image = image
        self.label = {"app": Names.to_label_value(self, include_hash=False)}
        self.deployment = deployment
        self.statefulset = statefulset
        self.replicas = replicas
        self.service_type = service_type
        self.port_mappings = port_mappings
        self.config = config
        self.health_check = health_check
        self.pvc = pvc
        self.ingress = ingress
        self.args = args

        if namespace is not None:
            self.get_namespace()

        if port_mappings is not None:
            self.get_port_mappings()
        
        if config is not None:
            self.get_config_map()
            
        if self.deployment:
            self.get_deployment()

        if ingress is not None:
            self.get_ingress()
        
        if pvc is not None:
            self.get_pvc()
        

    def get_namespace(self):
        return k8s.KubeNamespace(
            self,
            "namespace",
            metadata=k8s.ObjectMeta(
                name=self.namespace
            )
        )


    def get_config_map(self):
        return k8s.KubeConfigMap(
            self,
            "configmap",
            metadata=k8s.ObjectMeta(
                name=f"{self.node.id}-config"
            ),
            data=dict(config=json.dumps(self.config.get())),
        )
        

    def get_deployment(self):
        return k8s.KubeDeployment(
            self,
            "deployment",
            spec=k8s.DeploymentSpec(
                replicas=self.replicas,
                selector=k8s.LabelSelector(match_labels=self.label),
                template=k8s.PodTemplateSpec(
                    metadata=k8s.ObjectMeta(labels=self.label),
                    spec=k8s.PodSpec(
                        security_context=k8s.PodSecurityContext(
                            fs_group=1000
                        ),
                        containers=[
                            k8s.Container(
                                name=f"{self.node.id}-container",
                                image=self.image,
                                # command=["sleep", "infinity"],
                                args=self.args or [],
                                ports=[k8s.ContainerPort(container_port=port_map.container_port) for port_map in self.port_mappings or []],
                                startup_probe=k8s.Probe(
                                    http_get=k8s.HttpGetAction(
                                        path=self.health_check.startup_probe.path,
                                        port=k8s.IntOrString.from_string(self.health_check.startup_probe.port) 
                                            if isinstance(self.health_check.startup_probe.port, str) 
                                            else k8s.IntOrString.from_number(self.health_check.startup_probe.port)
                                    ),
                                    period_seconds=self.health_check.startup_probe.period_seconds,
                                    failure_threshold=self.health_check.startup_probe.failure_threshold,
                                    timeout_seconds=self.health_check.startup_probe.timeout_seconds
                                ) if self.health_check.startup_probe is not None else None,

                                readiness_probe=k8s.Probe(
                                    http_get=k8s.HttpGetAction(
                                        path=self.health_check.readiness_probe.path,
                                        port=k8s.IntOrString.from_string(self.health_check.readiness_probe.port) 
                                            if isinstance(self.health_check.readiness_probe.port, str) 
                                            else k8s.IntOrString.from_number(self.health_check.readiness_probe.port)
                                    ),
                                    period_seconds=self.health_check.readiness_probe.period_seconds,
                                    failure_threshold=self.health_check.readiness_probe.failure_threshold,
                                    timeout_seconds=self.health_check.readiness_probe.timeout_seconds
                                ) if self.health_check.readiness_probe is not None else None,

                                liveness_probe=k8s.Probe(
                                    http_get=k8s.HttpGetAction(
                                        path=self.health_check.liveness_probe.path,
                                        port=k8s.IntOrString.from_string(self.health_check.liveness_probe.port) 
                                            if isinstance(self.health_check.liveness_probe.port, str) 
                                            else k8s.IntOrString.from_number(self.health_check.liveness_probe.port)
                                    ),
                                    period_seconds=self.health_check.liveness_probe.period_seconds,
                                    failure_threshold=self.health_check.liveness_probe.failure_threshold,
                                    timeout_seconds=self.health_check.liveness_probe.timeout_seconds
                                ) if self.health_check.liveness_probe is not None else None,

                                volume_mounts=[
                                    mount for mount in [
                                        k8s.VolumeMount(
                                            name=f"{self.node.id}-config",
                                            mount_path=self.config.mount_path,
                                            read_only=True
                                        ) if self.config is not None else None,

                                        k8s.VolumeMount(
                                            name=f"{self.node.id}-data",
                                            mount_path=self.pvc.mount_path,
                                            read_only=self.pvc.read_only,
                                        ) if self.pvc is not None else None
                                    ] if mount is not None
                                ]
                            )
                        ],
                        volumes=[
                            vol for vol in [
                                k8s.Volume(
                                    name=f"{self.node.id}-config",
                                    config_map=k8s.ConfigMapVolumeSource(name=f"{self.node.id}-config")
                                ) if self.config is not None else None,

                                k8s.Volume(
                                    name=f"{self.node.id}-data",
                                    persistent_volume_claim=k8s.PersistentVolumeClaimVolumeSource(claim_name=f"{self.node.id}-data", read_only=self.pvc.read_only)
                                ) if self.pvc is not None else None
                            ] if vol is not None
                        ] 
                    ),
                ),
            ),
        )

    def get_backend_config(self):
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
    
    def get_ingress(self):
        return k8s.KubeIngress(
            self,
            "ingress",
            metadata=k8s.ObjectMeta(
                name=f"{self.node.id}-ingress",
                labels=self.label,
                annotations=self.ingress.annotations
            ),
            spec=k8s.IngressSpec(
                ingress_class_name=self.ingress.class_name,
                tls=[
                    k8s.IngressTls(
                        hosts=tls.hosts,
                        secret_name=tls.secret_name
                    )
                    for tls in self.ingress.tls or []
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
                    for rule in self.ingress.rules or []
                ]
            )
        )

    def get_pvc(self):
        k8s.KubePersistentVolumeClaim(
            self,
            "pvc",
            metadata=k8s.ObjectMeta(
                name=f"{self.node.id}-data",
                labels=self.label
            ),
            spec=k8s.PersistentVolumeClaimSpec(
                storage_class_name=self.pvc.storage_class_name,
                access_modes=self.pvc.access_modes,
                volume_mode=self.pvc.volume_mode,
                resources=k8s.ResourceRequirements(
                    requests={"storage": k8s.Quantity.from_string(self.pvc.storage)}
                )
            )
        )

    def get_port_mappings(self):
        return k8s.KubeService(
                self,
                "service",
                spec=k8s.ServiceSpec(
                    type=self.service_type.value if self.service_type is not None else None,
                    ports=[
                        k8s.ServicePort(
                            name=port_map.name,
                            port=port_map.port,
                            target_port=k8s.IntOrString.from_number(port_map.container_port),
                        ) for port_map in self.port_mappings
                    ],
                    selector=self.label
                )
            )

