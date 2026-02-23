#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timedelta, timezone
from typing import List, Optional, Tuple
from zoneinfo import ZoneInfo

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
# Subprocess helpers
# ------------------------------


def run_capture(cmd: List[str]) -> str:
    p = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if p.returncode != 0:
        raise RuntimeError(p.stderr.strip() or "<empty stderr>")
    return p.stdout.strip()


# ------------------------------
# Time utilities
# ------------------------------


def parse_local_timestamp(ts: str) -> datetime:
    """
    Parse timestamp in format YYYY-MM-DDTHH:MM:SS as IST and return datetime in IST.
    """
    ts = ts.strip()
    dt_naive = datetime.fromisoformat(ts)
    israel_tz = ZoneInfo("Asia/Jerusalem")
    dt_ist = dt_naive.replace(tzinfo=israel_tz)
    return dt_ist


def fmt_utc(dt: datetime) -> str:
    return dt.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def ist_to_utc(ist_time: datetime) -> datetime:
    """
    Convert IST (Israel Standard Time) datetime to UTC.
    """
    return ist_time.astimezone(timezone.utc)


def get_24_hours_window() -> Tuple[datetime, datetime]:
    now = datetime.now(timezone.utc)
    start = now - timedelta(hours=24)
    return start, now


# ------------------------------
# Filter builders
# ------------------------------


def common_prefix(ns_re: str) -> str:
    return (
        'resource.type="k8s_container" '
        f'AND resource.labels.namespace_name=~"{ns_re}" '
        'AND (logName:"/logs/stdout" OR logName:"/logs/stderr")'
    )


def consensus_height_filter(common: str, height: int) -> str:
    return f'{common} AND jsonPayload.message:"Running consensus for height {height}"'


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


def first_timestamp(project: str, flt: str) -> str:
    return run_capture(
        [
            "gcloud",
            "logging",
            "read",
            flt,
            "--project",
            project,
            "--format=value(timestamp)",
            "--order=asc",
            "--limit=1",
        ]
    )


def compute_window(
    args: argparse.Namespace,
    environment: Optional[EnvConfig] = None,
    common_filter_prefix: Optional[str] = None,
) -> Tuple[datetime, datetime]:
    """Compute the time window based on provided arguments.

    Priority/validation:
      - --auto, --near, --start/--end, --last-24-hours are mutually exclusive
      - --start/--end must be provided together
      - --last-24-hours (or no args) uses (current_time - 24 hours) to current_time window
      - --auto requires environment and common_filter_prefix parameters
    """

    # Check for conflicts between time options
    options = [args.auto, args.near, (args.start or args.end), args.today]
    if sum(bool(o) for o in options) > 1:
        raise RuntimeError("--auto, --near, --start/--end, and --today are mutually exclusive")

    if args.auto:
        if not environment or not common_filter_prefix:
            raise RuntimeError("--auto requires environment config")
        start_ts = first_timestamp(
            environment.project, consensus_height_filter(common_filter_prefix, args.height)
        )
        if not start_ts:
            raise RuntimeError(
                f"START_MARKER not found: Running consensus for height {args.height}"
            )
        start_dt = parse_local_timestamp(start_ts)

        end_ts = ""
        try:
            end_ts = first_timestamp(
                environment.project, consensus_height_filter(common_filter_prefix, args.height + 1)
            )
        except Exception:
            end_ts = ""

        end_dt = parse_local_timestamp(end_ts) if end_ts else (start_dt + timedelta(minutes=15))
        # Add ±30 seconds buffer
        return start_dt - timedelta(seconds=30), end_dt + timedelta(seconds=30)

    if args.near:
        near_ts = ist_to_utc(parse_local_timestamp(args.near))
        return near_ts - timedelta(hours=2), near_ts + timedelta(hours=2)

    if args.start or args.end:
        if not (args.start and args.end):
            raise RuntimeError("--start and --end must be provided together")
        return ist_to_utc(parse_local_timestamp(args.start)), ist_to_utc(
            parse_local_timestamp(args.end)
        )

    # Default: last 24 hours window
    return get_24_hours_window()


def prepare_filter(args, environment) -> Tuple[str, datetime, datetime]:
    """Prepare log filter and time bounds. Returns (log_filter, start_time, end_time)."""
    common_filter_prefix = common_prefix(environment.namespace_re)
    wide_filter = wide_search_filter(common_filter_prefix, args.height)
    start_time, end_time = compute_window(args, environment, common_filter_prefix)
    log_filter = add_time_bounds(wide_filter, start_time, end_time)

    if args.print_filters:
        print(
            "START_MARKER_FILTER:\n"
            + consensus_height_filter(common_filter_prefix, args.height)
            + "\n"
        )
        print(
            "END_MARKER_FILTER:\n"
            + consensus_height_filter(common_filter_prefix, args.height + 1)
            + "\n"
        )
        print("FINAL_FILTER:\n" + log_filter + "\n")
        raise SystemExit(0)

    return log_filter, start_time, end_time


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
        help="TIMESTAMP - time window start, in local time, (requires --end). TIMESTAMP format: YYYY-MM-DDTHH:MM:SS",
    )
    ap.add_argument("--end", help="TIMESTAMP - time window end, in local time, (requires --start)")
    ap.add_argument("--near", help="TIMESTAMP - search near this time, in local time, (±2h window)")
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
    env_map = load_env_map()
    args = get_args(env_map)

    environment = env_map[args.env]

    try:
        log_filter, start_time, end_time = prepare_filter(args, environment)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        return 2

    # TODO(lev): Add log downloading
    # TODO(lev): Add report generation

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
