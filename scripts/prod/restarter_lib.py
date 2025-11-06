#!/usr/bin/env python3

import sys
from abc import ABC, abstractmethod
from time import sleep
from typing import Callable, Optional

from common_lib import (
    Colors,
    NamespaceAndInstructionArgs,
    RestartStrategy,
    Service,
    get_namespace_args,
    print_colored,
    print_error,
    run_kubectl_command,
    wait_until_y_or_n,
)
from metrics_lib import MetricConditionGater


def _get_pod_names(
    namespace: str, service: Service, index: int, cluster: Optional[str] = None
) -> list[str]:
    kubectl_args = [
        "get",
        "pods",
        "-o",
        "name",
    ]
    kubectl_args.extend(get_namespace_args(namespace, cluster))
    pods = run_kubectl_command(kubectl_args, capture_output=True).stdout.splitlines()
    return [pod.split("/")[1] for pod in pods if pod.startswith(f"pod/{service.pod_name}")]


class ServiceRestarter(ABC):
    """Abstract class for restarting service instances."""

    def __init__(
        self,
        namespace_and_instruction_args: NamespaceAndInstructionArgs,
        service: Service,
    ):
        self.namespace_and_instruction_args = namespace_and_instruction_args
        self.service = service

    @staticmethod
    def _restart_pod(
        namespace: str, service: Service, index: int, cluster: Optional[str] = None
    ) -> None:
        """Restart pod by deleting it"""
        pods = _get_pod_names(namespace, service, index, cluster)

        if not pods:
            print_error(
                f"Could not find pods for service {service.pod_name} with namespace {namespace} and cluster {cluster}."
            )
            sys.exit(1)

        # Go over each pod and delete it.
        for pod in pods:
            kubectl_args = [
                "delete",
                "pod",
                pod,
            ]
            kubectl_args.extend(get_namespace_args(namespace, cluster))

            try:
                run_kubectl_command(kubectl_args, capture_output=False)
                print_colored(f"Restarted {pod} for node {index}")
            except Exception as e:
                print_error(f"Failed restarting {pod} for node {index}: {e}")
                sys.exit(1)

    @abstractmethod
    def restart_service(self, instance_index: int) -> bool:
        """Restart service for a specific instance. If returns False, the restart process should be aborted."""

    # from_restart_strategy is a static method that returns the appropriate ServiceRestarter based on the restart strategy.
    @staticmethod
    def from_restart_strategy(
        restart_strategy: RestartStrategy,
        namespace_and_instruction_args: NamespaceAndInstructionArgs,
        service: Service,
    ) -> "ServiceRestarter":
        if restart_strategy == RestartStrategy.ONE_BY_ONE:
            check_between_restarts = lambda instance_index: (
                True
                if instance_index == namespace_and_instruction_args.size() - 1
                else wait_until_y_or_n(f"Do you want to restart the next pod?")
            )

            return ChecksBetweenRestartsCompositeRestarter(
                namespace_and_instruction_args,
                service,
                check_between_restarts,
                RestartPodOnlyRestarter(namespace_and_instruction_args, service),
            )
        elif restart_strategy == RestartStrategy.ALL_AT_ONCE:
            return ChecksBetweenRestartsCompositeRestarter(
                namespace_and_instruction_args,
                service,
                lambda instance_index: True,
                RestartPodOnlyRestarter(namespace_and_instruction_args, service),
            )
        elif restart_strategy == RestartStrategy.NO_RESTART:
            assert (
                namespace_and_instruction_args.get_instruction(0) is None
            ), f"post_restart_instructions is not allowed with no_restart as the restart strategy"
            return NoOpServiceRestarter(namespace_and_instruction_args, service)
        else:
            raise ValueError(f"Invalid restart strategy: {restart_strategy}")


class RestartPodOnlyRestarter(ServiceRestarter):
    """Restarter that only restarts the pod and does not check anything else."""

    def __init__(
        self, namespace_and_instruction_args: NamespaceAndInstructionArgs, service: Service
    ):
        super().__init__(namespace_and_instruction_args, service)

    def restart_service(self, instance_index: int) -> bool:
        """Restarts the pod and does nothing else."""
        self._restart_pod(
            self.namespace_and_instruction_args.get_namespace(instance_index),
            self.service,
            instance_index,
            self.namespace_and_instruction_args.get_cluster(instance_index),
        )
        print_colored(f"Restarted pod {instance_index}. ", Colors.YELLOW)
        return True


class ChecksBetweenRestartsCompositeRestarter(ServiceRestarter):
    """Checks between restarts."""

    def __init__(
        self,
        namespace_and_instruction_args: NamespaceAndInstructionArgs,
        service: Service,
        check_between_restarts: Callable[[int], bool],
        base_service_restarter: ServiceRestarter,
    ):
        super().__init__(namespace_and_instruction_args, service)
        self.check_between_restarts = check_between_restarts
        self.base_service_restarter = base_service_restarter

    def restart_service(self, instance_index: int) -> bool:
        """Call the base restarter on each instance one by one, running the check_between_restarts in between each."""
        self.base_service_restarter.restart_service(instance_index)

        instructions = self.namespace_and_instruction_args.get_instruction(instance_index)
        if instructions is not None:
            print_colored(f"{instructions} ", Colors.YELLOW)
        return self.check_between_restarts(instance_index)


