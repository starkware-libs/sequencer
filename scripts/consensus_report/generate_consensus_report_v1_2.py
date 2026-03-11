#!/usr/bin/env python3
"""
generate_canonical_report_v1_2.py

Usage:
  python3 generate_canonical_report_v1_2.py <logs.json> <height> <output.txt>

Canonical report v1.1 generator (plain text with ASCII tables).

Key canonical rules implemented:
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
- Input is a GCP Logs Explorer JSON export (JSON array of entries).
- All timestamps are rendered in UTC.
"""

from __future__ import annotations

import json
import re
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional, Tuple

ROUND_RE = re.compile(r"\bround[=: ]\s*(\d+)\b")


@dataclass
class ConsensusData:
    """Holds all parsed and indexed consensus log data."""

    # All filtered log entries for the height
    all_log_entries: List[Dict[str, Any]]

    # Indexed by namespace for weights lookup
    log_entries_by_namespace: Dict[str, List[Dict[str, Any]]]

    # Indexed by (round, validator_id)
    log_entries_by_round_and_validator: Dict[Tuple[int, str], List[Dict[str, Any]]]

    # Maps validator ID to its namespace
    namespace_by_validator_id: Dict[str, str]

    # All validator IDs participating in consensus
    validator_ids: List[str]

    # All rounds found in the logs
    consensus_rounds: List[int]

    # The block height being analyzed
    block_height: str


@dataclass
class VoteAnalysisResult:
    """Results from analyzing votes across all rounds and validators."""

    # Key: (round, validator_id, vote_type), Value: vote message
    vote_messages_by_round_validator_type: Dict[Tuple[int, str, str], str]


@dataclass
class ValidationAnalysisResult:
    """Results from analyzing proposal validation across rounds."""

    # Nested: round -> validator_id -> status ("Passed" or "Failed [N]")
    validation_status_by_round_validator: Dict[int, Dict[str, str]]

    # Evidence number and message for failed proposals
    validation_evidence_by_round: Dict[int, List[Tuple[int, str]]]


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


def get_validator_id(entry: Dict[str, Any]) -> Optional[str]:
    jp = entry.get("jsonPayload") or {}
    for sp in jp.get("spans") or []:
        if sp.get("name") == "run_consensus" and "validator_id" in sp:
            return sp["validator_id"]
    msg = jp.get("message") or ""
    match = re.search(r"PatriciaKey\((0x[0-9a-fA-F]+)\)", msg)
    return match.group(1) if match is not None else None


def height_match(entry: Dict[str, Any], height_str: str) -> bool:
    jp = entry.get("jsonPayload") or {}

    for sp in jp.get("spans") or []:
        if sp.get("name") == "run_height" and "height" in sp:
            return str(sp["height"]) == height_str

    blob = json.dumps(jp, ensure_ascii=False)
    if f"BlockNumber({height_str})" in blob:
        return True
    if f'"height": "{height_str}"' in blob:
        return True

    # Use word boundary regex to match height as complete number, not part of another number
    pattern = rf"\b{re.escape(height_str)}\b"
    return bool(re.search(pattern, jp.get("message") or ""))


def get_round(entry: Dict[str, Any]) -> Optional[int]:
    jp = entry.get("jsonPayload") or {}
    for sp in jp.get("spans") or []:
        if "round" in sp:
            try:
                return int(sp["round"])
            except Exception:
                pass
    msg = jp.get("message") or ""
    match = ROUND_RE.search(msg)
    if match is not None:
        return int(match.group(1))
    return None


def main() -> int:
    print("not implemented", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
