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
    (Weights logs often lack consensus spans, so we match by namespace + time.)
    If missing => blank resource subcolumns.
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


def parse_timestamp(entry: Dict[str, Any]) -> Optional[datetime]:
    timestamp_str = (
        entry.get("timestamp")
        or (entry.get("jsonPayload") or {}).get("timestamp")
        or entry.get("receiveTimestamp")
    )
    if timestamp_str is None:
        return None
    return datetime.fromisoformat(timestamp_str.replace("Z", "+00:00")).astimezone(timezone.utc)


def short_id(full_hex: Optional[str]) -> str:
    if full_hex is None or not isinstance(full_hex, str) or not full_hex.startswith("0x"):
        return full_hex or ""
    hex_digits = full_hex[2:].lstrip("0") or "0"
    if len(hex_digits) > 4:
        hex_digits = hex_digits[-4:]
    return "0x" + hex_digits.lower()


def fmt_timestamp(dt: Optional[datetime]) -> str:
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
    If the value does not appear in the payload, it returns the whole message entry.
    """
    jp = entry.get("jsonPayload") or {}
    for sp in jp.get("spans") or []:
        if span_name is None or sp.get("name") == span_name:
            if field_name in sp:
                return sp[field_name], None
    return None, jp.get("message") or ""


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

    jp = entry.get("jsonPayload") or {}
    blob = json.dumps(jp, ensure_ascii=False)
    if f"BlockNumber({height_str})" in blob:
        return True
    if f'"height": "{height_str}"' in blob:
        return True

    # Fallback: look for height in the message field.
    # Use word boundary regex to match height as complete number, not part of another number
    pattern = rf"\b{re.escape(height_str)}\b"
    return True if re.search(pattern, message) is not None else False


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


def determine_column_widths(headers: List[str], rows: List[List[str]]) -> List[int]:
    """Calculate the width needed for each column based on headers and row data."""
    widths = [len(str(header)) for header in headers]
    for row in rows:
        for i, cell in enumerate(row):
            if i < len(widths):
                widths[i] = max(widths[i], len(str(cell)))
    return widths


def ascii_table(
    headers: List[str], rows: List[List[str]], aligns: Optional[List[str]] = None
) -> str:
    aligns = aligns or ["l"] * len(headers)
    widths = determine_column_widths(headers, rows)

    def pad(cell: str, width: int, align: str) -> str:
        s = str(cell)
        if align == "r":
            return s.rjust(width)
        if align == "c":
            return s.center(width)
        return s.ljust(width)

    sep = "+" + "+".join("-" * (width + 2) for width in widths) + "+"
    out = [
        sep,
        "|"
        + "|".join(" " + pad(header, width, "c") + " " for header, width in zip(headers, widths))
        + "|",
        sep,
    ]
    for row in rows:
        out.append(
            "|"
            + "|".join(
                " " + pad(cell, width, align) + " "
                for cell, width, align in zip(row, widths, aligns)
            )
            + "|"
        )
    out.append(sep)
    return "\n".join(out)


def ascii_table_with_spanner(
    spanner: str, left_headers: List[str], right_headers: List[str], rows: List[List[str]]
) -> str:
    headers = left_headers + right_headers
    widths = determine_column_widths(headers, rows)
    sep = "+" + "+".join("-" * (width + 2) for width in widths) + "+"

    r_start = len(left_headers)
    total_right = sum((width + 2) for width in widths[r_start:]) + (len(right_headers) - 1)
    sp_cell = " " + spanner.center(max(0, total_right - 2)) + " "

    out = [sep]
    row_a = "|"
    for i in range(len(left_headers)):
        row_a += " " + "".ljust(widths[i]) + " |"
    row_a += sp_cell + "|"
    out.append(row_a)

    row_b = (
        "|"
        + "|".join(" " + header.center(width) + " " for header, width in zip(headers, widths))
        + "|"
    )
    out.append(row_b)
    out.append(sep)

    aligns = ["l"] * len(headers)
    for j, header in enumerate(headers):
        if header in (
            "Duration",
            "l1_gas",
            "state_diff_size",
            "sierra_gas",
            "n_txs",
            "proving_gas",
        ):
            aligns[j] = "r"

    for row in rows:
        out.append(
            "|"
            + "|".join(
                " " + (str(cell).rjust(width) if align == "r" else str(cell).ljust(width)) + " "
                for cell, width, align in zip(row, widths, aligns)
            )
            + "|"
        )
    out.append(sep)
    return "\n".join(out)


def grab(msg: str, name: str) -> str:
    match = re.search(rf"{name}:\s*([^,}}]+)", msg)
    return match.group(1).strip() if match is not None else ""


def parse_weights(msg: str) -> Dict[str, str]:
    l1_gas = grab(msg, "l1_gas")
    state_diff_size = grab(msg, "state_diff_size")
    sierra_gas = re.sub(r"GasAmount\((\d+)\)", r"\1", grab(msg, "sierra_gas"))
    n_txs = grab(msg, "n_txs")
    proving_gas = re.sub(r"GasAmount\((\d+)\)", r"\1", grab(msg, "proving_gas"))
    return dict(
        l1_gas=l1_gas,
        state_diff_size=state_diff_size,
        sierra_gas=sierra_gas,
        n_txs=n_txs,
        proving_gas=proving_gas,
    )


def vote_state(vote_msg: Optional[str]) -> str:
    if vote_msg is None:
        return "missed"
    if "proposal_commitment: Some" in vote_msg or "proposal_commitment=Some" in vote_msg:
        return "yes"
    if "proposal_commitment: None" in vote_msg or "proposal_commitment=None" in vote_msg:
        return "nil"
    return "missed"


def round_start(entries: List[Dict[str, Any]], round_num: int) -> Optional[datetime]:
    timestamps = []
    for entry in entries:
        if get_round(entry) == round_num:
            msg = get_message(entry)
            if "Starting round" in msg:
                timestamp = parse_timestamp(entry)
                if timestamp is not None:
                    timestamps.append(timestamp)
    return min(timestamps) if len(timestamps) > 0 else None


def get_round_proposer_id(entries: List[Dict[str, Any]], round_num: int) -> Optional[str]:
    for entry in entries:
        if get_round(entry) == round_num:
            msg = get_message(entry)
            if "Starting round" in msg and "as Proposer" in msg:
                return get_validator_id(entry)
    return None


def get_round_proposer_namespace(
    entries: List[Dict[str, Any]], ns_by_id: Dict[str, str], round_num: int
) -> Optional[str]:
    for entry in entries:
        if get_round(entry) == round_num:
            msg = get_message(entry)
            if "Starting round" in msg and "as Proposer" in msg:
                ns = get_namespace(entry)
                if ns is not None:
                    return ns
                proposer_id = get_validator_id(entry)
                if proposer_id is not None and proposer_id in ns_by_id:
                    return ns_by_id.get(proposer_id)
    return None


def round_end(entries: List[Dict[str, Any]], round_num: int) -> Optional[datetime]:
    timestamps = []
    for entry in entries:
        if get_round(entry) == round_num:
            msg = get_message(entry)
            if (
                ("DECISION_REACHED" in msg)
                or ("Decision reached" in msg)
                or ("PROPOSAL_FAILED" in msg)
            ):
                timestamp = parse_timestamp(entry)
                if timestamp is not None:
                    timestamps.append(timestamp)
    if len(timestamps) > 0:
        return min(timestamps)
    timestamps = [parse_timestamp(entry) for entry in entries if get_round(entry) == round_num]
    return max(timestamps) if len(timestamps) > 0 else None


def proposal_start(
    entries_by_round_and_vid: Dict[Tuple[int, str], List[Dict[str, Any]]],
    validator_id: str,
    round_num: int,
) -> Optional[datetime]:
    timestamps = []
    for entry in entries_by_round_and_vid.get((round_num, validator_id), []):
        msg = get_message(entry)
        if "Accepting ProposalInit" in msg or "Received ProposalInit" in msg:
            timestamp = parse_timestamp(entry)
            if timestamp is not None:
                timestamps.append(timestamp)
    return min(timestamps) if timestamps else None


def main() -> int:
    print("not implemented", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
