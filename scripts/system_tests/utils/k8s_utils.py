"""
Kubernetes utilities using Python Kubernetes client.

This module provides high-level functions for common Kubernetes operations,
using the official Python Kubernetes client library for better error handling
and type safety.
"""
import subprocess
import time
from typing import List, Optional

from kubernetes import client, config
from kubernetes.client.rest import ApiException

# Lazy initialization of Kubernetes clients
_core_v1: Optional[client.CoreV1Api] = None
_apps_v1: Optional[client.AppsV1Api] = None
_config_loaded = False


def _ensure_config_loaded() -> None:
    """Ensure Kubernetes config is loaded."""
    global _config_loaded
    if not _config_loaded:
        config.load_kube_config()
        _config_loaded = True


def _get_core_v1() -> client.CoreV1Api:
    """Get or create CoreV1Api client."""
    global _core_v1
    _ensure_config_loaded()
    if _core_v1 is None:
        _core_v1 = client.CoreV1Api()
    return _core_v1


def _get_apps_v1() -> client.AppsV1Api:
    """Get or create AppsV1Api client."""
    global _apps_v1
    _ensure_config_loaded()
    if _apps_v1 is None:
        _apps_v1 = client.AppsV1Api()
    return _apps_v1


def make_cmd_verbose_if_needed(cmd: List[str], verbose: bool) -> List[str]:
    """
    Add verbose flag to kubectl command if needed.

    Inserts `-v=6` at position 1 (after "kubectl") when verbose is True.
    Used for kubectl cp subprocess wrapper.
    """
    if verbose:
        cmd = cmd.copy()  # Don't modify the original list
        cmd.insert(1, "-v=6")
    return cmd


def get_pod_name(
    label_selector: str, namespace: Optional[str] = None, verbose: bool = False
) -> str:
    """
    Get pod name by label selector.

    Args:
        label_selector: Label selector (e.g., "service=sequencer-core")
        namespace: Kubernetes namespace (optional, uses current context default if not provided)
        verbose: Enable verbose logging

    Returns:
        Pod name

    Raises:
        RuntimeError: If no pod found or multiple pods found
    """
    # Get namespace from current context if not provided
    if namespace is None:
        try:
            _ensure_config_loaded()
            # Try to get namespace from current context
            contexts, active_context = config.list_kube_config_contexts()
            if active_context and active_context.get("context", {}).get("namespace"):
                namespace = active_context["context"]["namespace"]
            else:
                namespace = "default"
            if verbose:
                print(f"🔍 Using namespace from current context: {namespace}")
        except Exception:
            namespace = "default"
            if verbose:
                print(f"🔍 Could not determine namespace from context, using default: {namespace}")

    if verbose:
        print(f"🔍 Finding pod with label selector: {label_selector} in namespace {namespace}")

    try:
        core_v1 = _get_core_v1()
        pods = core_v1.list_namespaced_pod(
            namespace=namespace,
            label_selector=label_selector,
        )

        if not pods.items:
            raise RuntimeError(
                f"No pod found with label selector '{label_selector}' in namespace '{namespace}'"
            )

        if len(pods.items) > 1:
            pod_names = [p.metadata.name for p in pods.items]
            if verbose:
                print(f"⚠️  Multiple pods found: {pod_names}, using first one: {pod_names[0]}")
            else:
                print(f"⚠️  Multiple pods found, using first one: {pod_names[0]}")

        pod_name = pods.items[0].metadata.name
        if verbose:
            print(f"✅ Found pod: {pod_name}")
        return pod_name

    except ApiException as e:
        error_msg = f"Failed to get pod with label selector '{label_selector}': {e}"
        if verbose:
            error_msg += f"\n   Status: {e.status}\n   Reason: {e.reason}\n   Body: {e.body}"
        raise RuntimeError(error_msg) from e


