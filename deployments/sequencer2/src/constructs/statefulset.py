from imports import k8s
from src.config import constants as const
from src.constructs.base import BaseConstruct


class StatefulSet(BaseConstruct):
    def __init__(
        self,
        scope,
        id: str,
        service_topology,
        labels,
        monitoring_endpoint_port,
        node_config,
    ):
        super().__init__(
            scope,
            id,
            service_topology,
            labels,
            monitoring_endpoint_port,
            node_config,
        )

        self.statefulset = self._get_statefulset()

    def _get_statefulset(self) -> k8s.KubeStatefulSet:
        return k8s.KubeStatefulSet(
            self,
            "statefulset",
            metadata=k8s.ObjectMeta(labels=self.labels),
            spec=k8s.StatefulSetSpec(
                service_name=f"{self.node.id}-service",
                replicas=self.service_topology.replicas,
                selector=k8s.LabelSelector(match_labels=self.labels),
                update_strategy=self._get_statefulset_update_strategy(
                    type=self.service_topology.update_strategy_type
                ),
                template=k8s.PodTemplateSpec(
                    metadata=k8s.ObjectMeta(
                        labels=self.labels,
                        annotations={
                            "prometheus.io/path": const.MONITORING_METRICS_ENDPOINT,
                            "prometheus.io/port": str(self.monitoring_endpoint_port),
                            "prometheus.io/scrape": "true",
                        },
                    ),
                    spec=k8s.PodSpec(
                        security_context=k8s.PodSecurityContext(fs_group=1000),
                        volumes=self._get_volumes(),
                        tolerations=self._get_tolerations(),
                        node_selector=self._get_node_selector(),
                        affinity=self._get_affinity(),
                        containers=[
                            k8s.Container(
                                name=self.node.id,
                                image=self.service_topology.image,
                                image_pull_policy="IfNotPresent",
                                env=self._get_container_env(),
                                args=self._get_container_args(),
                                ports=self._get_container_ports(),
                                startup_probe=self._get_http_probe(
                                    success_threshold=const.STARTUP_PROBE_SUCCESS_THRESHOLD,
                                    failure_threshold=const.STARTUP_PROBE_FAILURE_THRESHOLD,
                                    period_seconds=const.STARTUP_PROBE_PERIOD_SECONDS,
                                    timeout_seconds=const.STARTUP_PROBE_TIMEOUT_SECONDS,
                                    path=const.PROBE_MONITORING_ALIVE_PATH,
                                ),
                                readiness_probe=self._get_http_probe(
                                    success_threshold=const.READINESS_PROBE_SUCCESS_THRESHOLD,
                                    failure_threshold=const.READINESS_PROBE_FAILURE_THRESHOLD,
                                    period_seconds=const.READINESS_PROBE_PERIOD_SECONDS,
                                    timeout_seconds=const.READINESS_PROBE_TIMEOUT_SECONDS,
                                    path=const.PROBE_MONITORING_READY_PATH,
                                ),
                                liveness_probe=self._get_http_probe(
                                    success_threshold=const.LIVENESS_PROBE_SUCCESS_THRESHOLD,
                                    failure_threshold=const.LIVENESS_PROBE_FAILURE_THRESHOLD,
                                    period_seconds=const.LIVENESS_PROBE_PERIOD_SECONDS,
                                    timeout_seconds=const.LIVENESS_PROBE_TIMEOUT_SECONDS,
                                    path=const.PROBE_MONITORING_ALIVE_PATH,
                                ),
                                volume_mounts=self._get_volume_mounts(),
                                resources=self._get_container_resources(),
                            )
                        ],
                    ),
                ),
            ),
        )

    def _get_statefulset_rolling_update(
        self,
        max_unavailable: str,
        partition: int,
    ) -> k8s.RollingUpdateStatefulSetStrategy:
        return k8s.RollingUpdateStatefulSetStrategy(
            max_unavailable=k8s.IntOrString.from_string(max_unavailable),
            partition=partition,
        )

    def _get_statefulset_update_strategy(self, type: str) -> k8s.StatefulSetUpdateStrategy:
        assert type in [
            "OnDelete",
            "RollingUpdate",
            "Recreate",
        ], f"StatefulSet strategy type must be one of 'OnDelete', 'Recreate' or 'RollingUpdate', got {type}."

        max_unavailable = const.DEFAULT_ROLLING_UPDATE_MAX_UNAVAILABLE
        partition = const.DEFAULT_ROLLING_UPDATE_PARTITION

        if type == "Recreate":
            type = "RollingUpdate"
            max_unavailable = const.RECREATE_ROLLING_UPDATE_MAX_UNAVAILABLE
        return k8s.StatefulSetUpdateStrategy(
            type=type,
            rolling_update=(
                (
                    self._get_statefulset_rolling_update(
                        max_unavailable=max_unavailable,
                        partition=partition,
                    )
                )
                if type == "RollingUpdate"
                else None
            ),
        )
