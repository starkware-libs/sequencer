#!/usr/bin/env python3

import signal
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
    run_in_parallel,
    run_kubectl_command,
    wait_until_y_or_n,
)
from metrics_lib import MetricConditionGater, terminate_all_port_forwards


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
                # Capture (rather than stream) so output stays grouped per node when restarts run
                # in parallel; echo it through print_colored which honors the per-node buffer.
                result = run_kubectl_command(kubectl_args, capture_output=True)
                if result.stdout:
                    print_colored(result.stdout.rstrip())
                print_colored(f"Restarted {pod} for node {index}")
            except Exception as e:
                print_error(f"Failed restarting {pod} for node {index}: {e}")
                sys.exit(1)

    @abstractmethod
    def restart_service(self, instance_index: int) -> bool:
        """Restart service for a specific instance. If returns False, the restart process should be aborted."""

    def restart_all(self, max_parallelism: int) -> None:
        """Restart all instances.

        Default: sequential, one instance at a time, aborting if any `restart_service` returns
        False. Subclasses that have no inter-node ordering dependency override this to run in
        parallel. `max_parallelism` is ignored by this sequential default.
        """
        for instance_index in range(self.namespace_and_instruction_args.size()):
            if not self.restart_service(instance_index):
                print_colored("\nAborting restart process.")
                sys.exit(1)

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
                parallel=False,
            )
        elif restart_strategy == RestartStrategy.ALL_AT_ONCE:
            return ChecksBetweenRestartsCompositeRestarter(
                namespace_and_instruction_args,
                service,
                lambda instance_index: True,
                RestartPodOnlyRestarter(namespace_and_instruction_args, service),
                parallel=True,
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
        parallel: bool = False,
    ):
        super().__init__(namespace_and_instruction_args, service)
        self.check_between_restarts = check_between_restarts
        self.base_service_restarter = base_service_restarter
        # When True there is no inter-node ordering dependency (e.g. ALL_AT_ONCE), so restart_all
        # restarts every node and then runs the post-restart checks concurrently. When False
        # (interactive ONE_BY_ONE / NO_RESTART) restart_all stays sequential.
        self.parallel = parallel

    def _label(self, instance_index: int) -> str:
        return self.namespace_and_instruction_args.get_namespace(instance_index)

    def restart_service(self, instance_index: int) -> bool:
        """Call the base restarter on each instance one by one, running the check_between_restarts in between each."""
        self.base_service_restarter.restart_service(instance_index)

        instructions = self.namespace_and_instruction_args.get_instruction(instance_index)
        if instructions is not None:
            print_colored(f"{instructions} ", Colors.YELLOW)
        return self.check_between_restarts(instance_index)

    def restart_all(self, max_parallelism: int) -> None:
        if not self.parallel:
            super().restart_all(max_parallelism)
            return

        indices = list(range(self.namespace_and_instruction_args.size()))

        # Restarting every Core pod simultaneously brings all consensus validators down at once and
        # can halt the chain. Require an explicit extra confirmation before doing so.
        if self.service == Service.Core and not wait_until_y_or_n(
            f"WARNING: this will restart ALL {len(indices)} Core pods at the same time, which can "
            "halt consensus. Are you sure you want to continue?"
        ):
            print_colored("\nAborting restart process.")
            sys.exit(1)

        # Phase 1: restart every node's pod concurrently (pod deletes have no ordering dependency).
        print_colored(f"\nRestarting {len(indices)} node(s) in parallel...", Colors.YELLOW)
        run_in_parallel(
            indices,
            self.base_service_restarter.restart_service,
            max_parallelism,
            self._label,
        )

        for instance_index in indices:
            instructions = self.namespace_and_instruction_args.get_instruction(instance_index)
            if instructions is not None:
                print_colored(f"[{self._label(instance_index)}] {instructions}", Colors.YELLOW)

        # Phase 2: run post-restart checks (if any) concurrently.
        self._wait_all(indices, max_parallelism)

    def _wait_all(self, indices: list[int], max_parallelism: int) -> None:
        """Run post-restart checks for all nodes concurrently. No-op when there is nothing to wait
        for (overridden by restarters that gate on metrics)."""


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
        # ALL_AT_ONCE has no inter-node ordering dependency: restart every node, then wait for all
        # conditions concurrently. ONE_BY_ONE / NO_RESTART stay sequential (they prompt the user
        # between nodes).
        parallel = restart_strategy == RestartStrategy.ALL_AT_ONCE
        if restart_strategy == RestartStrategy.ONE_BY_ONE:
            check_function = self._check_between_each_restart
            base_restarter = RestartPodOnlyRestarter(namespace_and_instruction_args, service)
        elif restart_strategy == RestartStrategy.ALL_AT_ONCE:
            # check_function is unused in the parallel path (restart_all drives the phases directly).
            check_function = lambda instance_index: True
            base_restarter = RestartPodOnlyRestarter(namespace_and_instruction_args, service)
        elif restart_strategy == RestartStrategy.NO_RESTART:
            check_function = self._check_between_each_restart
            base_restarter = NoOpServiceRestarter(namespace_and_instruction_args, service)
        else:
            print_error(f"Invalid restart strategy: {restart_strategy} for WaitOnMetricRestarter.")
            sys.exit(1)

        super().__init__(
            namespace_and_instruction_args, service, check_function, base_restarter, parallel
        )

    def _check_between_each_restart(self, instance_index: int) -> bool:
        if not self._wait_for_pod_to_satisfy_condition(instance_index):
            print_error(f"Failed waiting for condition(s) for Pod {instance_index}.")
        if instance_index == self.namespace_and_instruction_args.size() - 1:
            # Last instance, no need to prompt the user about the next restart.
            return True
        return wait_until_y_or_n(f"Do you want to restart the next pod?")

    def _wait_all(self, indices: list[int], max_parallelism: int) -> None:
        # gate() starts a kubectl port-forward per node on a worker thread, which cannot install
        # signal handlers; install one here (main thread) so Ctrl-C tears all of them down.
        def signal_handler(signum, frame):
            terminate_all_port_forwards()
            sys.exit(0)

        signal.signal(signal.SIGINT, signal_handler)
        signal.signal(signal.SIGTERM, signal_handler)

        run_in_parallel(indices, self._wait_for_index, max_parallelism, self._label)

    def _wait_for_index(self, instance_index: int) -> None:
        if not self._wait_for_pod_to_satisfy_condition(instance_index):
            print_error(f"Failed waiting for condition(s) for Pod {instance_index}.")

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
        return True

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
                    # Capture (rather than stream) so output stays grouped per node under parallel
                    # waits; progress is surfaced by run_in_parallel's heartbeat instead.
                    result = run_kubectl_command(kubectl_args, capture_output=True)
                    if result.stdout:
                        print_colored(result.stdout.rstrip())

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
