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
from typing import List, Tuple

import yaml

# Constants
RUNNING_CONSENSUS_FOR_HEIGHT = "Running consensus for height"


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


def run_stream(cmd: List[str], output_path: Optional[str] = None) -> int:
    """Execute command, optionally writing stdout to a file."""
    with ExitStack() as stack:
        out_file = (
            stack.enter_context(open(output_path, "w", encoding="utf-8")) if output_path else None
        )
        p = subprocess.run(cmd, stdout=out_file, stderr=subprocess.PIPE, text=True)

    if p.returncode == 0 and output_path:
        print(f"Output logs written to {output_path}")
    elif p.returncode != 0 and p.stderr:
        print(p.stderr.strip(), file=sys.stderr)
    return p.returncode


# ------------------------------
# Time utilities
# ------------------------------


def parse_local_timestamp(ts: str) -> datetime:
    """
    Parse timestamp in format YYYY-MM-DDTHH:MM:SS and returns datetime in local timezone.
    """
    ts = ts.strip()
    dt_without_tz = datetime.fromisoformat(ts)
    dt_with_tz = dt_without_tz.astimezone()
    return dt_with_tz


def parse_rfc3339(ts: str) -> datetime:
    """
    Parse RFC3339 timestamp string to datetime.
    """
    ts = ts.strip()

    # Handle fractional seconds if present
    if "." in ts and ts.endswith("Z"):
        head, rest = ts.split(".", 1)
        # Extract fractional seconds (up to 6 digits for microseconds)
        frac = rest.rstrip("Z")[:6].ljust(6, "0")
        ts = f"{head}.{frac}Z"

    # Replace 'Z' with '+00:00' for fromisoformat
    ts = ts.replace("Z", "+00:00")

    return datetime.fromisoformat(ts)


def fmt_utc(dt: datetime) -> str:
    return dt.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def to_utc(local_time: datetime) -> datetime:
    """
    Convert local datetime to UTC.
    """
    return local_time.astimezone(timezone.utc)


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
    return f'{common} AND jsonPayload.message:"{RUNNING_CONSENSUS_FOR_HEIGHT} {height}"'


def wide_search_filter(common: str, height: int) -> str:
    return f"""{common} AND (
      (
        -- Consensus logs (keyed by height)
        (
          jsonPayload.spans.height="{height}"
          OR jsonPayload.message:"{height}"
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
    return f'{flt} AND timestamp>="{fmt_utc(start)}" AND timestamp<="{fmt_utc(end)}"'


# ------------------------------
# Timestamp discovery + windowing
# ------------------------------


def retrieve_first_timestamp(project: str, flt: str) -> str:
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


def determine_search_window(
    args: argparse.Namespace,
    environment: EnvConfig,
    common_filter_prefix: str,
) -> Tuple[datetime, datetime]:
    """Determine the search time window based on provided arguments.

    Priority/validation:
      - --auto, --near, --range, --last-24-hours are mutually exclusive
      - --range requires exactly 2 arguments: start and end timestamps
      - --last-24-hours (or no args) uses (current_time - 24 hours) to current_time window
      - --auto requires environment and common_filter_prefix parameters
    """

    if args.auto:
        start_ts = retrieve_first_timestamp(
            environment.project, consensus_height_filter(common_filter_prefix, args.height)
        )
        if not start_ts:
            raise RuntimeError(
                f"START_MARKER not found: {RUNNING_CONSENSUS_FOR_HEIGHT} {args.height}"
            )
        start_dt = parse_rfc3339(start_ts)

        end_ts = ""
        try:
            end_ts = retrieve_first_timestamp(
                environment.project, consensus_height_filter(common_filter_prefix, args.height + 1)
            )
        except Exception:
            end_ts = ""

        end_dt = parse_rfc3339(end_ts) if end_ts else (start_dt + timedelta(minutes=15))
        # Add ±30 seconds buffer
        return start_dt - timedelta(seconds=30), end_dt + timedelta(seconds=30)

    if args.near:
        near_ts = to_utc(parse_local_timestamp(args.near))
        return near_ts - timedelta(hours=2), near_ts + timedelta(hours=2)

    if args.range:
        start_str, end_str = args.range
        return to_utc(parse_local_timestamp(start_str)), to_utc(parse_local_timestamp(end_str))

    # Default: last 24 hours window
    return get_24_hours_window()


def prepare_filter(args, environment) -> Tuple[str, datetime, datetime]:
    """Prepare log filter and time bounds. Returns (log_filter, start_time, end_time)."""
    common_filter_prefix = common_prefix(environment.namespace_re)
    wide_filter = wide_search_filter(common_filter_prefix, args.height)
    start_time, end_time = determine_search_window(args, environment, common_filter_prefix)
    log_filter = add_time_bounds(wide_filter, start_time, end_time)

    if args.print_filters:
        if args.auto:
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


def download_logs(
    args,
    environment,
    log_filter: str,
    start_time: str,
    end_time: str,
) -> Tuple[Optional[str], bool]:
    """Download logs from GCP. Returns (logs_path, is_temp_file) or (None, False) on error."""
    print(
        f"Downloading logs for height {args.height} from {start_time} to {end_time} from {environment.project}"
    )

    cmd = [
        "gcloud",
        "logging",
        "read",
        log_filter,
        "--project",
        environment.project,
        "--format=json",
        "--order=asc",
        "--limit=500000",
    ]

    output_path = args.out_json_path
    if output_path and not os.path.splitext(output_path)[1]:
        output_path = output_path + ".json"

    temp_file = False
    if args.report_path and not output_path:
        temp_logs_file = tempfile.NamedTemporaryFile(
            mode="w", suffix=".json", delete=False, encoding="utf-8"
        )
        output_path = temp_logs_file.name
        temp_logs_file.close()
        temp_file = True

    rc = run_stream(cmd, output_path)
    if rc != 0:
        if temp_file:
            os.unlink(output_path)
        return None, False

    return output_path if output_path else "stdout", temp_file


def generate_report(logs_path: str, height: int, report_output: str) -> int:
    """Generate consensus report from logs. Returns exit code."""
    # TODO(lev): Implement report generation
    print("Report generation not yet implemented")
    return 0


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

    # Create mutually exclusive group for time options
    time_group = ap.add_mutually_exclusive_group()
    time_group.add_argument(
        "--auto",
        action="store_true",
        help=f"Auto-detect time window by searching for '{RUNNING_CONSENSUS_FOR_HEIGHT} N' markers",
    )
    time_group.add_argument(
        "--range",
        nargs=2,
        metavar=("START", "END"),
        help="Time window range in local time. Format: YYYY-MM-DDTHH:MM:SS YYYY-MM-DDTHH:MM:SS",
    )
    time_group.add_argument(
        "--near",
        metavar="TIMESTAMP",
        help="Search near this time in local time (±2h window). Format: YYYY-MM-DDTHH:MM:SS",
    )
    time_group.add_argument(
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

    logs_path, temp_file = download_logs(
        args, environment, log_filter, fmt_utc(start_time), fmt_utc(end_time)
    )
    if logs_path is None:
        return 1
    if logs_path == "stdout":
        return 0

    rc = 0
    if args.report_path:
        rc = generate_report(logs_path, args.height, args.report_path)

    if temp_file:
        os.unlink(logs_path)

    return rc


if __name__ == "__main__":
    raise SystemExit(main())
