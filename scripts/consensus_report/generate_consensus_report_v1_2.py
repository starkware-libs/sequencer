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
from collections import defaultdict
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

ROUND_RE = re.compile(r"\bround[=: ]\s*(\d+)\b")

ADV_RE = re.compile(r"Advancing step:\s*from\s*(\w+)\s*to\s*(\w+)", re.I)

BROADCAST_PREVOTE_RE = re.compile(r"Broadcasting Vote\s*\{[^}]*vote_type:\s*Prevote\b", re.I)
BROADCAST_PRECOMMIT_RE = re.compile(r"Broadcasting Vote\s*\{[^}]*vote_type:\s*Precommit\b", re.I)
BROADCAST_VOTE_RE = {
    "prevote": BROADCAST_PREVOTE_RE,
    "precommit": BROADCAST_PRECOMMIT_RE,
}

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


def final_weights_marker(height_str: str) -> str:
    return f"Block {height_str} final weights"


ADV_P2P_STR = "Advancing step: from Propose to Prevote"


def parse_timestamp(entry: Dict[str, Any]) -> Optional[datetime]:
    timestamp_str = (
        entry.get("timestamp")
        or (entry.get("jsonPayload") or {}).get("timestamp")
        or entry.get("receiveTimestamp")
    )
    if not timestamp_str:
        return None
    return datetime.fromisoformat(timestamp_str.replace("Z", "+00:00")).astimezone(timezone.utc)


def short_id(full_hex: Optional[str]) -> str:
    if not full_hex or not isinstance(full_hex, str) or not full_hex.startswith("0x"):
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
    return match.group(1) if match else None


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
    if match:
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
        return match.group(1).strip() if match else ""

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
    if not vote_msg:
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
    return min(timestamps) if timestamps else None


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
                if ns:  # Only return if we got a valid namespace
                    return ns
    # Fallback: get via validator ID lookup
    pid = get_round_proposer_id(entries, round_num)
    return ns_by_id.get(pid) if pid else None


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
    if timestamps:
        return min(timestamps)
    timestamps = [parse_timestamp(e) for e in entries if get_round(e) == round_num]
    return max(timestamps) if timestamps else None


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
        if timestamps:
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
    if not pns:
        return ""
    rs = round_start(entries, round_num)
    re_ = round_end(entries, round_num)
    if re_ is None:
        later = [rr for rr in rounds if rr > round_num]
        if later:
            re_ = round_start(entries, later[0])
    if rs is None:
        return ""
    for e in entries_by_ns.get(pns, []):
        timestamp = parse_timestamp(e)
        if timestamp is None:
            continue
        if timestamp < rs:
            continue
        if re_ is not None and timestamp > re_:
            continue
        msg = (e.get("jsonPayload") or {}).get("message") or ""
        match = NTXS_RE.search(msg)
        if match:
            return match.group(1)
    return ""


def block_closing_reason(entries: List[Dict[str, Any]], round_num: int) -> str:
    rs = round_start(entries, round_num)
    re_ = round_end(entries, round_num)
    for e in entries:
        if get_round(e) != round_num:
            continue
        msg = ((e.get("jsonPayload") or {}).get("message") or "").lower()
        if "timeout" in msg and "block" in msg:
            timestamp = parse_timestamp(e)
            if rs and timestamp and timestamp < rs:
                continue
            if re_ and timestamp and timestamp > re_:
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
    if not entries_list:
        return dict(l1_gas="", state_diff_size="", sierra_gas="", n_txs="", proving_gas="")
    ns = get_namespace(entries_list[0]) or (
        ns_by_id.get(validator_id) if validator_id in ns_by_id else None
    )
    if not ns:
        return dict(l1_gas="", state_diff_size="", sierra_gas="", n_txs="", proving_gas="")

    # find the Propose->Prevote advance time for this node+roundr
    advance_timestamp = None
    for e in entries_list:
        msg = (e.get("jsonPayload") or {}).get("message") or ""
        if ADV_P2P_STR in msg:
            advance_timestamp = parse_timestamp(e)
            break
    if advance_timestamp is None:
        return dict(l1_gas="", state_diff_size="", sierra_gas="", n_txs="", proving_gas="")

    marker = final_weights_marker(height_str)
    # search within namespace logs for last marker before advance_timestamp
    candidates = []
    for e in entries_by_ns.get(ns, []):
        timestamp = parse_timestamp(e)
        if timestamp is None or timestamp >= advance_timestamp:
            break
        msg = (e.get("jsonPayload") or {}).get("message") or ""
        if marker in msg:
            candidates.append((timestamp, msg))
    if not candidates:
        return dict(l1_gas="", state_diff_size="", sierra_gas="", n_txs="", proving_gas="")
    return parse_weights(candidates[-1][1])


