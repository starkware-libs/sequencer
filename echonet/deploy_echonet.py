#!/usr/bin/env python3
"""
Python wrapper to deploy echonet via Kustomize.

  -x  delete existing resources first (kubectl delete -k)
  -l  allow files outside kustomize dir (use --load-restrictor=LoadRestrictionsNone) [default: enabled]
  -L  disable allowing files outside kustomize dir
  -r  rollout restart deployment after apply (to pick up code changes) [default: enabled]
  -R  disable rollout restart
  -n  namespace (kubectl -n <ns>)
  -t  X-Throttling-Bypass token value for feeder requests (sets FEEDER_X_THROTTLING_BYPASS env)
  -s  starting block number (sets START_BLOCK_DEFAULT env)
  -e  resync error threshold (sets RESYNC_ERROR_THRESHOLD env)
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path
from typing import List

import shutil


def _check_prereqs() -> None:
    """Ensure `kubectl` is available on PATH."""
    if shutil.which("kubectl") is None:
        print(
            "Error: `kubectl` not found on PATH. Please install kubectl and try again.",
            file=sys.stderr,
        )
        sys.exit(1)


def _run(cmd: List[str], **kwargs) -> None:
    """Run a command, echoing it first."""
    print(f"[deploy] Running: {' '.join(cmd)}")
    subprocess.run(cmd, check=True, **kwargs)


def main(argv: list[str] | None = None) -> int:
    _check_prereqs()

    parser = argparse.ArgumentParser(
        description="Deploy echonet via kubectl + kustomize (Python port of deploy-echonet.sh).",
        add_help=True,
    )

    # Flags that mirror the shell script defaults/behavior
    parser.add_argument(
        "-x",
        dest="delete_first",
        action="store_true",
        help="Delete existing resources first (kubectl delete -k).",
    )

    # Allow / disallow files outside kustomize dir
    parser.set_defaults(allow_outside=True)
    parser.add_argument(
        "-l",
        dest="allow_outside",
        action="store_true",
        help="Allow files outside kustomize dir (default).",
    )
    parser.add_argument(
        "-L",
        dest="allow_outside",
        action="store_false",
        help="Disable allowing files outside kustomize dir.",
    )

    # Rollout restart control
    parser.set_defaults(roll_restart=True)
    parser.add_argument(
        "-r",
        dest="roll_restart",
        action="store_true",
        help="Rollout restart deployment after apply (default).",
    )
    parser.add_argument(
        "-R",
        dest="roll_restart",
        action="store_false",
        help="Disable rollout restart after apply.",
    )

    parser.add_argument(
        "-n",
        dest="namespace",
        metavar="NAMESPACE",
        help="Kubernetes namespace to target (kubectl -n <ns>).",
    )
    parser.add_argument(
        "-t",
        dest="feeder_x_throttling_bypass",
        metavar="TOKEN",
        help="X-Throttling-Bypass token value for feeder requests (FEEDER_X_THROTTLING_BYPASS env).",
    )
    parser.add_argument(
        "-s",
        dest="start_block_default",
        metavar="BLOCK",
        help="Starting block number (START_BLOCK_DEFAULT env).",
    )
    parser.add_argument(
        "-e",
        dest="resync_error_threshold",
        metavar="COUNT",
        help="Resync error threshold (RESYNC_ERROR_THRESHOLD env).",
    )

    args = parser.parse_args(argv)

    # Mirror SCRIPT_DIR / KUSTOMIZE_DIR in the shell script
    script_dir = Path(__file__).resolve().parent
    kustomize_dir = script_dir / "k8s" / "echonet"

    if not kustomize_dir.is_dir():
        print(f"Error: Kustomize directory not found at {kustomize_dir}", file=sys.stderr)
        return 1

    namespace_args: list[str] = []
    if args.namespace:
        namespace_args = ["-n", args.namespace]

    # 1. Optional delete
    if args.delete_first:
        print("[deploy] Deleting existing resources...")
        _run(
            ["kubectl", *namespace_args, "delete", "-k", str(kustomize_dir), "--ignore-not-found"],
        )

    # 2. Apply manifests
    print("[deploy] Applying manifests...")
    if args.allow_outside:
        # Equivalent to: kubectl kustomize ... --load-restrictor=LoadRestrictionsNone | kubectl apply -f -
        # Do it without a shell pipe for better error handling.
        kustomize_cmd = [
            "kubectl",
            "kustomize",
            str(kustomize_dir),
            "--load-restrictor=LoadRestrictionsNone",
        ]
        print(
            f"[deploy] Running: {' '.join(kustomize_cmd)} | kubectl {' '.join(namespace_args)} apply -f -"
        )
        kustomize_proc = subprocess.run(
            kustomize_cmd,
            check=True,
            capture_output=True,
        )
        apply_cmd = ["kubectl", *namespace_args, "apply", "-f", "-"]
        subprocess.run(apply_cmd, check=True, input=kustomize_proc.stdout)
    else:
        _run(
            ["kubectl", *namespace_args, "apply", "-k", str(kustomize_dir)],
        )

    # 3. Optional env vars
    env_args: list[str] = []
    if args.feeder_x_throttling_bypass:
        env_args.append(f"FEEDER_X_THROTTLING_BYPASS={args.feeder_x_throttling_bypass}")
    if args.start_block_default:
        env_args.append(f"START_BLOCK_DEFAULT={args.start_block_default}")
    if args.resync_error_threshold:
        env_args.append(f"RESYNC_ERROR_THRESHOLD={args.resync_error_threshold}")

    if env_args:
        print(f"[deploy] Setting environment variables on deployment/echonet: {' '.join(env_args)}")
        _run(
            ["kubectl", *namespace_args, "set", "env", "deployment/echonet", *env_args],
        )

    # 4. Optional rollout restart
    if args.roll_restart:
        print("[deploy] Rolling restart deployment/echonet...")
        _run(["kubectl", *namespace_args, "rollout", "restart", "deployment/echonet"])
        _run(["kubectl", *namespace_args, "rollout", "status", "deployment/echonet"])

    print("[deploy] Done.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
