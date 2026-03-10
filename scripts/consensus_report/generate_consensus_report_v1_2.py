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

import sys
from dataclasses import dataclass
from typing import Any, Dict, List, Tuple


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


def main() -> int:
    print("not implemented", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
