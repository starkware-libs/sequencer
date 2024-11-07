import json

from typing import Optional, Dict
from constructs import Construct
from cdk8s import Names
from imports import k8s

from services.objects import HealthCheck, ServiceType, PersistentVolumeClaim


class Service(Construct):
    def __init__(
        self,
        scope: Construct,
        id: str,
        *,
        image: str,
        replicas: int = 1,
        service_type: Optional[ServiceType] = None,
        port_mappings: Optional[list[Dict[str, int]]] = None,
        config: Optional[Dict[str, str]] = None,
        health_check: Optional[HealthCheck] = None,
        pvc: Optional[PersistentVolumeClaim] = None
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
                            port=port_map["port"],
                            target_port=k8s.IntOrString.from_number(port_map.get("container_port")),
                        ) for port_map in port_mappings
                    ],
                    selector=label
                ),
            )

        if config is not None:
            k8s.KubeConfigMap(
                self,
                "config",
                data=dict(config=json.dumps(config)),
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
                                name="web",
                                image=image,
                                ports=[k8s.ContainerPort(container_port=port_map.get("container_port")) for port_map in port_mappings or []],
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
                                            mount_path="/",
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