def adv_step(msg: str) -> Optional[Tuple[str, str]]:
    match = ADV_RE.search(msg)
    if not match:
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
    return cand[0][1] if cand else None


def load_and_filter_log_entries_for_height(
    logs_file_path: str, block_height: str
) -> List[Dict[str, Any]]:
    """Load JSON log file, filter entries matching the block height, and sort by timestamp."""
    with open(logs_file_path, "r", encoding="utf-8") as f:
        data = json.load(f)

    filtered_entries = [e for e in data if height_match(e, block_height)]
    filtered_entries = [e for e in filtered_entries if parse_timestamp(e) is not None]
    filtered_entries.sort(key=lambda e: parse_timestamp(e))

    return filtered_entries


def build_indexed_consensus_data(
    filtered_log_entries: List[Dict[str, Any]], block_height: str
) -> ConsensusData:
    """Create all index structures (by namespace, by round+validator, validator mapping, etc.)."""
    # Index entries by namespace (for weights matching)
    log_entries_by_namespace: Dict[str, List[Dict[str, Any]]] = defaultdict(list)
    for e in filtered_log_entries:
        ns = get_namespace(e)
        if ns:
            log_entries_by_namespace[ns].append(e)
    for ns in list(log_entries_by_namespace.keys()):
        log_entries_by_namespace[ns].sort(key=lambda e: parse_timestamp(e))

    # Nodes mapping (validator_id <-> namespace) from consensus logs
    namespace_by_validator_id: Dict[str, str] = {}
    validator_ids: List[str] = []
    for e in filtered_log_entries:
        vid = get_validator_id(e)
        ns = get_namespace(e)
        if vid and vid not in validator_ids:
            validator_ids.append(vid)
        if vid and ns and vid not in namespace_by_validator_id:
            namespace_by_validator_id[vid] = ns
    validator_ids = sorted(set(validator_ids), key=lambda x: short_id(x))

    consensus_rounds = sorted(
        set(get_round(e) for e in filtered_log_entries if get_round(e) is not None)
    )

    log_entries_by_round_and_validator: Dict[Tuple[int, str], List[Dict[str, Any]]] = defaultdict(
        list
    )
    for e in filtered_log_entries:
        round_num = get_round(e)
        validator_id = get_validator_id(e)
        if round_num is not None and validator_id is not None:
            log_entries_by_round_and_validator[(round_num, validator_id)].append(e)

    return ConsensusData(
        all_log_entries=filtered_log_entries,
        log_entries_by_namespace=log_entries_by_namespace,
        log_entries_by_round_and_validator=log_entries_by_round_and_validator,
        namespace_by_validator_id=namespace_by_validator_id,
        validator_ids=validator_ids,
        consensus_rounds=consensus_rounds,
        block_height=block_height,
    )