def exec_in_pod(
    pod_name: str, namespace: str, command: List[str], verbose: bool = False
) -> tuple[str, str]:
    """
    Execute command in pod.

    Args:
        pod_name: Pod name
        namespace: Kubernetes namespace
        command: Command to execute (list of strings)
        verbose: Enable verbose logging

    Returns:
        Tuple of (stdout, stderr)

    Raises:
        RuntimeError: If command execution fails
    """
    if verbose:
        print(f"🔧 Executing command in pod {pod_name}: {' '.join(command)}")

    try:
        core_v1 = _get_core_v1()
        # Use _preload_content=True to get output as string
        # stdout and stderr are combined in the response
        resp = core_v1.connect_get_namespaced_pod_exec(
            pod_name,
            namespace,
            command=command,
            stdout=True,
            stderr=True,
            stdin=False,
            tty=False,
            _preload_content=True,
        )

        # Response is a string containing both stdout and stderr
        # For most commands, stderr is empty unless there's an error
        stdout = resp if isinstance(resp, str) else ""
        stderr = ""  # Kubernetes exec combines stdout/stderr, we can't easily separate them

        if verbose:
            print(f"✅ Command executed successfully")
            if stdout:
                print(f"   output: {stdout}")

        return stdout, stderr

    except ApiException as e:
        error_msg = f"Failed to execute command in pod {pod_name}: {e}"
        if verbose:
            error_msg += f"\n   Status: {e.status}\n   Reason: {e.reason}\n   Body: {e.body}"
        raise RuntimeError(error_msg) from e


def delete_pod(pod_name: str, namespace: str, verbose: bool = False) -> None:
    """
    Delete a pod.

    Args:
        pod_name: Pod name
        namespace: Kubernetes namespace
        verbose: Enable verbose logging

    Raises:
        RuntimeError: If pod deletion fails
    """
    if verbose:
        print(f"🗑️  Deleting pod {pod_name} in namespace {namespace}")

    try:
        core_v1 = _get_core_v1()
        core_v1.delete_namespaced_pod(
            name=pod_name,
            namespace=namespace,
            grace_period_seconds=0,  # Immediate deletion
        )
        if verbose:
            print(f"✅ Pod {pod_name} deleted successfully")
    except ApiException as e:
        error_msg = f"Failed to delete pod {pod_name}: {e}"
        if verbose:
            error_msg += f"\n   Status: {e.status}\n   Reason: {e.reason}\n   Body: {e.body}"
        raise RuntimeError(error_msg) from e


def wait_for_deployment(
    name: str, namespace: str, timeout: int = 180, verbose: bool = False
) -> None:
    """
    Wait for deployment to become ready.

    Args:
        name: Deployment name
        namespace: Kubernetes namespace
        timeout: Timeout in seconds
        verbose: Enable verbose logging

    Raises:
        RuntimeError: If timeout is reached
    """
    if verbose:
        print(f"⏳ Waiting for deployment {name} to become ready (timeout: {timeout}s)")

    apps_v1 = _get_apps_v1()
    poll_interval = 5
    elapsed = 0

    while elapsed < timeout:
        try:
            status = apps_v1.read_namespaced_deployment_status(
                name=name, namespace=namespace
            ).status
            ready = status.ready_replicas or 0
            desired = status.replicas or 0

            if verbose:
                print(f"   Status: ready={ready}, desired={desired}")

            if ready == desired and ready > 0:
                print(f"✅ Deployment {name} is ready")
                return

        except ApiException as e:
            if verbose:
                print(f"   Error checking status: {e.status} - {e.reason}")

        time.sleep(poll_interval)
        elapsed += poll_interval

    raise RuntimeError(f"Timeout waiting for deployment {name} to become ready after {timeout}s")


def wait_for_statefulset(
    name: str, namespace: str, timeout: int = 180, verbose: bool = False
) -> None:
    """
    Wait for statefulset to become ready.

    Args:
        name: StatefulSet name
        namespace: Kubernetes namespace
        timeout: Timeout in seconds
        verbose: Enable verbose logging

    Raises:
        RuntimeError: If timeout is reached
    """
    if verbose:
        print(f"⏳ Waiting for statefulset {name} to become ready (timeout: {timeout}s)")

    apps_v1 = _get_apps_v1()
    poll_interval = 5
    elapsed = 0

    while elapsed < timeout:
        try:
            status = apps_v1.read_namespaced_stateful_set_status(
                name=name, namespace=namespace
            ).status
            ready = status.ready_replicas or 0
            desired = status.replicas or 0

            if verbose:
                print(f"   Status: ready={ready}, desired={desired}")

            if ready == desired and ready > 0:
                print(f"✅ StatefulSet {name} is ready")
                return

        except ApiException as e:
            if verbose:
                print(f"   Error checking status: {e.status} - {e.reason}")

        time.sleep(poll_interval)
        elapsed += poll_interval

    raise RuntimeError(f"Timeout waiting for statefulset {name} to become ready after {timeout}s")


