#!/usr/bin/env python3
"""
This script takes as an input a GCP Logs Explorer JSON export (JSON array of entries) and a block height,
and generates a consensus report in plain text with ASCII tables.

Usage:
  python3 <script_name> <logs.json> <height> <output.txt>
  - <logs.json> is a GCP Logs Explorer JSON export (JSON array of entries)
  - <height> is the block height to generate the report for
  - <output.txt> is the path to the output file

These are some assumptions on the way to extract and print the information from the logs:
- Round N_txs:
    Extract ONLY from:
      "Finished building block as proposer. ... Final number of transactions (as set by the proposer): N."
    If absent for that round => blank.
- Round "Block closing reason":
    Printed ONLY if Round N_txs is non-blank; otherwise blank.
- Resources used (PER NODE, PER ROUND):
    For each node+round, choose the LAST "Block <HEIGHT> final weights: { ... }" message emitted in that node's
    namespace BEFORE that node's "Advancing step: from Propose to Prevote" timestamp.
    (Weights logs lack consensus spans, so we match by namespace + time.)
    If missing => blank.
- Votes:
    After "Advancing step: from Propose to Prevote" => pick FIRST subsequent
      "Broadcasting Vote { vote_type: Prevote ... }" for that node+round.
    After "Advancing step: from Prevote to Precommit" => pick FIRST subsequent
      "Broadcasting Vote { vote_type: Precommit ... }" for that node+round.
    If vote state is yes, render "yes [N]" and print evidence lines after the table; numbering resets per round.
- Duration:
    Always formatted as seconds.mmm (exactly 3 digits after '.').

Notes:
- All timestamps are rendered in UTC.
"""

from __future__ import annotations

import json
import re
import sys
from datetime import datetime, timezone
from typing import Any, Dict, Optional

ROUND_RE = re.compile(r"\bround[=: ]\s*(\d+)\b")
PATRICIA_KEY_RE = re.compile(r"PatriciaKey\((0x[0-9a-fA-F]+)\)")

HEIGHT_BLOCK_RE_TEMPLATE = (
    r"\b(?:block[\s_]?number|block|height)\b\s*(?::|=|\bis\b)?\s*\(?['\"]?\b{}\b['\"]?\)?"
)


def parse_timestamp(entry: Dict[str, Any]) -> Optional[datetime]:
    timestamp_str = (
        entry.get("timestamp")
        or (entry.get("jsonPayload") or {}).get("timestamp")
        or entry.get("receiveTimestamp")
    )
    if timestamp_str is None:
        return None
    # Truncate sub-second part to 6 digits (microseconds) - fromisoformat doesn't support nanoseconds
    normalized = re.sub(r"(\.\d{6})\d+", r"\1", timestamp_str.replace("Z", "+00:00"))
    return datetime.fromisoformat(normalized).astimezone(timezone.utc)


def short_id(full_hex: Optional[str]) -> str:
    if full_hex is None or not isinstance(full_hex, str) or not full_hex.startswith("0x"):
        return full_hex or ""
    hex_digits = full_hex[2:].lstrip("0") or "0"
    return "0x" + hex_digits.lower()


def fmt_timestamp(dt: Optional[datetime]) -> str:
    """Returns a UTC formatted datetime string truncated to milliseconds"""
    if dt is None:
        return ""
    return dt.astimezone(timezone.utc).strftime("%Y-%m-%d %H:%M:%S.%f")[:-3]


def fmt_duration_seconds(dt_end: Optional[datetime], dt_start: Optional[datetime]) -> str:
    if dt_end is None or dt_start is None:
        return ""
    sec = (dt_end - dt_start).total_seconds()
    return f"{sec:.3f}"


def get_namespace(entry: Dict[str, Any]) -> Optional[str]:
    return ((entry.get("resource") or {}).get("labels") or {}).get("namespace_name")


def get_message(entry: Dict[str, Any]) -> str:
    return (entry.get("jsonPayload") or {}).get("message") or ""


def get_spans_value_or_message_field(
    entry: Dict[str, Any], span_name: Optional[str], field_name: str
) -> Tuple[Optional[Any], Optional[Any]]:
    """
    Extract a field value from a span in the entry's jsonPayload.
    If span_name is None, search for the field in every span.
    Returns the span value, or None if the field is not found, and the message field.
    """
    jp = entry.get("jsonPayload") or {}
    msg = jp.get("message") or ""
    for sp in jp.get("spans") or []:
        if span_name is None or sp.get("name") == span_name:
            if field_name in sp:
                return sp[field_name], msg
    return None, msg


def get_validator_id(entry: Dict[str, Any]) -> Optional[str]:
    validator_id, message = get_spans_value_or_message_field(
        entry=entry, span_name=None, field_name="validator_id"
    )
    if validator_id is not None:
        return validator_id
    match = PATRICIA_KEY_RE.search(message)
    return match.group(1) if match is not None else None


def height_match(entry: Dict[str, Any], height_str: str) -> bool:
    height, message = get_spans_value_or_message_field(
        entry=entry, span_name="run_height", field_name="height"
    )
    if height is not None:
        return str(height) == height_str

    # Height may appear inside a value in "spans" as part of a long text, or in the message field.
    # Extract it from the entire textual representation of the log entry jsonPayload field.
    jp = entry.get("jsonPayload") or {}
    blob = json.dumps(jp, ensure_ascii=False)

    height_match_pattern = HEIGHT_BLOCK_RE_TEMPLATE.format(re.escape(height_str))
    if re.search(height_match_pattern, blob, re.IGNORECASE):
        return True
    return False


def get_round(entry: Dict[str, Any]) -> Optional[int]:
    round, message = get_spans_value_or_message_field(
        entry=entry, span_name=None, field_name="round"
    )
    if round is not None:
        try:
            return int(round)
        except Exception:
            message = get_message(entry)
    match = ROUND_RE.search(message)
    if match is not None:
        return int(match.group(1))
    return None


def main() -> int:
    print("not implemented", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
