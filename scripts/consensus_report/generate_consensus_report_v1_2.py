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

import json
import re
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional, Tuple

ROUND_RE = re.compile(r"\bround[=: ]\s*(\d+)\b")

ADV_RE = re.compile(r"Advancing step:\s*from\s*(\w+)\s*to\s*(\w+)", re.I)

NTXS_RE = re.compile(
    r"Finished building block as proposer\..*?Final number of transactions \(as set by the proposer\):\s*(\d+)\.",
    re.I | re.S,
)


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


def determine_column_widths(headers: List[str], rows: List[List[str]]) -> List[int]:
    """Calculate the width needed for each column based on headers and row data."""
    widths = [len(str(header)) for header in headers]
    for row in rows:
        for i, cell in enumerate(row):
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


def parse_weights(msg: str) -> Dict[str, str]:
    def grab(name: str) -> str:
        match = re.search(rf"{name}:\s*([^,}}]+)", msg)
        return match.group(1).strip() if match is not None else ""

    l1_gas = grab("l1_gas")
    state_diff_size = grab("state_diff_size")
    sierra_gas = re.sub(r"GasAmount\((\d+)\)", r"\1", grab("sierra_gas"))
    n_txs = grab("n_txs")
    proving_gas = re.sub(r"GasAmount\((\d+)\)", r"\1", grab("proving_gas"))
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
    for e in entries:
        if get_round(e) == round_num:
            msg = (e.get("jsonPayload") or {}).get("message") or ""
            if "Starting round" in msg and "round" in msg:
                timestamps.append(parse_timestamp(e))
    return min(timestamps) if len(timestamps) > 0 else None


def get_round_proposer_id(entries: List[Dict[str, Any]], round_num: int) -> Optional[str]:
    for e in entries:
        if get_round(e) == round_num:
            msg = (e.get("jsonPayload") or {}).get("message") or ""
            if "Starting round" in msg and "as Proposer" in msg:
                return get_validator_id(e)
    return None


def get_round_proposer_ns(
    entries: List[Dict[str, Any]], ns_by_id: Dict[str, str], round_num: int
) -> Optional[str]:
    for e in entries:
        if get_round(e) == round_num:
            msg = (e.get("jsonPayload") or {}).get("message") or ""
            if "Starting round" in msg and "as Proposer" in msg:
                ns = get_namespace(e)
                if ns is not None:
                    return ns
    # Fallback: get via validator ID lookup
    pid = get_round_proposer_id(entries, round_num)
    return ns_by_id.get(pid) if pid is not None else None


def round_end(entries: List[Dict[str, Any]], round_num: int) -> Optional[datetime]:
    timestamps = []
    for e in entries:
        if get_round(e) == round_num:
            msg = (e.get("jsonPayload") or {}).get("message") or ""
            if (
                ("DECISION_REACHED" in msg)
                or ("Decision reached" in msg)
                or ("PROPOSAL_FAILED" in msg)
            ):
                timestamps.append(parse_timestamp(e))
    if len(timestamps) > 0:
        return min(timestamps)
    timestamps = [parse_timestamp(e) for e in entries if get_round(e) == round_num]
    return max(timestamps) if len(timestamps) > 0 else None


def proposal_start(
    entries_by_rv: Dict[Tuple[int, str], List[Dict[str, Any]]], vid: str, round_num: int
) -> Optional[datetime]:
    best = None
    for substr in ("Accepting ProposalInit", "Received ProposalInit"):
        timestamps = [
            parse_timestamp(e)
            for e in entries_by_rv.get((round_num, vid), [])
            if substr in (((e.get("jsonPayload") or {}).get("message")) or "")
        ]
        if len(timestamps) > 0:
            best = min(timestamps) if best is None else min(best, min(timestamps))
    return best