def wait_for_resource(
    controller: str, name: str, namespace: str, timeout: int = 180, verbose: bool = False
) -> None:
    """
    Wait for resource (deployment or statefulset) to become ready.

    Args:
        controller: Controller type ("deployment" or "statefulset")
        name: Resource name
        namespace: Kubernetes namespace
        timeout: Timeout in seconds
        verbose: Enable verbose logging

    Raises:
        RuntimeError: If unknown controller type or timeout is reached
    """
    controller_lower = controller.lower()
    if controller_lower == "deployment":
        wait_for_deployment(name, namespace, timeout, verbose)
    elif controller_lower == "statefulset":
        wait_for_statefulset(name, namespace, timeout, verbose)
    else:
        raise RuntimeError(
            f"Unknown controller type: {controller}. Supported: deployment, statefulset"
        )


def port_forward(
    pod_name: str,
    local_port: int,
    remote_port: int,
    namespace: Optional[str] = None,
    verbose: bool = False,
) -> subprocess.Popen:
    """
    Port forward to a pod.

    Args:
        pod_name: Pod name
        local_port: Local port
        remote_port: Remote port in pod
        namespace: Kubernetes namespace (optional, uses current context default if not provided)
        verbose: Enable verbose logging

    Returns:
        subprocess.Popen process handle

    Note:
        This uses subprocess with kubectl for compatibility with existing code
        that expects a Popen handle. The Python client's port-forward is more
        complex and doesn't return a process handle.
    """
    if verbose:
        print(f"🔌 Port-forwarding {pod_name}:{remote_port} -> localhost:{local_port}")

    cmd = ["kubectl", "port-forward", pod_name, f"{local_port}:{remote_port}"]
    if namespace:
        cmd.extend(["-n", namespace])
    cmd = make_cmd_verbose_if_needed(cmd, verbose)

    process = subprocess.Popen(
        cmd,
        stdout=subprocess.DEVNULL if not verbose else None,
        stderr=subprocess.DEVNULL if not verbose else None,
    )

    if verbose:
        print(f"✅ Port-forward process started (PID: {process.pid})")

    return process


def copy_to_pod(
    pod_name: str, namespace: str, local_path: str, remote_path: str, verbose: bool = False
) -> None:
    """
    Copy files to pod using kubectl cp.

    Args:
        pod_name: Pod name
        namespace: Kubernetes namespace
        local_path: Local file/directory path
        remote_path: Remote path in pod
        verbose: Enable verbose logging

    Raises:
        RuntimeError: If copy fails

    Note:
        This uses subprocess with kubectl cp because the Python client doesn't
        have a direct equivalent. kubectl cp handles tar streaming internally.
    """
    if verbose:
        print(f"📥 Copying {local_path} to {pod_name}:{remote_path}")

    cmd = make_cmd_verbose_if_needed(
        ["kubectl", "cp", local_path, f"{namespace}/{pod_name}:{remote_path}", "--retries=3"],
        verbose,
    )

    try:
        result = subprocess.run(cmd, check=True, text=True, capture_output=True)
        if verbose:
            if result.stdout:
                print(f"   stdout: {result.stdout}")
            if result.stderr:
                print(f"   stderr: {result.stderr}")
        print(f"✅ Files copied successfully")
    except subprocess.CalledProcessError as e:
        error_msg = f"Failed to copy files to pod {pod_name}: {e}"
        if e.stdout:
            error_msg += f"\nstdout: {e.stdout}"
        if e.stderr:
            error_msg += f"\nstderr: {e.stderr}"
        raise RuntimeError(error_msg) from e
