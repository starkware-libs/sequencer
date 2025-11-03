from imports import k8s

from src.constructs.base import BaseConstruct


class HpaConstruct(BaseConstruct):
    def __init__(
        self,
        scope,
        id: str,
        common_config,
        service_config,
        labels,
        monitoring_endpoint_port,
        controller,
    ):
        super().__init__(
            scope,
            id,
            common_config,
            service_config,
            labels,
            monitoring_endpoint_port,
        )

        self.hpa = self._create_hpa(controller)

    def _create_hpa(self, controller) -> k8s.KubeHorizontalPodAutoscalerV2:
        return k8s.KubeHorizontalPodAutoscalerV2(
            self,
            "hpa",
            metadata=k8s.ObjectMeta(labels=self.labels),
            spec=k8s.HorizontalPodAutoscalerSpecV2(
                min_replicas=self.service_config.hpa.minReplicas,
                max_replicas=self.service_config.hpa.maxReplicas,
                scale_target_ref=k8s.CrossVersionObjectReferenceV2(
                    api_version=controller.api_version,
                    kind=controller.kind,
                    name=controller.metadata.name,
                ),
                metrics=self._build_metrics(),
                behavior=self._build_behavior(),
            ),
        )

    def _build_metrics(self) -> list[k8s.MetricSpecV2]:
        """Build HPA metrics from configuration."""
        metrics = []

        # Add standard CPU/Memory metrics if configured
        if self.service_config.hpa.targetCPUUtilizationPercentage:
            metrics.append(
                k8s.MetricSpecV2(
                    type="Resource",
                    resource=k8s.ResourceMetricSourceV2(
                        name="cpu",
                        target=k8s.MetricTargetV2(
                            type="Utilization",
                            average_utilization=self.service_config.hpa.targetCPUUtilizationPercentage,
                        ),
                    ),
                )
            )

        if self.service_config.hpa.targetMemoryUtilizationPercentage:
            metrics.append(
                k8s.MetricSpecV2(
                    type="Resource",
                    resource=k8s.ResourceMetricSourceV2(
                        name="memory",
                        target=k8s.MetricTargetV2(
                            type="Utilization",
                            average_utilization=self.service_config.hpa.targetMemoryUtilizationPercentage,
                        ),
                    ),
                )
            )

        # Add custom metrics if provided
        for metric_config in self.service_config.hpa.metrics:
            metrics.append(k8s.MetricSpecV2.from_json(metric_config))

        return metrics

    def _build_behavior(self) -> k8s.HorizontalPodAutoscalerBehaviorV2 | None:
        """Build HPA scaling behavior from configuration."""
        if not any(
            [
                self.service_config.hpa.scaleUpStabilizationWindowSeconds,
                self.service_config.hpa.scaleDownStabilizationWindowSeconds,
                self.service_config.hpa.scaleUpPolicies,
                self.service_config.hpa.scaleDownPolicies,
                self.service_config.hpa.behavior,
            ]
        ):
            return None

        # Use custom behavior if provided
        if self.service_config.hpa.behavior:
            return k8s.HorizontalPodAutoscalerBehaviorV2.from_json(self.service_config.hpa.behavior)

        # Build behavior from individual settings
        behavior = {}

        if (
            self.service_config.hpa.scaleUpStabilizationWindowSeconds
            or self.service_config.hpa.scaleUpPolicies
        ):
            behavior["scaleUp"] = k8s.HpaScalingRulesV2(
                stabilization_window_seconds=self.service_config.hpa.scaleUpStabilizationWindowSeconds,
                policies=(
                    [
                        k8s.HpaScalingPolicyV2.from_json(policy)
                        for policy in self.service_config.hpa.scaleUpPolicies
                    ]
                    if self.service_config.hpa.scaleUpPolicies
                    else None
                ),
            )

        if (
            self.service_config.hpa.scaleDownStabilizationWindowSeconds
            or self.service_config.hpa.scaleDownPolicies
        ):
            behavior["scaleDown"] = k8s.HpaScalingRulesV2(
                stabilization_window_seconds=self.service_config.hpa.scaleDownStabilizationWindowSeconds,
                policies=(
                    [
                        k8s.HpaScalingPolicyV2.from_json(policy)
                        for policy in self.service_config.hpa.scaleDownPolicies
                    ]
                    if self.service_config.hpa.scaleDownPolicies
                    else None
                ),
            )

        return k8s.HorizontalPodAutoscalerBehaviorV2.from_json(behavior) if behavior else None
