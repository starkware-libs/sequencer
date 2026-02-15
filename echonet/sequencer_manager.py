"""
Kubernetes orchestration helpers for the Echonet sequencer node.
"""

from __future__ import annotations

import json
import re
import time
from dataclasses import dataclass
from typing import Callable, Optional, Sequence

import kubernetes  # pyright: ignore[reportMissingImports]
from kubernetes.client.rest import ApiException  # pyright: ignore[reportMissingImports]

from echonet.echonet_types import CONFIG, JsonObject
from echonet.logger import get_logger

logger = get_logger("sequencer_manager")

REVERT_INACTIVITY_SUBSTRINGS: tuple[str, ...] = (
    "Reverting Batcher's storage to height marker",
    "Successfully reverted Batcher's storage to height marker",
    "Reverting State Sync's storage to height marker",
    "Successfully reverted State Sync's storage to height marker",
)


ConfigMutator = Callable[[JsonObject], None]


@dataclass(frozen=True, slots=True)
class SequencerKubeSpec:
    """Names/paths that identify the running sequencer inside the cluster."""

    configmap_name: str = "sequencer-node-config"
    statefulset_name: str = "sequencer-node-statefulset"
    serviceaccount_namespace_path: str = "/var/run/secrets/kubernetes.io/serviceaccount/namespace"


@dataclass(frozen=True, slots=True)
class SequencerTiming:
    """Polling defaults used by the manager."""

    poll_interval_seconds: float = 2.0
    scale_timeout_seconds: float = 3000.0


