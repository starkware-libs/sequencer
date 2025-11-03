from imports import k8s

from src.constructs.base import BaseConstruct
from src.constructs.helpers.pod_builder import PodBuilder


class StatefulSetConstruct(BaseConstruct):
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

        self.statefulset = self._create_statefulset()

    def _create_statefulset(self) -> k8s.KubeStatefulSet:
        # Merge StatefulSet labels with common labels
        statefulset_labels = (
            {**self.labels, **self.service_config.statefulSet.labels}
            if self.service_config.statefulSet and self.service_config.statefulSet.labels
            else self.labels
        )

        pod_builder = PodBuilder(
            self.common_config,
            self.service_config,
            statefulset_labels,
            self.monitoring_endpoint_port,
        )

        return k8s.KubeStatefulSet(
            self,
            "statefulset",
            metadata=k8s.ObjectMeta(
                labels=statefulset_labels,
                annotations=(
                    self.service_config.statefulSet.annotations
                    if self.service_config.statefulSet
                    else {}
                ),
            ),
            spec=k8s.StatefulSetSpec(
                service_name=self.service_config.name,
                replicas=self.service_config.replicas,
                selector=k8s.LabelSelector(match_labels=statefulset_labels),
                update_strategy=k8s.StatefulSetUpdateStrategy(
                    type=(
                        self.service_config.statefulSet.updateStrategy.type
                        if self.service_config.statefulSet.updateStrategy
                        else "RollingUpdate"
                    )
                ),
                pod_management_policy=self.service_config.statefulSet.podManagementPolicy,
                template=k8s.PodTemplateSpec(
                    metadata=k8s.ObjectMeta(
                        labels=statefulset_labels,
                        annotations=self.service_config.podAnnotations,
                    ),
                    spec=pod_builder.build_pod_spec(),
                ),
            ),
        )