def extract_vote_messages_for_all_rounds(consensus_data: ConsensusData) -> VoteAnalysisResult:
    """Find the first prevote/precommit broadcast after each advancing step transition."""
    vote_messages_by_round_validator_type: Dict[Tuple[int, str, str], str] = {}

    for (
        round_num,
        validator_id,
    ), entries_list in consensus_data.log_entries_by_round_and_validator.items():
        entries_list_sorted = sorted(entries_list, key=lambda e: parse_timestamp(e))
        for i, e in enumerate(entries_list_sorted):
            msg = (e.get("jsonPayload") or {}).get("message") or ""
            adv = adv_step(msg)
            if not adv:
                continue
            frm, to = adv[0].lower(), adv[1].lower()
            after_timestamp = parse_timestamp(e)

            if frm == "propose" and to == "prevote" or frm == "prevote" and to == "precommit":
                for e2 in entries_list_sorted[i + 1 :]:
                    msg2 = (e2.get("jsonPayload") or {}).get("message") or ""
                    if (
                        after_timestamp
                        and parse_timestamp(e2)
                        and parse_timestamp(e2) >= after_timestamp
                        and BROADCAST_VOTE_RE[to].search(msg2)
                    ):
                        vote_messages_by_round_validator_type[
                            (round_num, validator_id, to.capitalize())
                        ] = msg2
                        break

    return VoteAnalysisResult(
        vote_messages_by_round_validator_type=vote_messages_by_round_validator_type
    )


def collect_validation_evidence_for_all_rounds(
    consensus_data: ConsensusData,
) -> ValidationAnalysisResult:
    """Check each validator's proposal validation status per round and collect evidence."""
    validation_status_by_round_validator = defaultdict(dict)
    validation_evidence_by_round = defaultdict(list)

    for round_num in consensus_data.consensus_rounds:
        ev_no = 1
        for validator_id in consensus_data.validator_ids:
            pf = proposal_failed_msg(
                consensus_data.log_entries_by_round_and_validator, validator_id, round_num
            )
            if pf:
                validation_status_by_round_validator[round_num][validator_id] = f"Failed [{ev_no}]"
                validation_evidence_by_round[round_num].append((ev_no, pf))
                ev_no += 1
            else:
                validation_status_by_round_validator[round_num][validator_id] = "Passed"

    return ValidationAnalysisResult(
        validation_status_by_round_validator=validation_status_by_round_validator,
        validation_evidence_by_round=validation_evidence_by_round,
    )


def render_validator_nodes_summary_section(
    validator_ids: List[str], namespace_by_validator_id: Dict[str, str]
) -> List[str]:
    """Render the NODES section showing namespace and ID for each validator."""
    section_lines = []
    section_lines.append("NODES")
    section_lines.append("-----")
    section_lines.append("")
    section_lines.append(
        ascii_table(
            ["Namespace", "ID"],
            [
                [namespace_by_validator_id.get(validator_id, ""), short_id(validator_id)]
                for validator_id in validator_ids
            ],
            aligns=["l", "l"],
        )
    )
    section_lines.append("")
    return section_lines


@dataclass
class RoundMetadata:
    """Metadata collected for a round."""

    round_start_time: Optional[datetime]
    round_end_time: Optional[datetime]
    proposer: Optional[str]
    round_ntxs: str
    closing_reason: str


def collect_round_metadata(round_number: int, consensus_data: ConsensusData) -> RoundMetadata:
    """Collect round timing and metadata."""
    rs = round_start(consensus_data.all_log_entries, round_number)
    re_ = round_end(consensus_data.all_log_entries, round_number)
    proposer = get_round_proposer_id(consensus_data.all_log_entries, round_number)

    round_ntxs = ntxs_for_round(
        consensus_data.log_entries_by_namespace,
        consensus_data.all_log_entries,
        consensus_data.consensus_rounds,
        consensus_data.namespace_by_validator_id,
        round_number,
    )
    reason = (
        block_closing_reason(consensus_data.all_log_entries, round_number)
        if round_ntxs != ""
        else ""
    )

    return RoundMetadata(
        round_start_time=rs,
        round_end_time=re_,
        proposer=proposer,
        round_ntxs=round_ntxs,
        closing_reason=reason,
    )


def render_round_header(round_number: int) -> List[str]:
    """Render the round header with title and separator."""
    return [
        f"ROUND {round_number}",
        "-" * (6 + len(str(round_number))),
        "",
    ]


