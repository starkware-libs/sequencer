#!/usr/bin/env python3

import sys
from abc import ABC, abstractmethod
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
            print_error(f"Could not find pods for service {service.pod_name}.")
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

            return ChecksBetweenRestarts(
                namespace_and_instruction_args,
                service,
                check_between_restarts,
            )
        elif restart_strategy == RestartStrategy.ALL_AT_ONCE:
            return ChecksBetweenRestarts(
                namespace_and_instruction_args,
                service,
                lambda instance_index: True,
            )
        elif restart_strategy == RestartStrategy.NO_RESTART:
            assert (
                namespace_and_instruction_args.get_instruction(0) is None
            ), f"post_restart_instructions is not allowed with no_restart as the restart strategy"
            return NoOpServiceRestarter(namespace_and_instruction_args, service)
        else:
            raise ValueError(f"Invalid restart strategy: {restart_strategy}")


class ChecksBetweenRestarts(ServiceRestarter):
    """Checks between restarts."""

    def __init__(
        self,
        namespace_and_instruction_args: NamespaceAndInstructionArgs,
        service: Service,
        check_between_restarts: Callable[[int], bool],
    ):
        super().__init__(namespace_and_instruction_args, service)
        self.check_between_restarts = check_between_restarts

    def restart_service(self, instance_index: int) -> bool:
        """Restart the instance one by one, running the use code in between each restart."""
        # self._restart_pod(
        #     self.namespace_and_instruction_args.get_namespace(instance_index),
        #     self.service,
        #     instance_index,
        #     self.namespace_and_instruction_args.get_cluster(instance_index),
        # )
        instructions = self.namespace_and_instruction_args.get_instruction(instance_index)
        print_colored(
            f"Restarted pod {instance_index}.\n{instructions if instructions is not None else ''} ",
            Colors.YELLOW,
        )
        return self.check_between_restarts(instance_index)


class NoOpServiceRestarter(ServiceRestarter):
    """No-op service restarter."""

    def restart_service(self, instance_index: int) -> bool:
        """No-op."""
        print_colored("\nSkipping pod restart.")
        return True


