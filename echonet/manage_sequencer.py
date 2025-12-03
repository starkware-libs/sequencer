import json
import re
import time

from kubernetes import client, config  # pyright: ignore[reportMissingImports]
from kubernetes.client.rest import ApiException  # pyright: ignore[reportMissingImports]

CONFIGMAP_NAME = "sequencer-node-config"
STATEFULSET_NAME = "sequencer-node-statefulset"

# If None, we detect from serviceaccount file inside the pod
NAMESPACE = None

POLL_INTERVAL = 2
SCALE_TIMEOUT = 120


def _patch_configmap(core_v1, namespace, update_fn):
    """
    Generic helper to:
      - read the sequencer configmap
      - apply an in-place update to the decoded JSON config
      - write the updated config back
    """
    print(f"[INFO] Fetching ConfigMap '{CONFIGMAP_NAME}' in namespace '{namespace}'...")
    cm = core_v1.read_namespaced_config_map(CONFIGMAP_NAME, namespace)

    raw_cfg = cm.data["config"]
    cfg = json.loads(raw_cfg)

    update_fn(cfg)

    new_raw_cfg = json.dumps(cfg, indent=2)
    body = {"data": {"config": new_raw_cfg}}

    updated_cm = core_v1.patch_namespaced_config_map(
        name=CONFIGMAP_NAME,
        namespace=namespace,
        body=body,
    )

    print("[INFO] ConfigMap updated successfully.")
    return updated_cm


def get_namespace():
    if NAMESPACE:
        return NAMESPACE
    with open("/var/run/secrets/kubernetes.io/serviceaccount/namespace", "r") as f:
        ns = f.read().strip()
        print(f"[INFO] Auto-detected namespace: {ns}")
        return ns


def patch_configmap_should_revert(core_v1, namespace, should_revert: bool):
    def _update(cfg):
        cfg["revert_config.should_revert"] = should_revert

    return _patch_configmap(core_v1, namespace, _update)


def patch_configmap_start_sync(core_v1, namespace, block_number: int):
    # NOTE: block_number is currently unused but kept in the signature for API compatibility.
    def _update(cfg):
        cfg["revert_config.should_revert"] = False
        cfg["starknet_url"] = "https://feeder.alpha-mainnet.starknet.io/"
        cfg["validator_id"] = "0x1"

    return _patch_configmap(core_v1, namespace, _update)


def patch_configmap_stop_sync(core_v1, namespace, block_number: int):
    def _update(cfg):
        cfg["revert_config.should_revert"] = True
        cfg["revert_config.revert_up_to_and_including"] = block_number
        cfg["consensus_manager_config.immediate_active_height"] = block_number
        cfg["consensus_manager_config.cende_config.skip_write_height"] = block_number
        cfg["starknet_url"] = "http://echonet:80"
        cfg["validator_id"] = "0x64"

    return _patch_configmap(core_v1, namespace, _update)


