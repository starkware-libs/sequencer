import argparse
import hashlib
import random
import re
from typing import Optional

import string


def argument_parser() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--cluster", required=False, type=str, help="Provide the cluster name.")
    parser.add_argument("--namespace", required=True, type=str, help="Kubernetes namespace.")
    parser.add_argument(
        "--deployment-config-file", required=True, type=str, help="Path to deployment config file."
    )
    parser.add_argument(
        "--monitoring-dashboard-file",
        required=False,
        type=str,
        help="Path to Grafana dashboard JSON file.",
    )

    image_group = parser.add_mutually_exclusive_group()
    image_group.add_argument(
        "--deployment-image-tag",
        required=False,
        type=str,
        help="Apollo node image tag to fetch from GHCR (default: 'dev')",
    )
    image_group.add_argument(
        "--deployment-image",
        required=False,
        type=str,
        help="Full Docker image to use instead of default GHCR tag",
    )

    parser.add_argument(
        "--monitoring-alerts-folder",
        required=False,
        type=str,
        help="Path to Grafana alerts folder.",
    )

    return parser.parse_args()


def sanitize_name(name: str) -> str:
    """
    Sanitize a Kubernetes resource name to comply with k8s naming conventions:
    - Only lowercase letters (a-z), numbers (0-9), and hyphens (-) are allowed.
    - The name must start and end with a letter or number.
    - The name must not exceed 253 characters.
    - Underscores (_) are replaced with hyphens (-), and invalid characters are removed.

    Args:
        name (str): The original name to be sanitized.

    Returns:
        str: The sanitized name.
    """

    name = name.lower()
    name = name.replace("_", "-")
    name = re.sub(r"[^a-z0-9-]", "", name)
    name = name.strip("-")
    name = name[:253]

    return name


def generate_random_hash(length: int = 6, from_string: Optional[str] = None) -> str:
    if from_string:
        hash_object = hashlib.sha256(from_string.encode())
        return hash_object.hexdigest()[:length]
    else:
        return "".join(random.choices(string.ascii_letters, k=length))
