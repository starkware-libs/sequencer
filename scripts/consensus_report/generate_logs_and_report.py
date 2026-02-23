#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import sys
from dataclasses import dataclass

import yaml


@dataclass(frozen=True)
class EnvConfig:
    project: str
    namespace_re: str


def load_env_map() -> dict[str, EnvConfig]:
    """Load environment map from yaml file and return env_map."""
    script_dir = os.path.dirname(os.path.abspath(__file__))
    yaml_path = os.path.join(script_dir, "env_map.yaml")

    if not os.path.exists(yaml_path):
        print(f"Error: Environment config file not found: {yaml_path}", file=sys.stderr)
        sys.exit(1)

    with open(yaml_path, "r", encoding="utf-8") as f:
        data = yaml.safe_load(f)

    if data is None:
        print(f"Error: Environment config file is empty", file=sys.stderr)
        sys.exit(1)

    environments = data.get("environments")
    if environments is None:
        print(
            f"Error: Bad environment config file, missing environments definition", file=sys.stderr
        )
        sys.exit(1)

    env_map = {name: EnvConfig(**cfg) for name, cfg in environments.items()}

    return env_map


# ------------------------------
# Main
# ------------------------------


def get_args(env_map: dict[str, EnvConfig]) -> argparse.Namespace:
    ap = argparse.ArgumentParser(formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument(
        "--env",
        choices=env_map.keys(),
        required=True,
        help=f"One of the environments defined in the env_map.yaml file)",
    )
    ap.add_argument("--height", required=True, type=int, help="Block height, e.g. 6591090")
    ap.add_argument(
        "--out_json_path",
        help="Output file path for JSON. Extension .json added if missing. Omit to print to stdout.",
    )
    ap.add_argument(
        "--auto",
        action="store_true",
        help="Auto-detect time window by searching for 'Running consensus for height N' markers",
    )
    ap.add_argument(
        "--start",
        help="TIMESTAMP - time window start (requires --end). TIMESTAMP format: YYYY-MM-DDTHH:MM:SSZ",
    )
    ap.add_argument("--end", help="TIMESTAMP - time window end (requires --start)")
    ap.add_argument("--near", help="TIMESTAMP - search near this time (±2h window)")
    ap.add_argument(
        "--last-24-hours",
        action="store_true",
        help="Use last 24 hours time window (default if no time args)",
    )
    ap.add_argument(
        "--print-filters",
        action="store_true",
        help="Print START_MARKER, END_MARKER, and WIDE_SEARCH filters and exit.",
    )
    ap.add_argument(
        "--report_path",
        help="Generated report will be saved into this file. Extension .txt added if missing.",
    )

    return ap.parse_args()


def main() -> int:
    env_map, default_env = load_env_map()
    args = get_args(env_map, default_env)

    env_map[args.env]

    # TODO(lev): Add filter preparation
    # TODO(lev): Add log downloading
    # TODO(lev): Add report generation

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
