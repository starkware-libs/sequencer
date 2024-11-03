import json

from typing import Optional, Dict, Union
from constructs import Construct
from cdk8s import Names

from imports import k8s

class Service(Construct):
    def __init__(
        self,
        scope: Construct,
        id: str,
        *,
        image: str,
        replicas: int = 1,
        port: Optional[int] = 80,
        container_port: int = 8082,
        config: Optional[Dict[str, str]] = None,
        startup_probe_path: Optional[str] = "/",
        readiness_probe_path: Optional[str] = "/",
        liveness_probe_path: Optional[str] = "/"
        
    ):
        super().__init__(scope, id)

        label = {"app": Names.to_label_value(self, include_hash=False)}
        if port is not None:
            k8s.KubeService(
                self,
                "service",
                spec=k8s.ServiceSpec(
                    type="LoadBalancer",
                    ports=[
                        k8s.ServicePort(
                            port=port,
                            target_port=k8s.IntOrString.from_number(container_port),
                        )
                    ],
                    selector=label,
                ),
            )

        if config is not None:
            service_config = k8s.KubeConfigMap(
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
                                ports=[
                                    k8s.ContainerPort(container_port=container_port)
                                ],
                                startup_probe=k8s.Probe(
                                    http_get=k8s.HttpGetAction(port=k8s.IntOrString.from_string('http'), path=startup_probe_path),
                                    period_seconds=10, failure_threshold=12, timeout_seconds=5
                                ),
                                readiness_probe=k8s.Probe(
                                    http_get=k8s.HttpGetAction(port=k8s.IntOrString.from_string('http'), path=readiness_probe_path),
                                    period_seconds=10, failure_threshold=3, timeout_seconds=5
                                ),
                                liveness_probe=k8s.Probe(
                                    http_get=k8s.HttpGetAction(port=k8s.IntOrString.from_string('http'), path=liveness_probe_path),
                                    period_seconds=5, failure_threshold=5, timeout_seconds=5
                                )
                            )
                        ],
                        volumes=(
                            [
                                k8s.Volume(
                                    name=service_config.name,
                                    config_map=k8s.ConfigMapVolumeSource(name=service_config.name),
                                )
                            ]
                            if config is not None
                            else None
                        ),
                    ),
                ),
            ),
        )
