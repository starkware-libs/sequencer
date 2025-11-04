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
            metric_type = metric_config.get("type")
            if metric_type == "Resource":
                resource_config = metric_config.get("resource", {})
                target_config = resource_config.get("target", {})

                # Build MetricTarget
                target_type = target_config.get("type")
                if target_type == "AverageValue":
                    average_value_str = target_config.get("averageValue")
                    # Convert string to Quantity
                    average_value = (
                        k8s.Quantity.from_string(average_value_str)
                        if isinstance(average_value_str, str)
                        else average_value_str
                    )
                    target = k8s.MetricTargetV2(
                        type="AverageValue",
                        average_value=average_value,
                    )
                elif target_type == "Utilization":
                    target = k8s.MetricTargetV2(
                        type="Utilization",
                        average_utilization=target_config.get("averageUtilization"),
                    )
                elif target_type == "Value":
                    target = k8s.MetricTargetV2(
                        type="Value",
                        value=target_config.get("value"),
                    )
                else:
                    raise ValueError(f"Unsupported target type: {target_type}")

                # Build ResourceMetricSource
                resource = k8s.ResourceMetricSourceV2(
                    name=resource_config.get("name"),
                    target=target,
                )

                # Build MetricSpec
                metrics.append(
                    k8s.MetricSpecV2(
                        type="Resource",
                        resource=resource,
                    )
                )
            else:
                # For other metric types, use from_json if available, otherwise construct manually
                raise ValueError(f"Unsupported metric type: {metric_type}")

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
            behavior_dict = self.service_config.hpa.behavior
            scale_up = None
            scale_down = None

            if "scaleUp" in behavior_dict:
                scale_up_config = behavior_dict["scaleUp"]
                policies = None
                if "policies" in scale_up_config:
                    policies = [
                        k8s.HpaScalingPolicyV2(
                            period_seconds=policy.get("periodSeconds"),
                            type=policy.get("type"),
                            value=policy.get("value"),
                        )
                        for policy in scale_up_config["policies"]
                    ]
                scale_up = k8s.HpaScalingRulesV2(
                    stabilization_window_seconds=scale_up_config.get("stabilizationWindowSeconds"),
                    select_policy=scale_up_config.get("selectPolicy"),
                    policies=policies,
                )

            if "scaleDown" in behavior_dict:
                scale_down_config = behavior_dict["scaleDown"]
                policies = None
                if "policies" in scale_down_config:
                    policies = [
                        k8s.HpaScalingPolicyV2(
                            period_seconds=policy.get("periodSeconds"),
                            type=policy.get("type"),
                            value=policy.get("value"),
                        )
                        for policy in scale_down_config["policies"]
                    ]
                scale_down = k8s.HpaScalingRulesV2(
                    stabilization_window_seconds=scale_down_config.get(
                        "stabilizationWindowSeconds"
                    ),
                    select_policy=scale_down_config.get("selectPolicy"),
                    policies=policies,
                )

            return k8s.HorizontalPodAutoscalerBehaviorV2(
                scale_up=scale_up,
                scale_down=scale_down,
            )

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

        if not behavior:
            return None

        scale_up = None
        scale_down = None

        if "scaleUp" in behavior:
            scale_up_config = behavior["scaleUp"]
            policies = None
            if scale_up_config.get("policies"):
                policies = [
                    k8s.HpaScalingPolicyV2(
                        period_seconds=policy.get("periodSeconds"),
                        type=policy.get("type"),
                        value=policy.get("value"),
                    )
                    for policy in scale_up_config["policies"]
                ]
            scale_up = k8s.HpaScalingRulesV2(
                stabilization_window_seconds=scale_up_config.get("stabilizationWindowSeconds"),
                select_policy=scale_up_config.get("selectPolicy"),
                policies=policies,
            )

        if "scaleDown" in behavior:
            scale_down_config = behavior["scaleDown"]
            policies = None
            if scale_down_config.get("policies"):
                policies = [
                    k8s.HpaScalingPolicyV2(
                        period_seconds=policy.get("periodSeconds"),
                        type=policy.get("type"),
                        value=policy.get("value"),
                    )
                    for policy in scale_down_config["policies"]
                ]
            scale_down = k8s.HpaScalingRulesV2(
                stabilization_window_seconds=scale_down_config.get("stabilizationWindowSeconds"),
                select_policy=scale_down_config.get("selectPolicy"),
                policies=policies,
            )

        return (
            k8s.HorizontalPodAutoscalerBehaviorV2(
                scale_up=scale_up,
                scale_down=scale_down,
            )
            if (scale_up or scale_down)
            else None
        )
