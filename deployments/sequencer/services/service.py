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
        service_type: Optional[ServiceType] = None,
        port_mappings: Optional[Sequence[PortMapping]] = None,
        config: Optional[Config] = None,
        health_check: Optional[HealthCheck] = None,
        pvc: Optional[PersistentVolumeClaim] = None,
        ingress: Optional[Ingress] = None,
        args: Optional[List[str]] = None
    ):
        super().__init__(scope, id)

        label = {"app": Names.to_label_value(self, include_hash=False)}

        if port_mappings is not None:
            k8s.KubeService(
                self,
                "service",
                spec=k8s.ServiceSpec(
                    type=service_type.value if service_type is not None else None,
                    ports=[
                        k8s.ServicePort(
                            name=port_map.name,
                            port=port_map.port,
                            target_port=k8s.IntOrString.from_number(port_map.container_port),
                        ) for port_map in port_mappings
                    ],
                    selector=label
                ),
            )

        if config is not None:
            k8s.KubeConfigMap(
                self,
                "config",
                data=dict(config=json.dumps(config.get())),
            )

        k8s.KubeDeployment(
            self,
            "deployment",
            spec=k8s.DeploymentSpec(
                replicas=replicas,
                selector=k8s.LabelSelector(match_labels=label),
                template=k8s.PodTemplateSpec(
                    metadata=k8s.ObjectMeta(labels=label),
                    spec=k8s.PodSpec(
                        containers=[
                            k8s.Container(
                                name="sequencer",
                                image=image,
                                args=args or [],
                                ports=[k8s.ContainerPort(container_port=port_map.container_port) for port_map in port_mappings or []],
                                startup_probe=k8s.Probe(
                                    http_get=k8s.HttpGetAction(
                                        path=health_check.startup_probe.path,
                                        port=k8s.IntOrString.from_string(health_check.startup_probe.port) 
                                            if isinstance(health_check.startup_probe.port, str) 
                                            else k8s.IntOrString.from_number(health_check.startup_probe.port)
                                    ),
                                    period_seconds=health_check.startup_probe.period_seconds,
                                    failure_threshold=health_check.startup_probe.failure_threshold,
                                    timeout_seconds=health_check.startup_probe.timeout_seconds
                                ) if health_check.startup_probe is not None else None,

                                readiness_probe=k8s.Probe(
                                    http_get=k8s.HttpGetAction(
                                        path=health_check.readiness_probe.path,
                                        port=k8s.IntOrString.from_string(health_check.readiness_probe.port) 
                                            if isinstance(health_check.readiness_probe.port, str) 
                                            else k8s.IntOrString.from_number(health_check.readiness_probe.port)
                                    ),
                                    period_seconds=health_check.readiness_probe.period_seconds,
                                    failure_threshold=health_check.readiness_probe.failure_threshold,
                                    timeout_seconds=health_check.readiness_probe.timeout_seconds
                                ) if health_check.readiness_probe is not None else None,

                                liveness_probe=k8s.Probe(
                                    http_get=k8s.HttpGetAction(
                                        path=health_check.liveness_probe.path,
                                        port=k8s.IntOrString.from_string(health_check.liveness_probe.port) 
                                            if isinstance(health_check.liveness_probe.port, str) 
                                            else k8s.IntOrString.from_number(health_check.liveness_probe.port)
                                    ),
                                    period_seconds=health_check.liveness_probe.period_seconds,
                                    failure_threshold=health_check.liveness_probe.failure_threshold,
                                    timeout_seconds=health_check.liveness_probe.timeout_seconds
                                ) if health_check.liveness_probe is not None else None,

                                volume_mounts=[
                                    mount for mount in [
                                        k8s.VolumeMount(
                                            name=f"{self.node.id}-config",
                                            mount_path=config.mount_path,
                                            read_only=True
                                        ) if config is not None else None,

                                        k8s.VolumeMount(
                                            name=f"{self.node.id}-data",
                                            mount_path=pvc.mount_path,
                                            read_only=True
                                        ) if pvc is not None else None
                                    ] if mount is not None
                                ]
                            )
                        ],
                        volumes=[
                            vol for vol in [
                                k8s.Volume(
                                    name=f"{self.node.id}-config",
                                    config_map=k8s.ConfigMapVolumeSource(name=f"{self.node.id}-config")
                                ) if config is not None else None,

                                k8s.Volume(
                                    name=f"{self.node.id}-data",
                                    persistent_volume_claim=k8s.PersistentVolumeClaimVolumeSource(claim_name=f"{self.node.id}-data", read_only=pvc.read_only)
                                ) if pvc is not None else None
                            ] if vol is not None
                        ] 
                    ),
                ),
            ),
        )

        google.BackendConfig(
            self,
            "backendconfig",
            metadata=ApiObjectMetadata(
                name=f"{self.node.id}-backendconfig",
                labels=label
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

        if ingress is not None:
            k8s.KubeIngress(
                self,
                "ingress",
                metadata=k8s.ObjectMeta(
                    name=f"{self.node.id}-ingress",
                    labels=label,
                    annotations=ingress.annotations
                ),
                spec=k8s.IngressSpec(
                    ingress_class_name=ingress.class_name,
                    tls=[
                        k8s.IngressTls(
                            hosts=tls.hosts,
                            secret_name=tls.secret_name
                        )
                        for tls in ingress.tls or []
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
                                                    name=path.backend_service_port_name,
                                                    number=path.backend_service_port_number
                                                )
                                            )
                                        )
                                    )
                                    for path in rule.paths or []
                                ]
                            )
                        )
                        for rule in ingress.rules or []
                    ]
                )
            )

        if pvc is not None:
            k8s.KubePersistentVolumeClaim(
                self,
                "pvc",
                metadata=k8s.ObjectMeta(
                    name=f"{self.node.id}-data",
                    labels=label
                ),
                spec=k8s.PersistentVolumeClaimSpec(
                    storage_class_name=pvc.storage_class_name,
                    access_modes=pvc.access_modes,
                    volume_mode=pvc.volume_mode,
                    resources=k8s.ResourceRequirements(
                        requests={"storage": k8s.Quantity.from_string(pvc.storage)}
                    )
                )
            )
