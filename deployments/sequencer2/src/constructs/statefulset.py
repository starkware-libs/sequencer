from imports import k8s
from src.constructs.base import BaseConstruct


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

        self.statefulset = self._get_statefulset()

    def _get_statefulset(self) -> k8s.KubeStatefulSet:
        image = f"{self.common_config.image.repository}:{self.common_config.image.tag}"
        return k8s.KubeStatefulSet(
            self,
            "statefulset",
            metadata=k8s.ObjectMeta(labels=self.labels, annotations=self.service_config.statefulSet.annotations),
            spec=k8s.StatefulSetSpec(
                service_name=self.service_config.name,
                replicas=self.service_config.replicas,
                selector=k8s.LabelSelector(match_labels=self.labels),
                update_strategy=self._get_statefulset_update_strategy(),
                pod_management_policy=self.service_config.statefulSet.podManagementPolicy,
                template=k8s.PodTemplateSpec(
                    metadata=k8s.ObjectMeta(labels=self.labels, annotations=self.service_config.podAnnotations),
                    spec=k8s.PodSpec(
                        service_account_name=self.service_config.serviceAccount.name if self.service_config.serviceAccount else None,
                        termination_grace_period_seconds=self.service_config.terminationGracePeriodSeconds,
                        priority_class_name=self.service_config.priorityClassName,
                        security_context=k8s.PodSecurityContext(
                            fs_group=self.service_config.securityContext.fsGroup,
                            run_as_group=self.service_config.securityContext.runAsGroup,
                            run_as_user=self.service_config.securityContext.runAsUser,
                            run_as_non_root=self.service_config.securityContext.runAsNonRoot,
                        ),
                        image_pull_secrets=[{"name": s} for s in self.common_config.imagePullSecrets],
                        volumes=self._get_volumes(),
                        tolerations=self._get_tolerations(),
                        node_selector=self._get_node_selector(),
                        affinity=self._get_affinity(),
                        containers=[
                            k8s.Container(
                                name=self.service_config.name,
                                image=image,
                                image_pull_policy=self.common_config.image.imagePullPolicy,
                                command=self.service_config.command,
                                env=self._get_container_env(),
                                ports=self._get_container_ports(),
                                startup_probe=self._get_http_probe(self.service_config.startupProbe),
                                readiness_probe=self._get_http_probe(self.service_config.readinessProbe),
                                liveness_probe=self._get_http_probe(self.service_config.livenessProbe),
                                volume_mounts=self._get_volume_mounts(),
                                resources=self._get_container_resources(),
                            )
                        ],
                    ),
                ),
            ),
        )

    def _get_statefulset_update_strategy(self) -> k8s.StatefulSetUpdateStrategy:
        strategy_type = self.service_config.updateStrategy.type if self.service_config.updateStrategy else "RollingUpdate"
        return k8s.StatefulSetUpdateStrategy(type=strategy_type)

