"""
Kubernetes orchestration helpers for the Echonet sequencer node.
"""

from __future__ import annotations

import json
import re
import time
from dataclasses import dataclass
from typing import Callable, MutableMapping, Optional

from kubernetes import client, config  # pyright: ignore[reportMissingImports]
from kubernetes.client.rest import ApiException  # pyright: ignore[reportMissingImports]

from echonet import consts
from echonet.logger import get_logger

logger = get_logger("manage_sequencer")


JsonObject = MutableMapping[str, object]
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
    scale_timeout_seconds: float = 200.0


class SequencerManager:
    """
    High-level API for managing the sequencer node from within the cluster.
    """

    def __init__(
        self,
        namespace: str,
        core_v1: client.CoreV1Api,
        apps_v1: client.AppsV1Api,
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
        config.load_incluster_config()
        resolved_namespace = namespace or _read_namespace_from_serviceaccount(
            spec.serviceaccount_namespace_path
        )
        return cls(
            namespace=resolved_namespace,
            core_v1=client.CoreV1Api(),
            apps_v1=client.AppsV1Api(),
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
        cm_name = self._spec.configmap_name
        logger.info("Fetching ConfigMap '%s' in namespace '%s'...", cm_name, self._namespace)
        cm = self._core_v1.read_namespaced_config_map(cm_name, self._namespace)

        cfg: JsonObject = json.loads(cm.data["config"])

        mutator(cfg)

        body = {"data": {"config": json.dumps(cfg, indent=2)}}
        updated = self._core_v1.patch_namespaced_config_map(
            name=cm_name, namespace=self._namespace, body=body
        )
        logger.info("ConfigMap updated successfully.")
        return updated

    def set_should_revert(self, should_revert: bool):
        def _mutate(cfg: JsonObject) -> None:
            cfg["revert_config.should_revert"] = should_revert

        return self.patch_node_config(_mutate)

    def configure_start_sync(self):
        def _mutate(cfg: JsonObject) -> None:
            cfg["revert_config.should_revert"] = False
            cfg["starknet_url"] = consts.CONFIG.feeder.base_url
            cfg["validator_id"] = "0x1"

        return self.patch_node_config(_mutate)

    def configure_stop_sync(self, block_number: int):
        def _mutate(cfg: JsonObject) -> None:
            cfg["revert_config.should_revert"] = True
            cfg["revert_config.revert_up_to_and_including"] = block_number
            cfg["consensus_manager_config.immediate_active_height"] = block_number
            cfg["consensus_manager_config.cende_config.skip_write_height"] = block_number
            cfg["starknet_url"] = "http://echonet:80"
            cfg["validator_id"] = "0x64"

        return self.patch_node_config(_mutate)

    def scale(self, replicas: int) -> None:
        ss_name = self._spec.statefulset_name
        logger.info(
            "Scaling StatefulSet '%s' in namespace '%s' to %s replicas...",
            ss_name,
            self._namespace,
            replicas,
        )
        self._apps_v1.patch_namespaced_stateful_set_scale(
            name=ss_name,
            namespace=self._namespace,
            body={"spec": {"replicas": replicas}},
        )
        self._wait_for_statefulset_replicas(expected_replicas=replicas)
        logger.info("Scaling to %s replicas done.", replicas)

    def bounce(self) -> None:
        """Restart the node by scaling `1 -> 0 -> 1` (waits at each step)."""
        self.scale(replicas=0)
        self.scale(replicas=1)

    def _wait_for_statefulset_replicas(self, expected_replicas: int) -> None:
        ss_name = self._spec.statefulset_name
        logger.info(
            "Waiting for StatefulSet '%s' to reach %s replicas...",
            ss_name,
            expected_replicas,
        )
        start = time.time()

        while True:
            ss = self._apps_v1.read_namespaced_stateful_set(ss_name, self._namespace)
            replicas = ss.status.replicas or 0
            ready = ss.status.ready_replicas or 0
            logger.info("Current replicas: %s, ready: %s", replicas, ready)

            if replicas == expected_replicas and ready == expected_replicas:
                logger.info("StatefulSet reached %s replicas.", expected_replicas)
                return

            if time.time() - start > self._timing.scale_timeout_seconds:
                raise TimeoutError(
                    f"Timed out waiting for StatefulSet '{ss_name}' "
                    f"to reach {expected_replicas} replicas."
                )

            time.sleep(self._timing.poll_interval_seconds)

    def wait_for_log_substring(
        self,
        substring: str,
        timeout_seconds: float = 6000.0,
        poll_interval_seconds: float = 1.0,
        tail_lines: int = 500,
        occurrences_required: int = 1,
        pod_name: Optional[str] = None,
    ) -> None:
        """
        Block until `substring` appears in the pod logs `occurrences_required` times, or timeout.
        """
        pod = pod_name or self.pod_name
        logger.info(
            "Waiting for log message '%s' in pod '%s' (ns=%s)... (required occurrences: %s)",
            substring,
            pod,
            self._namespace,
            occurrences_required,
        )
        start = time.time()
        tail_arg = None if occurrences_required > 1 else tail_lines

        while True:
            logs = self._read_pod_logs(pod_name=pod, tail_lines=tail_arg)

            if occurrences_required > 1:
                # If the container restarted, the message might exist only in previous logs.
                logs_prev = self._read_pod_logs(pod_name=pod, tail_lines=tail_arg, previous=True)
                if logs_prev:
                    logs = f"{logs_prev}\n{logs}"

            seen = logs.count(substring)
            if seen >= occurrences_required:
                logger.info(
                    "Found log message '%s' in pod '%s' (occurrences seen: %s/%s).",
                    substring,
                    pod,
                    seen,
                    occurrences_required,
                )
                return

            if time.time() - start > timeout_seconds:
                raise TimeoutError(
                    f"Timed out after {timeout_seconds}s waiting for log message "
                    f"'{substring}' in pod '{pod}' (seen {seen}/{occurrences_required})."
                )
            time.sleep(poll_interval_seconds)

    def wait_for_synced_block_at_least(
        self,
        target_block: int,
        timeout_seconds: float = 300.0,
        poll_interval_seconds: float = 1.0,
        tail_lines: int = 500,
        pod_name: Optional[str] = None,
    ) -> None:
        """Wait until logs show a synced block height >= `target_block`."""
        pod = pod_name or self.pod_name
        logger.info(
            "Waiting for synced block to be >= %s in pod '%s' (ns=%s)...",
            target_block,
            pod,
            self._namespace,
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
                        "Found new block synced %s (>= %s) in pod '%s'.",
                        block_num,
                        target_block,
                        pod,
                    )
                    return

            if highest_seen is not None:
                logger.info("Latest new block synced found in recent logs: %s", highest_seen)

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
    ) -> str:
        try:
            logs = self._core_v1.read_namespaced_pod_log(
                name=pod_name,
                namespace=self._namespace,
                tail_lines=tail_lines,
                timestamps=False,
                previous=previous,
            )
            return logs or ""
        except ApiException as e:
            logger.warning("Could not read logs for pod %s: %s", pod_name, e.reason)
            return ""

    def scale_to_zero(self) -> None:
        """Scale the sequencer StatefulSet down to 0 replicas and wait until it reaches 0."""
        self.scale(replicas=0)

    def initial_revert_then_restore(self, block_number: int) -> None:
        """
        One-time sequence used at `transaction_sender` startup:

        - Revert to `block_number` and restart the node.
        - Then disable revert and restart again.
        """
        logger.info("Running initial revert -> restore sequence...")

        self.configure_stop_sync(block_number=block_number)
        self.bounce()
        self.wait_for_log_substring(
            substring="Starting eternal pending",
            occurrences_required=2,
        )

        self.set_should_revert(should_revert=False)
        self.bounce()

        logger.info("Initial revert -> restore sequence completed.")

    def resync(self, block_number: int) -> None:
        """
        Full resync loop around a target block:
        - Enable revert + bounce + wait for pending
        - Start sync + bounce + wait for catching up
        - Stop sync at block + bounce + wait for pending
        - Disable revert + bounce
        """
        logger.info("Starting resync workflow around block %s...", block_number)

        self.set_should_revert(should_revert=True)
        self.bounce()
        self.wait_for_log_substring(
            substring="Starting eternal pending",
            occurrences_required=2,
        )

        self.configure_start_sync()
        self.bounce()
        self.wait_for_synced_block_at_least(target_block=block_number + 10)

        self.configure_stop_sync(block_number=block_number)
        self.bounce()
        self.wait_for_log_substring(
            substring="Starting eternal pending",
            occurrences_required=2,
        )

        self.set_should_revert(should_revert=False)
        self.bounce()

        logger.info("Resync workflow complete.")


def _read_namespace_from_serviceaccount(namespace_path: str) -> str:
    with open(namespace_path, "r") as f:
        ns = f.read().strip()
    logger.info("Auto-detected namespace: %s", ns)
    return ns
