from typing import Optional, Dict
from constructs import Construct
from cdk8s import Names
import json
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
        container_port: int = 8080,
        config: Optional[Dict[str, str]] = None,
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
                            )
                        ],
                        volumes=(
                            [
                                k8s.Volume(
                                    name=service_config.name,
                                    config_map=k8s.ConfigMapVolumeSource(
                                        name=service_config.name
                                    ),
                                )
                            ]
                            if config is not None
                            else None
                        ),
                    ),
                ),
            ),
        )
