#!/usr/bin/env python3
import argparse
import json
import re
import subprocess
import sys

import logging


# --- Helper Functions ---
def init_logging(verbose: bool):
    """Sets up the logging configuration."""
    logging.basicConfig(
        level=logging.DEBUG if verbose else logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )


def run_kubectl(args: list[str]) -> str:
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

    def __init__(self, context: str, namespaces_regex: str = None, pods_regex: str = None):
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


# --- Main Entry Point ---
def main():
    parser = argparse.ArgumentParser(
        description="List pod container images across contexts and namespaces."
    )
    parser.add_argument(
        "--verbose",
        action="store_true",
        help="Enable verbose logging.",
    )

    args = parser.parse_args()
    init_logging(args.verbose)

    logging.info("Script initialized. No logic implemented yet.")


if __name__ == "__main__":
    main()
