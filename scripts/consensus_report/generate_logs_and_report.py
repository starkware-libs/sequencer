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
from generate_consensus_report import generate_consensus_report

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
        env_file_format = """
    environments:
    env1:
        project: "GCP project1"
        namespace_re: "namespace1"
    env2:
        project: "GCP project2"
        namespace_re: "namespace2"
    """

        print(
            f"Error: Missing environment config file at: {yaml_path}.\nExpected format:{env_file_format} ",
            file=sys.stderr,
        )
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
            stack.enter_context(open(output_path, "w", encoding="utf-8"))
            if output_path is not None
            else None
        )
        p = subprocess.run(cmd, stdout=out_file, stderr=subprocess.PIPE, text=True)

    if p.returncode != 0 and p.stderr is not None:
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


def parse_hours_minutes(duration_str: str) -> timedelta:
    """Parse HH:MM format and return timedelta. Raises ValueError if invalid format."""
    parts = duration_str.split(":")
    if len(parts) != 2:
        raise ValueError(f"Invalid duration format '{duration_str}'. Expected HH:MM")

    try:
        hours = int(parts[0])
        minutes = int(parts[1])
    except ValueError:
        raise ValueError(
            f"Invalid duration format '{duration_str}'. Hours and minutes must be integers"
        )

    if hours < 0 or minutes < 0 or minutes >= 60:
        raise ValueError(
            f"Invalid duration '{duration_str}'. Hours must be >= 0, minutes must be 0-59"
        )

    return timedelta(hours=hours, minutes=minutes)


def get_last_duration_window(duration_str: str) -> Tuple[datetime, datetime]:
    """Get time window from (now - duration) to now."""
    duration = parse_hours_minutes(duration_str)
    now = datetime.now(timezone.utc)
    start = now - duration
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
          OR jsonPayload.message=~"\\D{height}\\D"
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


def build_gcloud_logging_cmd(
    project: str,
    log_filter: str,
    format_type: str,
    limit: int,
) -> list[str]:
    """
    Build gcloud logging read command.
    """
    return [
        "gcloud",
        "logging",
        "read",
        log_filter,
        "--project",
        project,
        f"--format={format_type}",
        "--order=asc",
        f"--limit={limit}",
    ]


def retrieve_first_timestamp(project: str, flt: str) -> str:
    return run_capture(
        build_gcloud_logging_cmd(
            project=project,
            log_filter=flt,
            format_type="value(timestamp)",
            limit=1,
        )
    )


def determine_search_window(
    args: argparse.Namespace,
    environment: EnvConfig,
    common_filter_prefix: str,
) -> Tuple[datetime, datetime]:
    """Determine the search time window based on provided arguments.

    Priority/validation:
      - --auto, --near, --range, --last are mutually exclusive
      - --range requires exactly 2 arguments: start and end timestamps
      - --last HH:MM (or no args, defaults to 24:00) uses (current_time - duration) to current_time window
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

    # Default: last 24 hours window, or custom duration if --last is provided
    if args.last:
        return get_last_duration_window(args.last)
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


def resolve_output_path(args) -> Tuple[Optional[str], bool]:
    output_path = args.out_json_path
    if output_path is not None and os.path.splitext(output_path)[1] == "":
        output_path = output_path + ".json"

    temp_file = False
    if args.report_path is not None and output_path is None:
        temp_logs_file = tempfile.NamedTemporaryFile(
            mode="w", suffix=".json", delete=False, encoding="utf-8"
        )
        output_path = temp_logs_file.name
        temp_logs_file.close()
        temp_file = True

    return output_path, temp_file


def download_logs(environment, log_filter: str, output_path: Optional[str]) -> int:
    """Download logs from GCP. Returns (logs_path, is_temp_file) or (None, False) on error."""
    cmd = build_gcloud_logging_cmd(
        project=environment.project,
        log_filter=log_filter,
        format_type="json",
        limit=500000,
    )

    rc = run_stream(cmd, output_path)
    if rc != 0:
        print(f"Error: Failed to download logs: {rc}", file=sys.stderr)

    return rc


def generate_report(logs_path: str, height: int, report_path: str) -> int:
    """Generate consensus report from logs. Returns exit code."""
    if os.path.splitext(report_path)[1] == "":
        report_path = report_path + ".txt"

    print("Generating report...")
    return generate_consensus_report(logs_path, str(height), report_path)


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
        "--last",
        metavar="HOURS:MINUTES",
        help="Search last HOURS:MINUTES time window (default: 24:00 if no time args). Format: HH:MM",
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

    args = ap.parse_args()

    # Validate that paths are not empty or whitespace-only
    if args.out_json_path is not None and args.out_json_path.strip() == "":
        ap.error("--out_json_path cannot be empty or contain only whitespace")

    if args.report_path is not None and args.report_path.strip() == "":
        ap.error("--report_path cannot be empty or contain only whitespace")

    return args


def main() -> int:
    env_map = load_env_map()
    args = get_args(env_map)

    environment = env_map[args.env]

    try:
        log_filter, start_time, end_time = prepare_filter(args, environment)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        return 2

    output_path, temp_file = resolve_output_path(args)

    print(
        f"Downloading logs for height {args.height} from {fmt_utc(start_time)}"
        f" to {fmt_utc(end_time)} from {environment.project}",
        file=sys.stderr,
    )
    rc = download_logs(environment, log_filter, output_path)
    if rc == 0:
        if output_path is None:
            return 0

        if not temp_file:
            print(f"Output logs written to {output_path}")

        if args.report_path:
            rc = generate_report(output_path, args.height, args.report_path)

    if temp_file:
        os.unlink(output_path)

    return rc


if __name__ == "__main__":
    raise SystemExit(main())
