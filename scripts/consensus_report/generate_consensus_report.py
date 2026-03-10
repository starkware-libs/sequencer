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
from typing import Any, Dict, List, Optional, Tuple

ROUND_RE = re.compile(r"\bround[=: ]\s*(\d+)\b")
PATRICIA_KEY_RE = re.compile(r"PatriciaKey\((0x[0-9a-fA-F]+)\)")
HEIGHT_BLOCK_RE_TEMPLATE = (
    r"\b(?:block[\s_]?number|block|height)\b\s*(?::|=|\bis\b)?\s*\(?['\"]?\b{}\b['\"]?\)?"
)
ADVANCING_STEP_RE = re.compile(r"Advancing step:\s*from\s*(\w+)\s*to\s*(\w+)", re.I)
N_TXS_RE = re.compile(
    r"Finished building block as proposer\..*?Final number of transactions \(as set by the proposer\):\s*(\d+)\.",
    re.I | re.S,
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


def extract_weight(msg: str, name: str) -> str:
    match = re.search(rf"{name}:\s*(GasAmount\()?(\d+)", msg)
    return match.group(2).strip() if match is not None else ""


def parse_weights(msg: str) -> Dict[str, str]:
    l1_gas = extract_weight(msg, "l1_gas")
    state_diff_size = extract_weight(msg, "state_diff_size")
    sierra_gas = extract_weight(msg, "sierra_gas")
    n_txs = extract_weight(msg, "n_txs")
    proving_gas = extract_weight(msg, "proving_gas")
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
            if "START_ROUND" in msg:
                timestamp = parse_timestamp(entry)
                if timestamp is not None:
                    timestamps.append(timestamp)
    return min(timestamps) if len(timestamps) > 0 else None


def get_round_proposer_id(entries: List[Dict[str, Any]], round_num: int) -> Optional[str]:
    for entry in entries:
        if get_round(entry) == round_num:
            msg = get_message(entry)
            if "START_ROUND_PROPOSER" in msg:
                return get_validator_id(entry)
    return None


def get_round_proposer_namespace(
    entries: List[Dict[str, Any]], ns_by_id: Dict[str, str], round_num: int
) -> Optional[str]:
    for entry in entries:
        if get_round(entry) == round_num:
            msg = get_message(entry)
            if "START_ROUND_PROPOSER" in msg:
                ns = get_namespace(entry)
                if ns is not None:
                    return ns
                proposer_id = get_validator_id(entry)
                if proposer_id is not None and proposer_id in ns_by_id:
                    return ns_by_id.get(proposer_id)
                break
    return None


def round_end(entries: List[Dict[str, Any]], round_num: int) -> Optional[datetime]:
    """
    Determine the end timestamp of a consensus round.
    Returns the latest timestamp among entries for the given round that contain
    "DECISION_REACHED", "Decision reached", or "PROPOSAL_FAILED" in their message.
    If no such entries exist, falls back to the latest timestamp of any entry in the round.
    Returns None if no entries exist for the round.
    """
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
        return max(timestamps)
    # TODO(lev): Is this fallback necessary? One of the round ending messages must be in the log.
    timestamps = [parse_timestamp(entry) for entry in entries if get_round(entry) == round_num]
    return max(timestamps) if len(timestamps) > 0 else None


# TODO(lev): Add support for dealing with different namespaces with the same validator ID.
#            Change structures, functions and tables accordingly.


def proposal_start(
    entries_by_round_and_validator_id: Dict[Tuple[int, str], List[Dict[str, Any]]],
    validator_id: str,
    round_num: int,
) -> Optional[datetime]:
    timestamps = []
    for entry in entries_by_round_and_validator_id.get((round_num, validator_id), []):
        msg = get_message(entry)
        if "Accepting ProposalInit" in msg or "Received ProposalInit" in msg:
            timestamp = parse_timestamp(entry)
            if timestamp is not None:
                timestamps.append(timestamp)
    return min(timestamps) if timestamps else None


def n_txs_for_round(
    entries_by_ns: Dict[str, List[Dict[str, Any]]],
    entries: List[Dict[str, Any]],
    rounds: List[int],
    ns_by_id: Dict[str, str],
    round_num: int,
) -> str:
    """Round N_txs via proposer namespace within round window."""
    proposer_ns = get_round_proposer_namespace(entries, ns_by_id, round_num)
    if proposer_ns is None:
        return ""
    round_start_ts = round_start(entries, round_num)
    if round_start_ts is None:
        return ""
    round_end_ts = round_end(entries, round_num)
    if round_end_ts is None:
        later = [round for round in rounds if round > round_num]
        if len(later) > 0:
            round_end_ts = round_start(entries, later[0])
    for entry in entries_by_ns.get(proposer_ns, []):
        timestamp = parse_timestamp(entry)
        if (
            timestamp is None
            or timestamp < round_start_ts
            or (round_end_ts is not None and timestamp > round_end_ts)
        ):
            continue
        msg = get_message(entry)
        match = N_TXS_RE.search(msg)
        if match is not None:
            return match.group(1)
    return ""


def block_closing_reason(entries: List[Dict[str, Any]], round_num: int) -> str:
    # TODO(lev): Improve block closing reason determination logic in this function.
    round_start_ts = round_start(entries, round_num)
    round_end_ts = round_end(entries, round_num)
    for entry in entries:
        if get_round(entry) != round_num:
            continue
        msg = get_message(entry).lower()
        if "timeout" in msg and "block" in msg:
            timestamp = parse_timestamp(entry)
            if round_start_ts is not None and timestamp is not None and timestamp < round_start_ts:
                continue
            if round_end_ts is not None and timestamp is not None and timestamp > round_end_ts:
                continue
            return "TimeOut"
    return "Bounds"


def find_advancing_step(msg: str) -> Optional[Tuple[str, str]]:
    match = ADVANCING_STEP_RE.search(msg)
    if match is None:
        return None
    return match.group(1), match.group(2)


def weights_for_node_round(
    entries_by_round_and_validator: Dict[Tuple[int, str], List[Dict[str, Any]]],
    entries_by_ns: Dict[str, List[Dict[str, Any]]],
    ns_by_id: Dict[str, str],
    height_str: str,
    round_num: int,
    validator_id: str,
) -> Dict[str, str]:
    """Resources used: namespace-scoped weights before Advancing Propose->Prevote."""
    all_entries = entries_by_round_and_validator.get((round_num, validator_id), [])
    entries_list = [entry for entry in all_entries if parse_timestamp(entry) is not None]
    empty_resources = dict(l1_gas="", state_diff_size="", sierra_gas="", n_txs="", proving_gas="")
    if len(entries_list) == 0:
        return empty_resources
    ns = get_namespace(entries_list[0]) or (
        ns_by_id.get(validator_id) if validator_id in ns_by_id else None
    )
    if ns is None:
        return empty_resources

    # Looking for the "Advancing step: from Propose to Prevote".
    advance_timestamp = None
    for entry in entries_list:
        advancing_step = find_advancing_step(get_message(entry))
        if (
            advancing_step is not None
            and advancing_step[0].lower() == "propose"
            and advancing_step[1].lower() == "prevote"
        ):
            advance_timestamp = parse_timestamp(entry)
            break
    if advance_timestamp is None:
        return empty_resources

    marker = f"Block {height_str} final weights"
    candidates = []
    for entry in entries_by_ns.get(ns, []):
        timestamp = parse_timestamp(entry)
        if timestamp is None:
            continue
        if timestamp >= advance_timestamp:
            break
        msg = get_message(entry)
        if marker in msg:
            candidates.append((timestamp, msg))
    if len(candidates) == 0:
        return empty_resources
    return parse_weights(candidates[-1][1])


def proposal_failed_msg(
    entries_by_round_and_validator: Dict[Tuple[int, str], List[Dict[str, Any]]],
    validator_id: str,
    round_num: int,
) -> Optional[str]:
    for entry in entries_by_round_and_validator.get((round_num, validator_id), []):
        msg = get_message(entry)
        if "PROPOSAL_FAILED" in msg:
            timestamp = parse_timestamp(entry)
            if timestamp is not None:
                return msg
    return None


def main() -> int:
    print("not implemented", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