def render_round_summary_table(metadata: RoundMetadata) -> List[str]:
    """Render the round summary table with timing and metadata."""
    return [
        ascii_table(
            ["Start (UTC)", "End (UTC)", "Proposer", "N_txs", "Block closing reason"],
            [
                [
                    fmt_timestamp(metadata.round_start_time),
                    fmt_timestamp(metadata.round_end_time),
                    short_id(metadata.proposer) if metadata.proposer else "",
                    metadata.round_ntxs,
                    metadata.closing_reason,
                ]
            ],
            aligns=["l", "l", "l", "r", "l"],
        ),
        "",
    ]


def format_vote_cell(
    vote_msg: Optional[str], vote_notes: List[Tuple[int, str]], vote_no: int
) -> Tuple[str, int]:
    """Format a vote cell and update vote notes if needed."""
    vote = vote_state(vote_msg)
    if vote == "yes":
        cell = f"yes [{vote_no}]"
        vote_notes.append((vote_no, vote_msg or ""))
        return cell, vote_no + 1
    return vote, vote_no


def build_node_details_row(
    validator_id: str,
    round_number: int,
    metadata: RoundMetadata,
    consensus_data: ConsensusData,
    vote_analysis: VoteAnalysisResult,
    validation_analysis: ValidationAnalysisResult,
    vote_notes: List[Tuple[int, str]],
    vote_no: int,
) -> Tuple[List[str], int]:
    """Build a single row for node details table."""
    role = "Proposer" if metadata.proposer and validator_id == metadata.proposer else "Validator"
    ps = (
        proposal_start(
            consensus_data.log_entries_by_round_and_validator, validator_id, round_number
        )
        or metadata.round_start_time
    )
    dur = fmt_duration_seconds(metadata.round_end_time, ps)

    pv_msg = vote_analysis.vote_messages_by_round_validator_type.get(
        (round_number, validator_id, "Prevote")
    )
    pc_msg = vote_analysis.vote_messages_by_round_validator_type.get(
        (round_number, validator_id, "Precommit")
    )

    pv_cell, vote_no = format_vote_cell(pv_msg, vote_notes, vote_no)
    pc_cell, vote_no = format_vote_cell(pc_msg, vote_notes, vote_no)

    w_nv = weights_for_node_round(
        consensus_data.log_entries_by_round_and_validator,
        consensus_data.log_entries_by_namespace,
        consensus_data.namespace_by_validator_id,
        consensus_data.block_height,
        round_number,
        validator_id,
    )

    row = [
        short_id(validator_id),
        role,
        fmt_timestamp(ps),
        dur,
        validation_analysis.validation_status_by_round_validator[round_number].get(
            validator_id, "Passed"
        ),
        pv_cell,
        pc_cell,
        w_nv.get("l1_gas", ""),
        w_nv.get("state_diff_size", ""),
        w_nv.get("sierra_gas", ""),
        w_nv.get("n_txs", ""),
        w_nv.get("proving_gas", ""),
    ]

    return row, vote_no


def build_node_details_table(
    round_number: int,
    metadata: RoundMetadata,
    consensus_data: ConsensusData,
    vote_analysis: VoteAnalysisResult,
    validation_analysis: ValidationAnalysisResult,
) -> Tuple[List[str], List[Tuple[int, str]]]:
    """Build the complete node details table with all validators."""
    vote_notes: List[Tuple[int, str]] = []
    vote_no = 1

    left_headers = [
        "Node ID",
        "Role",
        "Proposal Start",
        "Duration",
        "Validation",
        "Prevote",
        "Precommit",
    ]
    right_headers = ["l1_gas", "state_diff_size", "sierra_gas", "n_txs", "proving_gas"]
    rows: List[List[str]] = []

    for validator_id in consensus_data.validator_ids:
        row, vote_no = build_node_details_row(
            validator_id,
            round_number,
            metadata,
            consensus_data,
            vote_analysis,
            validation_analysis,
            vote_notes,
            vote_no,
        )
        rows.append(row)

    table_lines = [ascii_table_with_spanner("Resources used:", left_headers, right_headers, rows)]

    return table_lines, vote_notes