class SequencerManager:
    """
    High-level API for managing the sequencer node from within the cluster.
    """

    def __init__(
        self,
        namespace: str,
        core_v1: kubernetes.client.CoreV1Api,
        apps_v1: kubernetes.client.AppsV1Api,
        spec: SequencerKubeSpec = SequencerKubeSpec(),
        timing: SequencerTiming = SequencerTiming(),
    ) -> None:
        self._namespace = namespace
        self._core_v1 = core_v1
        self._apps_v1 = apps_v1
        self._spec = spec
        self._timing = timing

    @classmethod
    def from_incluster(
        cls,
        namespace: Optional[str] = None,
        spec: SequencerKubeSpec = SequencerKubeSpec(),
        timing: SequencerTiming = SequencerTiming(),
    ) -> "SequencerManager":
        """Create a manager using in-cluster auth and the pod's namespace."""
        kubernetes.config.load_incluster_config()
        resolved_namespace = namespace or _read_namespace_from_serviceaccount(
            spec.serviceaccount_namespace_path
        )
        return cls(
            namespace=resolved_namespace,
            core_v1=kubernetes.client.CoreV1Api(),
            apps_v1=kubernetes.client.AppsV1Api(),
            spec=spec,
            timing=timing,
        )

    @property
    def namespace(self) -> str:
        return self._namespace

    @property
    def pod_name(self) -> str:
        # StatefulSet pod ordinal 0 is the one we manage in these workflows.
        return f"{self._spec.statefulset_name}-0"

    def patch_node_config(self, mutator: ConfigMutator):
        """
        Read the node JSON config from the sequencer ConfigMap, mutate it in-place, and patch it back.
        """
        configmap_name = self._spec.configmap_name
        logger.info(f"Fetching ConfigMap '{configmap_name}' in namespace '{self._namespace}'...")
        configmap = self._core_v1.read_namespaced_config_map(configmap_name, self._namespace)

        config: JsonObject = json.loads(configmap.data["config"])

        mutator(config)

        body = {"data": {"config": json.dumps(config, indent=2)}}
        updated = self._core_v1.patch_namespaced_config_map(
            name=configmap_name, namespace=self._namespace, body=body
        )
        logger.info("ConfigMap updated successfully.")
        return updated

    def configure_revert(self, should_revert: bool):
        def _mutate(config: JsonObject) -> None:
            config["revert_config.should_revert"] = should_revert

        return self.patch_node_config(_mutate)

    def configure_start_sync(self):
        def _mutate(config: JsonObject) -> None:
            config["revert_config.should_revert"] = False
            config["starknet_url"] = CONFIG.feeder.base_url
            config["validator_id"] = "0x1"

        return self.patch_node_config(_mutate)

    def configure_stop_sync(self, block_number: int):
        def _mutate(config: JsonObject) -> None:
            config["revert_config.should_revert"] = True
            config["revert_config.revert_up_to_and_including"] = block_number
            config["starknet_url"] = "http://echonet:80"
            config["validator_id"] = "0x64"

        return self.patch_node_config(_mutate)

    def scale(self, replicas: int) -> None:
        stateful_set_name = self._spec.statefulset_name
        logger.info(
            f"Scaling StatefulSet '{stateful_set_name}' in namespace '{self._namespace}' to {replicas} replicas..."
        )
        self._apps_v1.patch_namespaced_stateful_set_scale(
            name=stateful_set_name,
            namespace=self._namespace,
            body={"spec": {"replicas": replicas}},
        )
        self._wait_for_statefulset_replicas(expected_replicas=replicas)
        logger.info(f"Scaling to {replicas} replicas done.")

    def restart_node(self) -> None:
        """Restart the node by scaling `1 -> 0 -> 1` (waits at each step)."""
        self.scale(replicas=0)
        self.scale(replicas=1)

    def _wait_for_statefulset_replicas(self, expected_replicas: int) -> None:
        stateful_set_name = self._spec.statefulset_name
        logger.info(
            f"Waiting for StatefulSet '{stateful_set_name}' to reach {expected_replicas} replicas..."
        )
        start = time.time()

        while True:
            stateful_set = self._apps_v1.read_namespaced_stateful_set(
                stateful_set_name, self._namespace
            )
            replicas = stateful_set.status.replicas or 0
            ready = stateful_set.status.ready_replicas or 0
            logger.info(f"Current replicas: {replicas}, ready: {ready}")

            if replicas == expected_replicas and ready == expected_replicas:
                logger.info(f"StatefulSet reached {expected_replicas} replicas.")
                return

            if time.time() - start > self._timing.scale_timeout_seconds:
                raise TimeoutError(
                    f"Timed out waiting for StatefulSet '{stateful_set_name}' "
                    f"to reach {expected_replicas} replicas."
                )

            time.sleep(self._timing.poll_interval_seconds)

    def wait_for_log_inactivity(
        self,
        inactivity_substrings: Sequence[str],
        inactivity_seconds: float = 15,
        timeout_seconds: float = 6000.0,
        poll_interval_seconds: float = 1.0,
        tail_lines: int = 500,
        pod_name: Optional[str] = None,
    ) -> None:
        """
        Block until none of `inactivity_substrings` appear in the last `inactivity_seconds` of pod
        logs.
        """
        pod = pod_name or self.pod_name
        logger.info(
            f"Waiting for {inactivity_seconds}s of log inactivity in pod '{pod}' (ns={self._namespace})... "
            f"(substrings: {', '.join(inactivity_substrings)})"
        )
        start = time.time()

        while True:
            logs_recent = self._read_pod_logs(
                pod_name=pod,
                tail_lines=tail_lines,
                # K8s `sinceSeconds` must be an integer; passing a float can yield 400 Bad Request.
                since_seconds=int(inactivity_seconds),
            )
            if not any(s in logs_recent for s in inactivity_substrings):
                logger.info(
                    f"No revert activity logs were seen for the last {inactivity_seconds}s in pod '{pod}'."
                )
                return

            if time.time() - start > timeout_seconds:
                raise TimeoutError(
                    f"Timed out after {timeout_seconds}s waiting for {inactivity_seconds}s of log "
                    f"inactivity in pod '{pod}'."
                )
            time.sleep(poll_interval_seconds)

    def wait_for_synced_block_at_least(
        self,
        target_block: int,
        timeout_seconds: float = 3000.0,
        poll_interval_seconds: float = 1.0,
        tail_lines: int = 500,
        pod_name: Optional[str] = None,
    ) -> None:
        """Wait until logs show a synced block height >= `target_block`."""
        pod = pod_name or self.pod_name
        logger.info(
            f"Waiting for synced block to be >= {target_block} in pod '{pod}' (ns={self._namespace})..."
        )

        pattern = re.compile(r"Adding sync block to Batcher for height (\d+)")
        start = time.time()

        while True:
            logs = self._read_pod_logs(pod_name=pod, tail_lines=tail_lines)
            highest_seen: Optional[int] = None

            for match in pattern.finditer(logs):
                block_num = int(match.group(1))
                highest_seen = block_num if highest_seen is None else max(highest_seen, block_num)
                if block_num >= target_block:
                    logger.info(
                        f"Found new block synced {block_num} (>= {target_block}) in pod '{pod}'."
                    )
                    return

            if highest_seen:
                logger.info(f"Latest new block synced found in recent logs: {highest_seen}")

            if time.time() - start > timeout_seconds:
                raise TimeoutError(
                    f"Timed out after {timeout_seconds}s waiting for new block synced>= {target_block} "
                    f"in pod '{pod}'."
                )

            time.sleep(poll_interval_seconds)

    def _read_pod_logs(
        self,
        pod_name: str,
        tail_lines: Optional[int],
        previous: bool = False,
        since_seconds: Optional[int] = None,
    ) -> str:
        try:
            logs = self._core_v1.read_namespaced_pod_log(
                name=pod_name,
                namespace=self._namespace,
                tail_lines=tail_lines,
                timestamps=False,
                previous=previous,
                since_seconds=since_seconds,
            )
            return logs or ""
        except ApiException as e:
            if previous and getattr(e, "status", None) == 400:
                return ""
            logger.warning(f"Could not read logs for pod {pod_name}: {e.reason}")
            return ""

    def scale_to_zero(self) -> None:
        """Scale the sequencer StatefulSet down to 0 replicas and wait until it reaches 0."""
        self.scale(replicas=0)

    def initial_revert_then_restore(self, block_number: int) -> None:
        """
        One-time sequence used at `transaction_sender` startup to the starting block:

        - Revert to the starting block (`block_number`) and restart the node.
        - Then disable revert and restart again.
        """
        logger.info("Running initial revert -> restore sequence...")

        self.configure_stop_sync(block_number=block_number)
        self.restart_node()
        self.wait_for_log_inactivity(
            inactivity_substrings=REVERT_INACTIVITY_SUBSTRINGS,
        )

        self.configure_revert(should_revert=False)
        self.restart_node()

        logger.info("Initial revert -> restore sequence completed.")

    def resync(self, block_number: int) -> None:
        """
        Full resync loop around a target block:
        - Enable revert + bounce + wait for pending
        - Start sync + bounce + wait for catching up
        - Stop sync at block + bounce + wait for pending
        - Disable revert + bounce
        """
        logger.info(f"Starting resync workflow around block {block_number}...")

        self.configure_revert(should_revert=True)
        self.restart_node()
        self.wait_for_log_inactivity(inactivity_substrings=REVERT_INACTIVITY_SUBSTRINGS)

        self.configure_start_sync()
        self.restart_node()
        self.wait_for_synced_block_at_least(target_block=block_number + 10)

        self.configure_stop_sync(block_number=block_number)
        self.restart_node()
        self.wait_for_log_inactivity(inactivity_substrings=REVERT_INACTIVITY_SUBSTRINGS)

        self.configure_revert(should_revert=False)
        self.restart_node()

        logger.info("Resync workflow complete.")


def _read_namespace_from_serviceaccount(namespace_path: str) -> str:
    with open(namespace_path, "r") as f:
        namespace = f.read().strip()
    logger.info(f"Auto-detected namespace: {namespace}")
    return namespace