class NoOpServiceRestarter(ServiceRestarter):
    """No-op service restarter."""

    def restart_service(self, instance_index: int) -> bool:
        """No-op."""
        print_colored("\nSkipping pod restart.")
        return True


class WaitOnMetricRestarter(ChecksBetweenRestartsCompositeRestarter):
    def __init__(
        self,
        namespace_and_instruction_args: NamespaceAndInstructionArgs,
        service: Service,
        metrics: list["MetricConditionGater.Metric"],
        metrics_port: int,
        restart_strategy: RestartStrategy,
    ):
        self.metrics = metrics
        self.metrics_port = metrics_port
        if restart_strategy == RestartStrategy.ONE_BY_ONE:
            check_function = self._check_between_each_restart
            base_restarter = RestartPodOnlyRestarter(namespace_and_instruction_args, service)
        elif restart_strategy == RestartStrategy.ALL_AT_ONCE:
            check_function = self._check_all_only_after_last_restart
            base_restarter = RestartPodOnlyRestarter(namespace_and_instruction_args, service)
        elif restart_strategy == RestartStrategy.NO_RESTART:
            check_function = self._check_between_each_restart
            base_restarter = NoOpServiceRestarter(namespace_and_instruction_args, service)
        else:
            print_error(f"Invalid restart strategy: {restart_strategy} for WaitOnMetricRestarter.")
            sys.exit(1)

        super().__init__(namespace_and_instruction_args, service, check_function, base_restarter)

    def _check_between_each_restart(self, instance_index: int) -> bool:
        self._wait_for_pod_to_satisfy_condition(instance_index)
        if instance_index == self.namespace_and_instruction_args.size() - 1:
            # Last instance, no need to prompt the user about the next restart.
            return True
        return wait_until_y_or_n(f"Do you want to restart the next pod?")

    def _check_all_only_after_last_restart(self, instance_index: int) -> bool:
        # Restart all nodes without waiting for confirmation.
        if instance_index < self.namespace_and_instruction_args.size() - 1:
            return True

        # After the last node has been restarted, wait for all pods to satisfy the condition.
        for instance_index in range(self.namespace_and_instruction_args.size()):
            self._wait_for_pod_to_satisfy_condition(instance_index)
        return True

    def _wait_for_pod_to_satisfy_condition(self, instance_index: int) -> bool:
        # The sleep is to prevent the case where we get the pod name of the old pod we just deleted
        # instead of the new one.
        # TODO(guy.f): Verify this is not the name of the old pod some other way.
        sleep(2)
        pod_names = WaitOnMetricRestarter._wait_for_pods_to_be_ready(
            self.namespace_and_instruction_args.get_namespace(instance_index),
            self.namespace_and_instruction_args.get_cluster(instance_index),
            self.service,
        )
        if pod_names is None:
            return False

        for pod_name in pod_names:
            for metric in self.metrics:
                metric_condition_gater = MetricConditionGater(
                    metric,
                    self.namespace_and_instruction_args.get_namespace(instance_index),
                    self.namespace_and_instruction_args.get_cluster(instance_index),
                    pod_name,
                    self.metrics_port,
                )
                metric_condition_gater.gate()

    @staticmethod
    def _wait_for_pods_to_be_ready(
        namespace: str,
        cluster: Optional[str],
        service: Service,
        wait_timeout: int = 180,
        num_retry: int = 3,
        refresh_delay_sec: int = 3,
    ) -> Optional[list[str]]:
        """
        Wait for pods to be in ready mode as reported by Kubernetes.
        """

        for i in range(num_retry):
            pods = _get_pod_names(namespace, service, 0, cluster)
            if pods:
                for pod in pods:
                    print_colored(
                        f"Waiting for pod {pod} to be ready... (timeout set to {wait_timeout}s)"
                    )
                    kubectl_args = [
                        "wait",
                        "--for=condition=ready",
                        f"pod/{pod}",
                        "--timeout",
                        f"{wait_timeout}s",
                    ]
                    kubectl_args.extend(get_namespace_args(namespace, cluster))
                    result = run_kubectl_command(kubectl_args, capture_output=False)

                    if result.returncode != 0:
                        print_colored(
                            f"Timed out waiting for pod {pod} to be ready: {result.stderr}, retrying... (attempt {i + 1}/{num_retry})",
                            Colors.YELLOW,
                        )
                        break
                return pods
            else:
                print_colored(
                    f"Could not get pod names for service {service.pod_name}, retrying... (attempt {i + 1}/{num_retry})",
                    Colors.YELLOW,
                )
            sleep(refresh_delay_sec)

        print_error(f"Pods for service {service.pod_name} are not ready after {num_retry} attempts")
        return None