def render_evidence_sections(
    round_number: int,
    vote_notes: List[Tuple[int, str]],
    validation_analysis: ValidationAnalysisResult,
) -> List[str]:
    """Render vote and validation evidence sections."""
    lines = []

    # Vote evidence
    if vote_notes:
        lines.append("")
        lines.append("VOTE EVIDENCE")
        lines.append("~~~~~~~~~~~~~")
        lines.append("")
        for num, msg in vote_notes:
            lines.append(f"[{num}] - {msg}")

    # Validation evidence
    if validation_analysis.validation_evidence_by_round[round_number]:
        lines.append("")
        lines.append("VALIDATION EVIDENCE")
        lines.append("~~~~~~~~~~~~~~~~~~~")
        lines.append("")
        for num, msg in validation_analysis.validation_evidence_by_round[round_number]:
            lines.append(f"[{num}] - {msg}")

    return lines


def render_single_round_section(
    round_number: int,
    consensus_data: ConsensusData,
    vote_analysis: VoteAnalysisResult,
    validation_analysis: ValidationAnalysisResult,
) -> List[str]:
    """Render a complete round section with header, summary, node details, and evidence."""
    section_lines = []

    # Collect round metadata
    metadata = collect_round_metadata(round_number, consensus_data)

    # Render header and summary
    section_lines.extend(render_round_header(round_number))
    section_lines.extend(render_round_summary_table(metadata))

    # Build and render node details table
    table_lines, vote_notes = build_node_details_table(
        round_number, metadata, consensus_data, vote_analysis, validation_analysis
    )
    section_lines.extend(table_lines)

    # Render evidence sections
    section_lines.extend(render_evidence_sections(round_number, vote_notes, validation_analysis))

    section_lines.append("")

    return section_lines


def generate_full_consensus_report(
    consensus_data: ConsensusData,
    vote_analysis: VoteAnalysisResult,
    validation_analysis: ValidationAnalysisResult,
) -> str:
    """Orchestrate full report generation: title, nodes section, all rounds sections, footer."""
    report_lines = []

    # Report title
    report_lines.append(f"CONSENSUS REPORT — BLOCK HEIGHT {consensus_data.block_height}")
    report_lines.append("=" * len(report_lines[-1]))
    report_lines.append("")

    # Nodes summary section
    report_lines.extend(
        render_validator_nodes_summary_section(
            consensus_data.validator_ids, consensus_data.namespace_by_validator_id
        )
    )

    # All rounds sections
    for round_number in consensus_data.consensus_rounds:
        report_lines.extend(
            render_single_round_section(
                round_number, consensus_data, vote_analysis, validation_analysis
            )
        )

    # Report footer
    report_lines.append("END OF REPORT")

    return "\n".join(report_lines)


def main() -> int:
    if len(sys.argv) != 4:
        print(
            "Usage: python3 generate_canonical_report_v1_2.py <logs.json> <height> <output.txt>",
            file=sys.stderr,
        )
        return 2

    logs_file_path, block_height, output_file_path = sys.argv[1], sys.argv[2], sys.argv[3]

    # Stage 1: Load and filter log entries for the specified block height
    filtered_log_entries = load_and_filter_log_entries_for_height(logs_file_path, block_height)

    if not filtered_log_entries:
        Path(output_file_path).write_text(
            f"No log entries found for height {block_height}\n", encoding="utf-8"
        )
        return 0

    # Stage 2: Build indexed data structures for efficient lookups
    consensus_data = build_indexed_consensus_data(filtered_log_entries, block_height)

    # Stage 3: Analyze votes and validation across all rounds
    vote_analysis_result = extract_vote_messages_for_all_rounds(consensus_data)
    validation_analysis_result = collect_validation_evidence_for_all_rounds(consensus_data)

    # Stage 4: Generate the formatted consensus report
    consensus_report_text = generate_full_consensus_report(
        consensus_data, vote_analysis_result, validation_analysis_result
    )

    # Stage 5: Write report to output file
    Path(output_file_path).write_text(consensus_report_text, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
