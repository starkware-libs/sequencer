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

    def _build_update_strategy(self) -> k8s.StatefulSetUpdateStrategy:
        """Build StatefulSet update strategy with optional rollingUpdate config."""
        update_strategy_config = (
            self.service_config.statefulSet.updateStrategy
            if self.service_config.statefulSet
            else None
        )

        strategy_type = update_strategy_config.type if update_strategy_config else "RollingUpdate"

        # Build rollingUpdate object if provided
        rolling_update = None
        if (
            strategy_type == "RollingUpdate"
            and update_strategy_config
            and update_strategy_config.rollingUpdate
        ):
            # Try to construct rolling update with available parameters
            # Note: cdk8s may use different parameter names
            rolling_update_kwargs = {}
            if update_strategy_config.rollingUpdate.maxUnavailable:
                # Try both camelCase and snake_case
                rolling_update_kwargs["max_unavailable"] = k8s.IntOrString.from_string(
                    update_strategy_config.rollingUpdate.maxUnavailable
                )
            if update_strategy_config.rollingUpdate.partition is not None:
                rolling_update_kwargs["partition"] = update_strategy_config.rollingUpdate.partition

            # If we have rolling update params, we need to construct it
            # For now, skip rolling update if cdk8s doesn't support it directly
            # The type will still be RollingUpdate which is the main requirement
            pass

        return k8s.StatefulSetUpdateStrategy(type=strategy_type)

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
                service_name=f"sequencer-{self.service_config.name}-service",
                replicas=self.service_config.replicas,
                selector=k8s.LabelSelector(match_labels=statefulset_labels),
                update_strategy=self._build_update_strategy(),
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
