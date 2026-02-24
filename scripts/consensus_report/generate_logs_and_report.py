#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
from contextlib import ExitStack
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from typing import List, Optional, Tuple

import yaml


@dataclass(frozen=True)
class EnvConfig:
    project: str
    namespace_re: str


def load_env_map() -> Tuple[dict[str, EnvConfig], str]:
    """Load environment map and return (env_map, default_environment)."""
    script_dir = os.path.dirname(os.path.abspath(__file__))
    yaml_path = os.path.join(script_dir, "env_map.yaml")

    if not os.path.exists(yaml_path):
        print(f"Error: Environment config file not found: {yaml_path}", file=sys.stderr)
        sys.exit(1)

    with open(yaml_path, "r", encoding="utf-8") as f:
        data = yaml.safe_load(f)

    environments = data.get("environments", {})
    default_env = data.get("default_environment", "integration")
    env_map = {name: EnvConfig(**cfg) for name, cfg in environments.items()}

    return env_map, default_env


# ------------------------------
# Time utilities
# ------------------------------


def fmt_utc(dt: datetime) -> str:
    return dt.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def utc_midnight_window(now_utc: Optional[datetime] = None) -> Tuple[datetime, datetime]:
    now = now_utc or datetime.now(timezone.utc)
    start = now.replace(hour=0, minute=0, second=0, microsecond=0)
    return start, start + timedelta(days=1)


# ------------------------------
# Filter builders
# ------------------------------


def common_prefix(ns_re: str) -> str:
    return (
        'resource.type="k8s_container" '
        f'AND resource.labels.namespace_name=~"{ns_re}" '
        'AND (logName:"/logs/stdout" OR logName:"/logs/stderr")'
    )


def start_marker_filter(common: str, height: int) -> str:
    return f'{common} AND jsonPayload.message:"Running consensus for height {height}"'


def end_marker_filter(common: str, next_height: int) -> str:
    return f'{common} AND jsonPayload.message:"Running consensus for height {next_height}"'


def wide_search_filter(common: str, height: int) -> str:
    return f"""{common} AND (
      (
        -- Consensus logs (keyed by height)
        (
          jsonPayload.spans.height="{height}"
          OR jsonPayload.message:"{height}"
          OR textPayload:"{height}"
        )
        AND
        resource.labels.container_name="sequencer-core"
        AND
        (
          jsonPayload.message=~"^START_ROUND_(PROPOSER|VALIDATOR):"
          OR jsonPayload.message:"DECISION_REACHED"
          OR jsonPayload.message:"PROPOSAL_FAILED"
          OR jsonPayload.message=~"(?i)prevote|precommit|propose"

        )
      )
      OR
      (
        -- Batcher logs (keyed by propose_block_input)
        jsonPayload.filename:"apollo_batcher" AND "BlockNumber({height})"
        AND
        (
          jsonPayload.message:"finishing block building"
          OR jsonPayload.message:"Received final number of transactions in block proposal:"
          OR jsonPayload.message:"Finished building block as proposer"
        )
      )
      OR
      (
        -- Blockifier logs
        jsonPayload.filename:"blockifier"
        AND
        jsonPayload.message:"Block {height} final weights"
      )
    )"""


def add_time_bounds(flt: str, start: datetime, end: datetime) -> str:
    return f'{flt} AND timestamp>="{fmt_utc(start)}" AND timestamp<"{fmt_utc(end)}"'


# ------------------------------
# Timestamp discovery + windowing
# ------------------------------


def compute_window(
    args: argparse.Namespace,
    environment: Optional[EnvConfig] = None,
    common_prefix: Optional[str] = None,
) -> Tuple[datetime, datetime]:
    """Compute the time window based on provided arguments.

    Priority/validation:
      - --auto, --near, --start/--end, --today are mutually exclusive
      - --start/--end must be provided together
      - --today (or no args) uses today's UTC midnight-to-midnight
      - --auto requires environment and common_prefix parameters
    """

    # TODO: Implement all the logic for the different time options

    # Default: today's window
    return utc_midnight_window()


def prepare_filter(args, environment) -> Tuple[str, str, str]:
    """Prepare log filter and time bounds. Returns (log_filter, start_time, end_time)."""
    common_filter_prefix = common_prefix(environment.namespace_re)
    wide_filter = wide_search_filter(common_filter_prefix, args.height)
    start_time, end_time = compute_window(args, environment, common_filter_prefix)
    log_filter = add_time_bounds(wide_filter, start_time, end_time)

    if args.print_filters:
        print(
            "START_MARKER_FILTER:\n" + start_marker_filter(common_filter_prefix, args.height) + "\n"
        )
        print(
            "END_MARKER_FILTER:\n" + end_marker_filter(common_filter_prefix, args.height + 1) + "\n"
        )
        print("FINAL_FILTER:\n" + log_filter + "\n")
        raise SystemExit(0)

    return log_filter, start_time, end_time


# ------------------------------
# Main
# ------------------------------


def get_args(env_map: dict[str, EnvConfig], default_env: str) -> argparse.Namespace:
    ap = argparse.ArgumentParser(formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument(
        "--env",
        choices=env_map.keys(),
        default=default_env,
        help=f"Environment (default: {default_env})",
    )
    ap.add_argument("--height", required=True, type=int, help="Block height, e.g. 6591090")
    ap.add_argument(
        "--out_json",
        help="Output file path for JSON. Extension .json added if missing. Omit to print to stdout.",
    )
    ap.add_argument(
        "--auto",
        action="store_true",
        help="Auto-detect time window by searching for 'Running consensus for height N' markers",
    )
    ap.add_argument("--start", help="RFC3339 timestamp - time window start (requires --end)")
    ap.add_argument("--end", help="RFC3339 timestamp - time window end (requires --start)")
    ap.add_argument("--near", help="RFC3339 timestamp - search near this time (±2h window)")
    ap.add_argument(
        "--today",
        action="store_true",
        help="Use today's UTC midnight-to-midnight time window (default if no time args)",
    )
    ap.add_argument(
        "--print-filters",
        action="store_true",
        help="Print START_MARKER, END_MARKER, and WIDE_SEARCH filters and exit.",
    )
    ap.add_argument(
        "--report",
        help="Generate report to this file (requires --out_json). Extension .txt added if missing.",
    )

    return ap.parse_args()


def main() -> int:
    env_map, default_env = load_env_map()
    args = get_args(env_map, default_env)

    environment = env_map[args.env]

    try:
        log_filter, start_time, end_time = prepare_filter(args, environment)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        return 2

    # TODO: Add log downloading
    # TODO: Add report generation

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
