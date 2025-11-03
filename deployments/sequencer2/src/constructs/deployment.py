from imports import k8s

from src.constructs.base import BaseConstruct
from src.constructs.helpers.pod_builder import PodBuilder


class DeploymentConstruct(BaseConstruct):
    def __init__(
        self,
        scope,
        id: str,
        common_config,
        service_config,
        labels,
        monitoring_endpoint_port,
    ):
        super().__init__(
            scope,
            id,
            common_config,
            service_config,
            labels,
            monitoring_endpoint_port,
        )

        self.deployment = self._create_deployment()

    def _create_deployment(self) -> k8s.KubeDeployment:
        pod_builder = PodBuilder(
            self.common_config,
            self.service_config,
            self.labels,
            self.monitoring_endpoint_port,
        )

        return k8s.KubeDeployment(
            self,
            "deployment",
            metadata=k8s.ObjectMeta(
                labels=self.labels,
                annotations=self.service_config.deploymentAnnotations,
            ),
            spec=k8s.DeploymentSpec(
                replicas=self.service_config.replicas,
                selector=k8s.LabelSelector(match_labels=self.labels),
                strategy=k8s.DeploymentStrategy(type=self.service_config.updateStrategy.type),
                template=k8s.PodTemplateSpec(
                    metadata=k8s.ObjectMeta(
                        labels=self.labels,
                        annotations=self.service_config.podAnnotations,
                    ),
                    spec=pod_builder.build_pod_spec(),
                ),
            ),
        )