def ntxs_for_round(
    entries_by_ns: Dict[str, List[Dict[str, Any]]],
    entries: List[Dict[str, Any]],
    rounds: List[int],
    ns_by_id: Dict[str, str],
    round_num: int,
) -> str:
    """Round N_txs via proposer namespace within round window."""
    pns = get_round_proposer_ns(entries, ns_by_id, round_num)
    if pns is None:
        return ""
    round_start_ts = round_start(entries, round_num)
    if round_start_ts is None:
        return ""
    round_end_ts = round_end(entries, round_num)
    if round_end_ts is None:
        later = [rn for rn in rounds if rn > round_num]
        if len(later) > 0:
            round_end_ts = round_start(entries, later[0])
    for e in entries_by_ns.get(pns, []):
        timestamp = parse_timestamp(e)
        if timestamp is None:
            continue
        if timestamp < round_start_ts:
            continue
        if round_end_ts is not None and timestamp > round_end_ts:
            continue
        msg = (e.get("jsonPayload") or {}).get("message") or ""
        match = NTXS_RE.search(msg)
        if match is not None:
            return match.group(1)
    return ""


def block_closing_reason(entries: List[Dict[str, Any]], round_num: int) -> str:
    round_start_ts = round_start(entries, round_num)
    round_end_ts = round_end(entries, round_num)
    for e in entries:
        if get_round(e) != round_num:
            continue
        msg = ((e.get("jsonPayload") or {}).get("message") or "").lower()
        if "timeout" in msg and "block" in msg:
            timestamp = parse_timestamp(e)
            if round_start_ts is not None and timestamp is not None and timestamp < round_start_ts:
                continue
            if round_end_ts is not None and timestamp is not None and timestamp > round_end_ts:
                continue
            return "TimeOut"
    return "Bounds"


def weights_for_node_round(
    entries_by_rv: Dict[Tuple[int, str], List[Dict[str, Any]]],
    entries_by_ns: Dict[str, List[Dict[str, Any]]],
    ns_by_id: Dict[str, str],
    height_str: str,
    round_num: int,
    validator_id: str,
) -> Dict[str, str]:
    """Resources used: namespace-scoped weights before Advancing Propose->Prevote."""
    entries_list = sorted(
        entries_by_rv.get((round_num, validator_id), []), key=lambda e: parse_timestamp(e)
    )
    if len(entries_list) == 0:
        return dict(l1_gas="", state_diff_size="", sierra_gas="", n_txs="", proving_gas="")
    ns = get_namespace(entries_list[0]) or (
        ns_by_id.get(validator_id) if validator_id in ns_by_id else None
    )
    if ns is None:
        return dict(l1_gas="", state_diff_size="", sierra_gas="", n_txs="", proving_gas="")

    advance_timestamp = None
    for e in entries_list:
        msg = (e.get("jsonPayload") or {}).get("message") or ""
        if "Advancing step: from Propose to Prevote" in msg:
            advance_timestamp = parse_timestamp(e)
            break
    if advance_timestamp is None:
        return dict(l1_gas="", state_diff_size="", sierra_gas="", n_txs="", proving_gas="")

    marker = f"Block {height_str} final weights"
    candidates = []
    for e in entries_by_ns.get(ns, []):
        timestamp = parse_timestamp(e)
        if timestamp is None or timestamp >= advance_timestamp:
            break
        msg = (e.get("jsonPayload") or {}).get("message") or ""
        if marker in msg:
            candidates.append((timestamp, msg))
    if len(candidates) == 0:
        return dict(l1_gas="", state_diff_size="", sierra_gas="", n_txs="", proving_gas="")
    return parse_weights(candidates[-1][1])


def adv_step(msg: str) -> Optional[Tuple[str, str]]:
    match = ADV_RE.search(msg)
    if match is None:
        return None
    return match.group(1), match.group(2)


def proposal_failed_msg(
    entries_by_rv: Dict[Tuple[int, str], List[Dict[str, Any]]], vid: str, round_num: int
) -> Optional[str]:
    cand = []
    for e in entries_by_rv.get((round_num, vid), []):
        msg = (e.get("jsonPayload") or {}).get("message") or ""
        if "PROPOSAL_FAILED" in msg:
            cand.append((parse_timestamp(e), msg))
    cand.sort(key=lambda x: x[0])
    return cand[0][1] if len(cand) > 0 else None


def main() -> int:
    print("not implemented", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
