#!/usr/bin/env python3
"""
Python wrapper to deploy echonet via Kustomize.

- packages echonet source into a configmap-friendly bundle
- copies `echonet/echonet_keys.json` into `echonet/k8s/echonet/generated/echonet_keys.json`
- `kubectl apply -k` on the kustomize dir

Secrets must be applied separately (once):
  kubectl apply -f echonet/k8s/echonet/secret.yaml

Use argument `-x` to delete existing resources first (kubectl delete -k).
"""

from __future__ import annotations

import argparse
import base64
import logging
import select
import shlex
import shutil
import socket
import subprocess
import tarfile
import time
import urllib.error
import urllib.request
from pathlib import Path

from constants import ECHONET_KEYS_FILENAME
from helpers import read_json_object

logger = logging.getLogger("deploy_echonet")


def _check_prereqs() -> None:
    """Ensure required CLIs and (best-effort) auth are in place."""
    if shutil.which("kubectl") is None:
        logger.error("`kubectl` not found on PATH. Please install kubectl and try again.")
        raise SystemExit(1)


def _check_gcloud_auth() -> None:
    """
    Check that gcloud has an active authenticated account.
    """
    if shutil.which("gcloud") is None:
        logger.info("`gcloud` not found on PATH; skipping gcloud auth check.")
        return

    proc = subprocess.run(
        ["gcloud", "auth", "list", "--filter=status:ACTIVE", "--format=value(account)"],
        check=False,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        stderr = (proc.stderr or "").strip()
        logger.error(
            f"Failed to verify gcloud auth (exit {proc.returncode}): {stderr or '<no stderr>'}"
        )
        logger.error("Try running: gcloud auth login")
        raise SystemExit(1)

    active = (proc.stdout or "").strip()
    if not active:
        logger.error("No active gcloud account found. Try running: gcloud auth login")
        raise SystemExit(1)

    logger.info(f"Using gcloud account: {active}")


def _run(cmd: list[str], **kwargs) -> None:
    """Run a command, echoing it first."""
    logger.info(f"Running: {shlex.join(cmd)}")
    subprocess.run(cmd, check=True, **kwargs)


def _should_exclude(rel_posix: str) -> bool:
    parts = rel_posix.split("/")
    if any(
        p in {"__pycache__", ".pytest_cache", ".mypy_cache", ".ruff_cache", ".git", ".venv"}
        for p in parts
    ):
        return True
    if rel_posix.endswith((".pyc", ".pyo")):
        return True
    if parts and parts[0] == "k8s":
        return True
    return False


# TODO(Ron): Investigate and consider other solutions such as PyInstaller
def _build_source_bundle(echonet_dir: Path, out_path: Path) -> None:
    """
    Create a base64-encoded gzipped tarball containing the *directory tree* rooted at
    `echonet_dir`, with archive paths prefixed by `echonet/` to preserve imports.
    """
    out_path.parent.mkdir(parents=True, exist_ok=True)

    tmp_tgz = out_path.with_suffix(".tgz.tmp")
    tmp_b64 = out_path.with_suffix(out_path.suffix + ".tmp")
    for p in (tmp_tgz, tmp_b64):
        p.unlink(missing_ok=True)

    base_prefix = Path(echonet_dir.name)  # "echonet"

    def _tar_filter(ti: tarfile.TarInfo) -> tarfile.TarInfo:
        ti.uid = 0
        ti.gid = 0
        ti.uname = ""
        ti.gname = ""
        ti.mtime = 0
        return ti

    with tarfile.open(tmp_tgz, "w:gz") as tf:
        root_ti = tarfile.TarInfo(str(base_prefix))
        root_ti.type = tarfile.DIRTYPE
        root_ti.mode = 0o755
        tf.addfile(_tar_filter(root_ti))

        for p in sorted(echonet_dir.rglob("*")):
            rel = p.relative_to(echonet_dir)
            rel_posix = rel.as_posix()
            if _should_exclude(rel_posix):
                continue
            arcname = (base_prefix / rel).as_posix()
            # Preserve symlinks as symlinks; otherwise add file/dir contents.
            tf.add(str(p), arcname=arcname, recursive=False, filter=_tar_filter)

    # Encode as base64 text for ConfigMap embedding.
    with open(tmp_tgz, "rb") as f_in, open(tmp_b64, "wb") as f_out:
        # Wrap lines to keep manifests readable/debuggable; decoder tolerates newlines.
        f_out.write(base64.encodebytes(f_in.read()))

    tmp_tgz.unlink()
    tmp_b64.replace(out_path)
    size_kb = out_path.stat().st_size / 1024.0
    logger.info(f"Built source bundle: {out_path} ({size_kb:.1f} KiB, base64)")


def _namespace_args(namespace: str | None) -> list[str]:
    return ["-n", namespace] if namespace else []


def _wait_for_port_forward_ready(proc: subprocess.Popen[str], timeout_seconds: float = 8.0) -> str:
    """
    Best-effort: wait until kubectl prints "Forwarding from ..." (or exits).

    Returns a short captured snippet of kubectl output, useful for debugging failures.
    """
    if proc.stdout is None:
        return ""

    captured: list[str] = []
    deadline = time.time() + timeout_seconds
    while time.time() < deadline:
        if proc.poll() is not None:
            snippet = "\n".join(captured).strip()
            raise RuntimeError(f"port-forward process exited early.\n{snippet}".rstrip())

        # Non-blocking read: capture any available lines so kubectl errors are visible.
        rlist, _w, _x = select.select([proc.stdout], [], [], 0.15)
        if not rlist:
            continue

        line = proc.stdout.readline()
        if not line:
            continue

        s = line.rstrip("\n")
        if s:
            captured.append(s)
            # Keep the last N lines to avoid unbounded memory.
            if len(captured) > 25:
                captured = captured[-25:]

        if "Forwarding from" in s:
            return "\n".join(captured).strip()

    # Timed out, but the process is still alive: allow it to run and stream output.
    return "\n".join(captured).strip()


def _probe_http_ok(url: str, timeout_seconds: float = 1.0) -> bool:
    """
    Best-effort HTTP probe using the stdlib.

    Returns True on any 2xx response.
    """
    try:
        req = urllib.request.Request(url, method="GET")
        with urllib.request.urlopen(req, timeout=timeout_seconds) as resp:
            code = getattr(resp, "status", None)
            return bool(code) and 200 <= int(code) < 300
    except (urllib.error.HTTPError,) as e:
        # Endpoint exists but returned non-2xx.
        try:
            return 200 <= int(e.code) < 300
        except Exception:
            return False
    except (urllib.error.URLError, socket.timeout, ConnectionError, OSError):
        return False
    except Exception:
        return False


def _copy_generated_keys(keys_in_repo: Path, generated_path: Path) -> None:
    """
    Copy the non-secret echonet keys JSON into the kustomize generated/ directory.

    This file is consumed by the kustomize configMapGenerator (`echonet-keys`),
    then copied into the echonet PVC at /data/echonet/echonet_keys.json by an initContainer.
    """
    generated_path.parent.mkdir(parents=True, exist_ok=True)
    data = read_json_object(keys_in_repo)
    if "start_block" not in data:
        raise ValueError("Missing required key: start_block")

    if int(data["start_block"]) == 0:
        logger.error(
            f"Refusing to deploy: start_block is 0 in {keys_in_repo}. "
            "Set a non-zero start_block and re-run."
        )
        raise SystemExit(1)

    shutil.copyfile(keys_in_repo, generated_path)
    logger.info(f"Copied keys file: {keys_in_repo} -> {generated_path}")


def main(argv: list[str] | None = None) -> int:
    logging.basicConfig(level=logging.INFO, format="[deploy] %(levelname)s: %(message)s")

    _check_prereqs()
    _check_gcloud_auth()

    parser = argparse.ArgumentParser(description="Deploy echonet via kubectl + kustomize.")
    parser.add_argument(
        "-x",
        dest="delete_first",
        action="store_true",
        help="Delete existing resources first (kubectl delete -k).",
    )
    parser.add_argument(
        "-n",
        "--namespace",
        default=None,
        help="Kubernetes namespace (default: current context namespace).",
    )
    parser.add_argument(
        "--port-forward",
        action="store_true",
        help="After rollout, run `kubectl port-forward svc/echonet` and keep it in the foreground.",
    )
    parser.add_argument(
        "--port-forward-local-port",
        type=int,
        default=18080,
        help="Local port for --port-forward (default: 18080).",
    )
    args = parser.parse_args(argv)

    # Paths
    script_dir = Path(__file__).resolve().parent

    kustomize_dir = script_dir / "k8s" / "echonet"

    if not kustomize_dir.is_dir():
        logger.error(f"Kustomize directory not found at {kustomize_dir}")
        return 1

    # Build a configmap-friendly bundle of the echonet/ tree so imports work in the pod.
    generated_dir = kustomize_dir / "generated"
    bundle_path = generated_dir / "echonet-src.tgz.b64"
    _build_source_bundle(echonet_dir=script_dir, out_path=bundle_path)

    # Write non-secret echonet keys file into generated/ so kustomize can build the configmap.
    keys_in_repo = script_dir / ECHONET_KEYS_FILENAME
    generated_keys_path = generated_dir / ECHONET_KEYS_FILENAME
    _copy_generated_keys(keys_in_repo=keys_in_repo, generated_path=generated_keys_path)

    namespace_args = _namespace_args(args.namespace)

    if args.delete_first:
        logger.info("Deleting existing resources...")
        _run(["kubectl", *namespace_args, "delete", "-k", str(kustomize_dir), "--ignore-not-found"])

    # Ensure the sequencer is scaled down before deploying/updating echonet.
    logger.info("Scaling down statefulset/sequencer-node-statefulset to 0 replicas...")
    _run(
        [
            "kubectl",
            *namespace_args,
            "scale",
            "statefulset",
            "sequencer-node-statefulset",
            "--replicas=0",
        ]
    )
    logger.info("Waiting for rollout status statefulset/sequencer-node-statefulset...")
    _run(
        ["kubectl", *namespace_args, "rollout", "status", "statefulset/sequencer-node-statefulset"]
    )

    # Apply manifests
    logger.info("Applying manifests...")
    _run(["kubectl", *namespace_args, "apply", "-k", str(kustomize_dir)])

    # Wait for rollout to complete.
    logger.info("Waiting for rollout status deployment/echonet...")
    _run(["kubectl", *namespace_args, "rollout", "status", "deployment/echonet"])

    if args.port_forward:
        local_port = int(args.port_forward_local_port)
        if local_port <= 0 or local_port > 65535:
            raise ValueError(f"Invalid --port-forward-local-port: {local_port}")

        json_url = f"http://127.0.0.1:{local_port}/echonet/report"
        ui_url = f"http://127.0.0.1:{local_port}/echonet/report/ui"

        cmd = [
            "kubectl",
            *namespace_args,
            "port-forward",
            "svc/echonet",
            f"{local_port}:80",
        ]
        logger.info(f"Starting port-forward (Ctrl+C to stop): {shlex.join(cmd)}")
        logger.info("Waiting for echonet HTTP server to accept connections...")

        deadline = time.time() + 90.0
        proc: subprocess.Popen[str] | None = None
        captured = ""

        try:
            while True:
                if time.time() > deadline:
                    raise RuntimeError(
                        "Timed out waiting for echonet HTTP to become ready. "
                        f"Last known URL: {json_url}"
                    )

                # (Re)start port-forward.
                proc = subprocess.Popen(
                    cmd,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.STDOUT,
                    text=True,
                )

                try:
                    captured = _wait_for_port_forward_ready(proc)
                except Exception as e:
                    # Surface kubectl output and retry briefly (common: app not ready yet).
                    logger.warning(str(e))
                    proc.terminate()
                    proc.wait(timeout=5)
                    time.sleep(0.4)
                    continue

                # Now that the tunnel is up, probe the HTTP endpoint until it responds.
                while time.time() <= deadline:
                    if proc.poll() is not None:
                        # Port-forward died (e.g., because the target port wasn't listening yet).
                        break
                    if _probe_http_ok(json_url, timeout_seconds=1.0):
                        # Ready!
                        logger.info(f"Open: {ui_url}")
                        if captured:
                            print(captured)
                        assert proc.stdout is not None
                        for line in proc.stdout:
                            print(line.rstrip())
                        return 0
                    time.sleep(0.5)

                # Not ready yet; restart port-forward.
                if proc.poll() is None:
                    proc.terminate()
                    try:
                        proc.wait(timeout=5)
                    except Exception:
                        proc.kill()
                time.sleep(0.4)
        except KeyboardInterrupt:
            logger.info("Stopping port-forward...")
            return 0
        finally:
            if proc is not None and proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except Exception:
                    proc.kill()

    logger.info("Done.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
