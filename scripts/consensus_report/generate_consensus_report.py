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
from collections import defaultdict
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional, Tuple

ROUND_RE = re.compile(r"\bround[=: ]\s*(\d+)\b")
PATRICIA_KEY_RE = re.compile(r"PatriciaKey\((0x[0-9a-fA-F]+)\)")
HEIGHT_BLOCK_RE_TEMPLATE = (
    r"\b(?:block[\s_]?number|block|height)\b\s*(?::|=|\bis\b)?\s*\(?['\"]?\b{}\b['\"]?\)?"
)
ADVANCING_STEP_RE = re.compile(r"Advancing step:\s*from\s*(\w+)\s*to\s*(\w+)", re.I)
BROADCAST_PREVOTE_RE = re.compile(r"Broadcasting Vote\s*\{[^}]*vote_type:\s*Prevote\b", re.I)
BROADCAST_PRECOMMIT_RE = re.compile(r"Broadcasting Vote\s*\{[^}]*vote_type:\s*Precommit\b", re.I)
BROADCAST_VOTE_PATTERNS = {
    "prevote": BROADCAST_PREVOTE_RE,
    "precommit": BROADCAST_PRECOMMIT_RE,
}
N_TXS_RE = re.compile(
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


@dataclass
class RoundMetadata:
    """Metadata collected for a round."""

    round_start_time: Optional[datetime]
    round_end_time: Optional[datetime]
    proposer: Optional[str]
    round_ntxs: str
    closing_reason: str


def final_weights_marker(height_str: str) -> str:
    return f"Block {height_str} final weights"


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

    marker = final_weights_marker(height_str)
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


def load_and_filter_log_entries_for_height(
    logs_file_path: str, block_height: str
) -> List[Dict[str, Any]]:
    """Load JSON log file, filter entries matching the block height, and sort by timestamp."""
    with open(logs_file_path, "r", encoding="utf-8") as f:
        data = json.load(f)

    filtered_entries = [
        entry
        for entry in data
        if height_match(entry, block_height) and parse_timestamp(entry) is not None
    ]
    filtered_entries.sort(key=lambda entry: parse_timestamp(entry))

    return filtered_entries


def build_indexed_consensus_data(
    filtered_log_entries: List[Dict[str, Any]], block_height: str
) -> ConsensusData:
    log_entries_by_namespace: Dict[str, List[Dict[str, Any]]] = defaultdict(list)
    namespace_by_validator_id: Dict[str, str] = {}
    validator_ids_set: set = set()
    consensus_rounds: set = set()
    log_entries_by_round_and_validator: Dict[Tuple[int, str], List[Dict[str, Any]]] = defaultdict(
        list
    )

    for entry in filtered_log_entries:
        ns = get_namespace(entry)
        validator_id = get_validator_id(entry)
        round_num = get_round(entry)

        if ns is not None:
            log_entries_by_namespace[ns].append(entry)
        if validator_id is not None:
            validator_ids_set.add(validator_id)
            if ns is not None and validator_id not in namespace_by_validator_id:
                namespace_by_validator_id[validator_id] = ns
        if round_num is not None:
            consensus_rounds.add(round_num)
            if validator_id is not None:
                log_entries_by_round_and_validator[(round_num, validator_id)].append(entry)

    return ConsensusData(
        all_log_entries=filtered_log_entries,
        log_entries_by_namespace=log_entries_by_namespace,
        log_entries_by_round_and_validator=log_entries_by_round_and_validator,
        namespace_by_validator_id=namespace_by_validator_id,
        validator_ids=sorted(validator_ids_set, key=short_id),
        consensus_rounds=sorted(consensus_rounds),
        block_height=block_height,
    )


def extract_vote_messages_for_all_rounds(consensus_data: ConsensusData) -> VoteAnalysisResult:
    """Find the first prevote/precommit broadcast after each advancing step transition."""
    vote_messages_by_round_validator_type: Dict[Tuple[int, str, str], str] = {}

    for (
        round_num,
        validator_id,
    ), entries_list in consensus_data.log_entries_by_round_and_validator.items():
        for i, entry in enumerate(entries_list):
            msg = get_message(entry)
            advancing_step = find_advancing_step(msg)
            if advancing_step is None:
                continue
            after_timestamp = parse_timestamp(entry)
            if after_timestamp is None:
                continue
            from_stage, to_stage = advancing_step[0].lower(), advancing_step[1].lower()

            if (from_stage == "propose" and to_stage == "prevote") or (
                from_stage == "prevote" and to_stage == "precommit"
            ):
                for entry2 in entries_list[i + 1 :]:
                    entry2_timestamp = parse_timestamp(entry2)
                    msg2 = get_message(entry2)
                    if (
                        entry2_timestamp is not None
                        and entry2_timestamp >= after_timestamp
                        and BROADCAST_VOTE_PATTERNS[to_stage].search(msg2)
                    ):
                        vote_messages_by_round_validator_type[
                            (round_num, validator_id, to_stage.capitalize())
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
        evidence_number = 1
        for validator_id in consensus_data.validator_ids:
            proposal_failed = proposal_failed_msg(
                consensus_data.log_entries_by_round_and_validator, validator_id, round_num
            )
            if proposal_failed is not None:
                validation_status_by_round_validator[round_num][
                    validator_id
                ] = f"Failed [{evidence_number}]"
                validation_evidence_by_round[round_num].append((evidence_number, proposal_failed))
                evidence_number += 1
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


def collect_round_metadata(round_number: int, consensus_data: ConsensusData) -> RoundMetadata:
    """Collect round timing and metadata."""
    rs = round_start(consensus_data.all_log_entries, round_number)
    re_ = round_end(consensus_data.all_log_entries, round_number)
    proposer = get_round_proposer_id(consensus_data.all_log_entries, round_number)

    round_ntxs = n_txs_for_round(
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


def main() -> int:
    print("not implemented", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