def wait_for_log_message(
    core_v1: client.CoreV1Api,
    namespace: str,
    substring: str,
    pod_name: str = f"{STATEFULSET_NAME}-0",
    timeout: int = 6000,
    poll_interval: int = 1,
    tail_lines: int = 500,
    occurrences_required: int = 1,
):
    """
    Block until `substring` appears in the logs of `pod_name` the required number of times, or timeout.

    :param core_v1: CoreV1Api instance
    :param namespace: Namespace of the pod
    :param pod_name: Name of the pod to watch
    :param substring: Text to look for in logs
    :param timeout: Max seconds to wait
    :param poll_interval: Seconds between log checks
    :param tail_lines: How many log lines to fetch each time
    :param occurrences_required: Number of times the substring must appear to succeed
    """
    print(
        f"[INFO] Waiting for log message '{substring}' "
        f"in pod '{pod_name}' (ns={namespace})... "
        f"(required occurrences: {occurrences_required})"
    )
    start = time.time()

    # Polling path (robust for single or multiple occurrences)
    while True:
        try:
            # If requiring multiple occurrences (streaming failed), fetch logs since start time.
            since_seconds = None
            tail_arg = tail_lines
            if occurrences_required > 1:
                # Fetch full logs (no since_seconds) to avoid missing early lines between polls.
                tail_arg = None

            logs_current = core_v1.read_namespaced_pod_log(
                name=pod_name,
                namespace=namespace,
                tail_lines=tail_arg,
                since_seconds=since_seconds,
                timestamps=False,
            )
            logs = logs_current or ""
            # If still short on occurrences, also include previous container logs (in case of restart).
            if occurrences_required > 1:
                try:
                    logs_prev = core_v1.read_namespaced_pod_log(
                        name=pod_name,
                        namespace=namespace,
                        tail_lines=tail_arg,
                        since_seconds=since_seconds,
                        timestamps=False,
                        previous=True,
                    )
                    if logs_prev:
                        logs = f"{logs_prev}\n{logs}"
                except ApiException:
                    # No previous logs available; ignore.
                    pass
        except ApiException as e:
            # Pod may not be ready yet; keep retrying unless timeout hits
            print(f"[WARN] Could not read logs for pod {pod_name}: {e.reason}")
            logs = ""

        # Count occurrences within the fetched logs since start (or tail if single occurrence)
        try:
            current_count = logs.count(substring)
        except Exception:
            current_count = 0

        if current_count >= occurrences_required:
            print(
                f"[INFO] Found log message '{substring}' in pod '{pod_name}' "
                f"(occurrences seen: {current_count}/{occurrences_required})."
            )
            return

        if time.time() - start > timeout:
            raise TimeoutError(
                f"Timed out after {timeout}s waiting for log message "
                f"'{substring}' in pod '{pod_name}' "
                f"(seen {current_count}/{occurrences_required})."
            )

        time.sleep(poll_interval)


def wait_for_sync_new_block_at_least(
    core_v1: client.CoreV1Api,
    namespace: str,
    target_block: int,
    pod_name: str = f"{STATEFULSET_NAME}-0",
    timeout: int = 300,
    poll_interval: int = 1,
    tail_lines: int = 500,
):
    """
    Block until we observe a log line like:
      'SYNC_NEW_BLOCK: Added block <number>'
    where <number> >= target_block.
    """
    print(
        f"[INFO] Waiting for SYNC_NEW_BLOCK >= {target_block} "
        f"in pod '{pod_name}' (ns={namespace})..."
    )
    pattern = re.compile(r"Adding sync block to Batcher for height (\d+)")
    start = time.time()

    while True:
        try:
            logs = core_v1.read_namespaced_pod_log(
                name=pod_name,
                namespace=namespace,
                tail_lines=tail_lines,
                timestamps=False,
            )
        except ApiException as e:
            print(f"[WARN] Could not read logs for pod {pod_name}: {e.reason}")
            logs = ""

        matched_block = None
        for match in pattern.finditer(logs):
            block_num = int(match.group(1))
            if matched_block is None or block_num > matched_block:
                matched_block = block_num
            if block_num >= target_block:
                print(
                    f"[INFO] Found SYNC_NEW_BLOCK {block_num} (>= {target_block}) in pod '{pod_name}'."
                )
                return

        if matched_block is not None:
            print(f"   - Latest SYNC_NEW_BLOCK found in recent logs: {matched_block}")

        if time.time() - start > timeout:
            raise TimeoutError(
                f"Timed out after {timeout}s waiting for SYNC_NEW_BLOCK >= {target_block} "
                f"in pod '{pod_name}'."
            )

        time.sleep(poll_interval)


def wait_for_statefulset_replicas(apps_v1, namespace, expected_replicas):
    print(
        f"[INFO] Waiting for StatefulSet '{STATEFULSET_NAME}' to reach {expected_replicas} replicas..."
    )
    start = time.time()

    while True:
        ss = apps_v1.read_namespaced_stateful_set(STATEFULSET_NAME, namespace)
        replicas = ss.status.replicas or 0
        ready = ss.status.ready_replicas or 0

        print(f"   - Current replicas: {replicas}, ready: {ready}")

        if replicas == expected_replicas and ready == expected_replicas:
            print(f"[INFO] StatefulSet reached {expected_replicas} replicas.")
            return

        if time.time() - start > SCALE_TIMEOUT:
            raise TimeoutError(
                f"Timed out waiting for StatefulSet '{STATEFULSET_NAME}' "
                f"to reach {expected_replicas} replicas."
            )

        time.sleep(POLL_INTERVAL)


