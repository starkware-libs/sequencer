
from constructs import Construct
from imports import k8s
from src.config import constants as const


class HpaConstruct(Construct):
    def __init__(self, scope: Construct, id: str, labels, service_topology, controller):
        super().__init__(scope, id)

        self.hpa = k8s.KubeHorizontalPodAutoscalerV2(
            self,
            "hpa",
            metadata=k8s.ObjectMeta(labels=labels),
            spec=k8s.HorizontalPodAutoscalerSpecV2(
                min_replicas=service_topology.replicas,
                max_replicas=const.HPA_MAX_REPLICAS,
                scale_target_ref=k8s.CrossVersionObjectReferenceV2(
                    api_version=controller.api_version,
                    kind=controller.kind,
                    name=controller.metadata.name,
                ),
                metrics=[
                    k8s.MetricSpecV2(
                        type="Resource",
                        resource=k8s.ResourceMetricSourceV2(
                            name="cpu",
                            target=k8s.MetricTargetV2(
                                type="Utilization", average_utilization=50
                            ),
                        ),
                    )
                ],
                behavior=k8s.HorizontalPodAutoscalerBehaviorV2(
                    scale_up=k8s.HpaScalingRulesV2(
                        select_policy="Max",
                        stabilization_window_seconds=300,
                        policies=[
                            k8s.HpaScalingPolicyV2(
                                type="Pods",
                                value=2,
                                period_seconds=60,
                            )
                        ],
                    ),
                    scale_down=k8s.HpaScalingRulesV2(
                        select_policy="Max",
                        stabilization_window_seconds=300,
                        policies=[
                            k8s.HpaScalingPolicyV2(
                                type="Pods",
                                value=2,
                                period_seconds=60,
                            )
                        ],
                    ),
                ),
            ),
        )
