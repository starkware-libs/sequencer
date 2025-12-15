#!/usr/bin/env python3
import argparse
import dataclasses
import json
import re
from typing import Optional

import collections
import logging


# --- Helper Functions ---
def init_logging(verbose: bool):
    """Sets up the logging configuration."""
    logging.basicConfig(
        level=logging.DEBUG if verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )


def run_kubectl(args) -> str:
    """Executes kubectl commands safely."""
    cmd = ["kubectl"] + args
    try:
        result = subprocess.run(
            cmd,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        return result.stdout
    except subprocess.CalledProcessError as e:
        print(f"Error running {' '.join(cmd)}:\n{e.stderr}", file=sys.stderr)
        return ""


class KubeNamespace:
    """Handles fetching pod info within a specific namespace."""

    def __init__(self, context: str, namespace: str, pods_regex: str = None):
        self.context = context
        self.namespace = namespace
        self.pods_re = re.compile(pods_regex) if pods_regex else None
        self.pods = {}
        self.logger = logging.getLogger(f"KubeNamespace:{context}:{namespace}")

    def collect_pods_info(self):
        out = run_kubectl(
            ["--context", self.context, "-n", self.namespace, "get", "pods", "-o", "json"]
        )
        if not out:
            return

        data = json.loads(out)
        for pod in data.get("items", []):
            pod_name = pod["metadata"]["name"]
            # Make sure pod name matches regex if provided
            if self.pods_re and not self.pods_re.search(pod_name):
                continue

            self.logger.debug(f"Collecting pod {pod_name}")
            try:
                spec = pod.get("spec", {})
                container = spec.get("containers")[0]
                image = container["image"].split(":")[1]  # Strip registry prefix
                self.pods[container["name"]] = image
            except (IndexError, KeyError):
                self.logger.warning(f"Skipping malformed pod {pod_name}")


class KubeContext:
    """Handles discovery of namespaces within a context."""

    def __init__(self, context, namespaces_regex=None, pods_regex=None):
        self.context = context
        self.namespaces_re = re.compile(namespaces_regex) if namespaces_regex else None
        self.pods_re = pods_regex
        self.namespaces = []
        self.logger = logging.getLogger(f"KubeContext:{context}")

    def collect_namespaces(self):
        out = run_kubectl(["--context", self.context, "get", "ns", "-o", "json"])
        if not out:
            return

        data = json.loads(out)
        for item in data.get("items", []):
            ns_name = item["metadata"]["name"]
            if self.namespaces_re and not self.namespaces_re.search(ns_name):
                continue

            self.logger.debug(f"Collecting namespace {ns_name}")
            kube_ns = KubeNamespace(self.context, ns_name, self.pods_re)
            kube_ns.collect_pods_info()
            self.namespaces.append(kube_ns)


@dataclasses.dataclass(frozen=True)
class EnvInfo:
    desc: str
    cluster_re: str
    namespace_re: str
    pod_re: Optional[str]


class Runner:
    ENVS = {
        "Mainnet": EnvInfo(
            desc="Starknet Mainnet DEC",
            cluster_re=r"gke_starkware-prod_us-.*_starknet-mainnet-apollo-.*",
            namespace_re=r"apollo-mainnet-(.*)",
            pod_re=r"sequencer-(.+?)-.*",
        ),
        "Testnet": EnvInfo(
            desc="Starknet Testnet DEC",
            cluster_re=r"gke_starkware-starknet-testnet_us-.*_starknet-testnet-apollo-.*",
            namespace_re=r"apollo-sepolia-alpha-(.*)",
            pod_re=r"sequencer-(.+?)-.*",
        ),
        "Integration": EnvInfo(
            desc="Starknet Integration DEC",
            cluster_re="gke_starkware-starknet-testnet_us-central1_starknet-testnet",
            namespace_re=r"apollo-sepolia-integration-(.*)",
            pod_re=r"sequencer-(.+?)-.*",
        ),
        "POTC_Mainet": EnvInfo(
            desc="POTC Mainnet DEC",
            cluster_re=r"gke_starkware-prod_asia-.*_paradigm-mainnet-apollo-.*",
            namespace_re=r"apollo-potc-mainnet-(.*)",
            pod_re=r"sequencer-(.+?)-.*",
        ),
        "POTC_Testnet": EnvInfo(
            desc="POTC Testnet DEC",
            cluster_re=r"gke_starkware-integ-cust-tokyo_asia-.*_paradigm-testnet-apollo-.*",
            namespace_re=r"apollo-potc-testnet-(.*)",
            pod_re=r"sequencer-(.+?)-.*",
        ),
        "POTC_Integration": EnvInfo(
            desc="POTC Integration DEC",
            cluster_re="gke_starkware-integ-cust-tokyo_asia-northeast1_integ-cust-tokyo",
            namespace_re=r"apollo-potc-mock-(.*)",
            pod_re=r"sequencer-(.+?)-.*",
        ),
    }

    def __init__(self, args):
        self.args = args
        # Select specific envs if provided, otherwise default to all
        self.active_envs = (
            [self.ENVS[env] for env in args.env] if args.env else list(self.ENVS.values())
        )
        self.contexts = collections.defaultdict(list)
        self.logger = logging.getLogger("Runner")


# --- Main Entry Point ---
def main():
    parser = argparse.ArgumentParser(
        description="List pod container images across contexts and namespaces."
    )
    env_choices = list(Runner.ENVS.keys())
    parser.add_argument(
        "--env",
        nargs="+",
        action="extend",
        default=[],
        choices=env_choices,
        metavar="ENV",
        help=f"Optional list of environments ({','.join(env_choices)}). If omitted, all environments are used.",
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Enable verbose logging.",
    )

    args = parser.parse_args()
    init_logging(args.verbose)

    # Initialize runner to verify config works
    runner = Runner(args)
    logging.debug(f"Runner initialized for {len(runner.active_envs)} environments")


if __name__ == "__main__":
    main()
