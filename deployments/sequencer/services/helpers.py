import argparse
import re


def argument_parser():
    parser = argparse.ArgumentParser()

    parser.add_argument("--namespace", required=True, type=str, help="Kubernetes namespace.")
    parser.add_argument(
        "--deployment-config-file", required=True, type=str, help="Path to deployment config file."
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
