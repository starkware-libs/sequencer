#!/usr/bin/env python3
"""
Python wrapper to deploy echonet via Kustomize.

  -x  delete existing resources first (kubectl delete -k)
  -r  rollout restart deployment after apply (to pick up code changes) [default: enabled]
  -R  disable rollout restart
  -n  namespace (kubectl -n <ns>)
  -t  X-Throttling-Bypass token value for feeder requests (sets FEEDER_X_THROTTLING_BYPASS env)
  -s  starting block number (sets START_BLOCK_DEFAULT env)
  -e  resync error threshold (sets RESYNC_ERROR_THRESHOLD env)
  -a  L1 Alchemy API key (sets L1_ALCHEMY_API_KEY env)
  -B  blocked sender addresses (comma-separated) (sets BLOCKED_SENDERS env)
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

import base64
import logging
import shlex
import shutil
import tarfile

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
            "Failed to verify gcloud auth (exit %s): %s", proc.returncode, stderr or "<no stderr>"
        )
        logger.error("Try running: gcloud auth login")
        raise SystemExit(1)

    active = (proc.stdout or "").strip()
    if not active:
        logger.error("No active gcloud account found. Try running: gcloud auth login")
        raise SystemExit(1)

    logger.info("Using gcloud account: %s", active)


def _run(cmd: list[str], **kwargs) -> None:
    """Run a command, echoing it first."""
    logger.info("Running: %s", shlex.join(cmd))
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


def _build_source_bundle(echonet_dir: Path, out_path: Path) -> None:
    """
    Create a base64-encoded gzipped tarball containing the *directory tree* rooted at
    `echonet_dir`, with archive paths prefixed by `echonet/` to preserve imports.
    """
    out_path.parent.mkdir(parents=True, exist_ok=True)

    tmp_tgz = out_path.with_suffix(".tgz.tmp")
    tmp_b64 = out_path.with_suffix(out_path.suffix + ".tmp")
    for p in (tmp_tgz, tmp_b64):
        if p.exists():
            p.unlink()

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

    tmp_tgz.unlink(missing_ok=True)
    tmp_b64.replace(out_path)
    size_kb = out_path.stat().st_size / 1024.0
    print(f"[deploy] Built source bundle: {out_path} ({size_kb:.1f} KiB, base64)")


def _namespace_args(namespace: str | None) -> list[str]:
    return ["-n", namespace] if namespace else []


def main(argv: list[str] | None = None) -> int:
    logging.basicConfig(level=logging.INFO, format="[deploy] %(levelname)s: %(message)s")

    _check_prereqs()
    _check_gcloud_auth()

    parser = argparse.ArgumentParser(
        description="Deploy echonet via kubectl + kustomize.",
        add_help=True,
    )

    # Simple, imperative workflow:
    # - (optional) delete existing resources
    # - generate the source bundle consumed by kustomize
    # - kubectl apply -k
    # - (optional) kubectl set env
    # - (optional) rollout restart + wait
    parser.add_argument(
        "-x",
        dest="delete_first",
        action="store_true",
        help="Delete existing resources first (kubectl delete -k).",
    )

    # Rollout restart control:
    parser.add_argument(
        "-r",
        "--rollout-restart",
        dest="roll_restart",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="Rollout restart deployment after apply (default: enabled).",
    )

    parser.add_argument(
        "-n",
        dest="namespace",
        metavar="NAMESPACE",
        help="Kubernetes namespace to target (kubectl -n <ns>).",
    )

    # Backwards-compat: translate legacy `-R` into the BooleanOptionalAction negative.
    if argv is None:
        argv = sys.argv[1:]
    argv = ["--no-rollout-restart" if a == "-R" else a for a in argv]

    args = parser.parse_args(argv)

    # Paths
    script_dir = Path(__file__).resolve().parent
    kustomize_dir = script_dir / "k8s" / "echonet"

    if not kustomize_dir.is_dir():
        logger.error("Kustomize directory not found at %s", kustomize_dir)
        return 1

    # Build a configmap-friendly bundle of the echonet/ tree so imports work in the pod.
    generated_dir = kustomize_dir / "generated"
    bundle_path = generated_dir / "echonet-src.tgz.b64"
    _build_source_bundle(echonet_dir=script_dir, out_path=bundle_path)

    namespace_args = _namespace_args(args.namespace)

    # 1. Optional delete
    if args.delete_first:
        logger.info("Deleting existing resources...")
        _run(
            ["kubectl", *namespace_args, "delete", "-k", str(kustomize_dir), "--ignore-not-found"],
        )

    # 2. Apply manifests
    logger.info("Applying manifests...")
    _run(["kubectl", *namespace_args, "apply", "-k", str(kustomize_dir)])
    # 3. Optional rollout restart
    if args.roll_restart:
        logger.info("Rolling restart deployment/echonet...")
        _run(["kubectl", *namespace_args, "rollout", "restart", "deployment/echonet"])
        _run(["kubectl", *namespace_args, "rollout", "status", "deployment/echonet"])

    logger.info("Done.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