def scale_statefulset(apps_v1, namespace, replicas):
    print(
        f"[INFO] Scaling StatefulSet '{STATEFULSET_NAME}' in namespace '{namespace}' to {replicas} replicas..."
    )
    body = {"spec": {"replicas": replicas}}

    apps_v1.patch_namespaced_stateful_set_scale(
        name=STATEFULSET_NAME,
        namespace=namespace,
        body=body,
    )

    wait_for_statefulset_replicas(apps_v1, namespace, replicas)
    print(f"[INFO] Scaling to {replicas} replicas done.")


def scale_sequencer_to_zero():
    """
    Scale the sequencer StatefulSet down to 0 replicas and wait until it reaches 0.
    """
    print(f"[INFO] Scaling down StatefulSet '{STATEFULSET_NAME}' to 0 replicas with wait...")
    config.load_incluster_config()
    namespace = get_namespace()
    apps_v1 = client.AppsV1Api()
    scale_statefulset(apps_v1, namespace, replicas=0)
    print("[INFO] Sequencer scaled down to 0 replicas.")


def initial_revert_then_restore(block_number: int):
    """
    One-time sequence:
      - Set revert=true to the given block_number and cycle the statefulset 0->1
      - Set revert=false and cycle the statefulset 0->1 again
    Intended to be called at transaction_sender startup.
    """
    print("[INFO] Running initial revert -> restore sequence...")
    config.load_incluster_config()
    namespace = get_namespace()
    core_v1 = client.CoreV1Api()
    apps_v1 = client.AppsV1Api()

    # Revert to target block and bounce the pod
    patch_configmap_stop_sync(core_v1, namespace, block_number)
    scale_statefulset(apps_v1, namespace, replicas=0)
    scale_statefulset(apps_v1, namespace, replicas=1)
    wait_for_log_message(core_v1, namespace, "Starting eternal pending", occurrences_required=2)

    # Restore (revert=false) and bounce the pod again
    patch_configmap_should_revert(core_v1, namespace, should_revert=False)
    scale_statefulset(apps_v1, namespace, replicas=0)
    scale_statefulset(apps_v1, namespace, replicas=1)

    print("[INFO] Initial revert -> restore sequence completed.")


def resync_sequencer(block_number: int):
    print("[INFO] Loading in-cluster Kubernetes config...")
    config.load_incluster_config()

    namespace = get_namespace()
    core_v1 = client.CoreV1Api()
    apps_v1 = client.AppsV1Api()

    try:
        # 1. Update config: revert_config.should_revert = true
        patch_configmap_should_revert(core_v1, namespace, should_revert=True)

        # 2. Scale statefulset to 0
        scale_statefulset(apps_v1, namespace, replicas=0)

        # 3. Scale statefulset back to 1
        scale_statefulset(apps_v1, namespace, replicas=1)

        # 4. Wait for log message
        wait_for_log_message(core_v1, namespace, "Starting eternal pending", occurrences_required=2)

        # 5. Update config to start sync
        patch_configmap_start_sync(core_v1, namespace, block_number)

        # 6. Scale statefulset to 0
        scale_statefulset(apps_v1, namespace, replicas=0)

        # 7. Scale statefulset back to 1
        scale_statefulset(apps_v1, namespace, replicas=1)

        # 8. Wait for log message
        wait_for_sync_new_block_at_least(core_v1, namespace, block_number + 10)

        patch_configmap_stop_sync(core_v1, namespace, block_number)

        # 9. Scale statefulset to 0
        scale_statefulset(apps_v1, namespace, replicas=0)

        # 10. Scale statefulset back to 1
        scale_statefulset(apps_v1, namespace, replicas=1)

        # 11. Wait for log message
        wait_for_log_message(core_v1, namespace, "Starting eternal pending", occurrences_required=2)

        # 12. Update config: revert_config.should_revert = false
        patch_configmap_should_revert(core_v1, namespace, should_revert=False)

        # 13. Scale statefulset to 0
        scale_statefulset(apps_v1, namespace, replicas=0)

        # 14. Scale statefulset back to 1
        scale_statefulset(apps_v1, namespace, replicas=1)

        print("[INFO] Done: ConfigMap updated and StatefulSet scaled 0 -> 1.")
    except ApiException as e:
        print(f"[ERROR] Kubernetes API exception: {e}")
        print(f"Reason: {e.reason}")
        print(f"Body: {e.body}")
    except Exception as e:
        print(f"[ERROR] Unexpected error: {e}")
