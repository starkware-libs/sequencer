"""
Reporting utilities for Echonet.
"""

from __future__ import annotations

import argparse
import json
import re
from dataclasses import dataclass
from datetime import datetime, timedelta
from pathlib import Path
from typing import Callable, Mapping, Sequence

import requests
from collections import defaultdict

from echonet import helpers
from echonet.echonet_types import CONFIG, JsonObject
from echonet.report_models import SnapshotModel


def _format_percent(part: int, total: int) -> str:
    if total == 0:
        return "0.00%"
    return f"{(100.0 * part / total):.2f}%"


def _format_rate(numerator: int, denom_seconds: int | None, unit: str) -> str:
    if denom_seconds is None or denom_seconds <= 0:
        return "(n/a)"
    return f"{(float(numerator) / float(denom_seconds)):.2f} {unit}"


def append_key_value_lines(
    lines: list[str], items: Sequence[tuple[str, str]], pad: int = 26
) -> None:
    for k, v in items:
        lines.append(f"{k:<{pad}} {v}")


class ReportHttpClient:
    """HTTP client for the Echonet `/echonet/report` and related debug endpoints."""

    def __init__(self, base_url: str, timeout_seconds: float = 5.0) -> None:
        self._base_url = base_url.rstrip("/")
        self._timeout_seconds = timeout_seconds

    def fetch_report_snapshot(self) -> JsonObject:
        url = f"{self._base_url}/echonet/report"
        resp = requests.get(url, timeout=self._timeout_seconds)
        resp.raise_for_status()
        return resp.json()

    def fetch_block_dump(self, block_number: int, kind: str) -> JsonObject:
        url = f"{self._base_url}/echonet/block_dump?blockNumber={block_number}&kind={kind}"
        resp = requests.get(url, timeout=self._timeout_seconds)
        resp.raise_for_status()
        return resp.json()


@dataclass(frozen=True, slots=True)
class PreResyncContext:
    """Metadata describing a pre-resync report bundle (what triggered it, and when)."""

    trigger_tx_hash: str
    trigger_block: int
    trigger_reason: str
    timestamp_utc: datetime

    @classmethod
    def create(
        cls, trigger_tx_hash: str, trigger_block: int, trigger_reason: str
    ) -> "PreResyncContext":
        return cls(
            trigger_tx_hash=trigger_tx_hash,
            trigger_block=trigger_block,
            trigger_reason=trigger_reason,
            timestamp_utc=helpers.utc_now(),
        )


class SnapshotTextReport:
    """Render the high-level snapshot report."""

    def __init__(self, snapshot: SnapshotModel) -> None:
        self._s = snapshot

    def render(self) -> str:
        s = self._s
        lines: list[str] = []

        lines.append("=== Echonet snapshot ===")
        append_key_value_lines(
            lines,
            [
                (
                    "Initial start block",
                    str(s.initial_start_block)
                    if s.initial_start_block is not None
                    else "(unknown)",
                ),
                (
                    "Current start block",
                    str(s.current_start_block)
                    if s.current_start_block is not None
                    else "(unknown)",
                ),
                (
                    "Current block",
                    str(s.current_block) if s.current_block is not None else "(unknown)",
                ),
                (
                    "First block timestamp",
                    helpers.timestamp_to_iso(s.first_block_timestamp)
                    if s.first_block_timestamp is not None
                    else "(unknown)",
                ),
                (
                    "Latest block timestamp",
                    helpers.timestamp_to_iso(s.latest_block_timestamp)
                    if s.latest_block_timestamp is not None
                    else "(unknown)",
                ),
                (
                    "Time span (first->latest)",
                    f"{s.timestamp_diff_seconds} sec ({str(timedelta(seconds=int(s.timestamp_diff_seconds)))})"
                    if s.timestamp_diff_seconds is not None
                    else "(unknown)",
                ),
                (
                    "Blocks processed",
                    str(s.blocks_sent_count) if s.blocks_sent_count is not None else "(unknown)",
                ),
                (
                    "Forward rate",
                    _format_rate(s.total_sent_tx_count, s.timestamp_diff_seconds, unit="tx/s"),
                ),
            ],
        )

        lines.append("")
        lines.append("=== Transaction status summary ===")
        append_key_value_lines(
            lines,
            [
                (
                    "Committed transactions",
                    f"{s.committed_count} ({_format_percent(s.committed_count, s.pending_total_count)})",
                ),
                (
                    "Pending transactions",
                    f"{s.pending_not_committed_count} ({_format_percent(s.pending_not_committed_count, s.pending_total_count)})",
                ),
                (
                    "Gateway errors (live)",
                    f"{len(s.gateway_errors)} ({_format_percent(len(s.gateway_errors), s.total_sent_tx_count)})",
                ),
                (
                    "Reverted only on Mainnet",
                    f"{len(s.revert_errors_mainnet)} ({_format_percent(len(s.revert_errors_mainnet), s.total_sent_tx_count)})",
                ),
                (
                    "Reverted only on Echonet",
                    f"{len(s.revert_errors_echonet)} ({_format_percent(len(s.revert_errors_echonet), s.total_sent_tx_count)})",
                ),
                ("Resync triggers (first failures)", str(len(s.resync_causes))),
                ("Certain failures (repeated triggers)", str(len(s.certain_failures))),
            ],
        )

        lines.append("")
        lines.append("=== Gateway errors (hash -> response) ===")
        if not s.gateway_errors:
            lines.append("(none)")
        else:
            for tx_hash, response in s.gateway_errors.items():
                lines.append(f"{tx_hash}: {response}")

        lines.append("")
        lines.append("=== Non-committed tx hashes ===")
        if not s.sent_tx_hashes:
            lines.append("(none)")
        else:
            for tx_hash, src_bn in sorted(s.sent_tx_hashes.items(), key=lambda kv: kv[1]):
                lines.append(f"{tx_hash} @ {src_bn}")

        lines.append("")
        lines.append("=== Resync triggers (first failures) ===")
        if not s.resync_causes:
            lines.append("(none)")
        else:
            for tx_hash, meta in sorted(
                s.resync_causes.items(), key=lambda kv: kv[1]["block_number"]
            ):
                lines.append(
                    f"{tx_hash}: bn={meta['block_number']} reason={meta['reason']} count={meta['count']}"
                )

        lines.append("")
        lines.append("=== Certain failures (repeated triggers) ===")
        if not s.certain_failures:
            lines.append("(none)")
        else:
            for tx_hash, meta in sorted(
                s.certain_failures.items(), key=lambda kv: kv[1]["block_number"]
            ):
                lines.append(
                    f"{tx_hash}: bn={meta['block_number']} reason={meta['reason']} count={meta['count']}"
                )

        return "\n".join(lines).rstrip() + "\n"


@dataclass(frozen=True, slots=True)
class RevertRule:
    name: str
    predicate: Callable[[str], bool]


class RevertClassifier:
    """
    Classify revert errors into coarse-grained buckets.
    """

    _leaf_paren = re.compile(r"\('([^']*)'\)")
    _leaf_quote = re.compile(r'"([^"]+)"')

    def __init__(self, rules: list[RevertRule] | None = None) -> None:
        self._rules = rules or self._default_rules()

    def leaf_message(self, raw: str) -> str:
        if not raw:
            return ""

        lines = [ln.strip() for ln in raw.splitlines() if ln.strip()]
        if not lines:
            return raw

        for line in reversed(lines):
            m = self._leaf_paren.search(line)
            if m and m.group(1):
                return m.group(1)
            m = self._leaf_quote.search(line)
            if m and m.group(1):
                return m.group(1)
        return lines[-1]

    def classify(self, leaf_message: str) -> str:
        if not leaf_message:
            return "Unknown"
        for rule in self._rules:
            if rule.predicate(leaf_message):
                return rule.name
        return "Other"

    def group(self, reverts: Mapping[str, str]) -> dict[str, list[tuple[str, str]]]:
        grouped: dict[str, list[tuple[str, str]]] = defaultdict(list)
        for tx_hash, raw_msg in reverts.items():
            leaf = self.leaf_message(raw_msg)
            grouped[self.classify(leaf)].append((tx_hash, leaf))
        return grouped

    @staticmethod
    def _default_rules() -> list[RevertRule]:
        def lc(s: str) -> str:
            return s.lower()

        return [
            RevertRule("CLEAR_AT_LEAST_MINIMUM", lambda m: "clear_at_least_minimum" in lc(m)),
            RevertRule("NO_PROFIT", lambda m: "no_profit" in lc(m)),
            RevertRule("INVALID_NONCE", lambda m: "invalid_nonce" in lc(m)),
            RevertRule(
                "Overflow",
                lambda m: "overflow" in lc(m)
                and any(x in lc(m) for x in ("u256_sub", "u128_sub", "u64_sub", "u32_sub")),
            ),
            RevertRule("Result::unwrap failed", lambda m: "result::unwrap failed" in lc(m)),
            RevertRule("NEGATIVE", lambda m: "negative:" in m),
            RevertRule("Not player", lambda m: "not player" in m),
            RevertRule("Player not won last round", lambda m: "player not won last round" in lc(m)),
            RevertRule(
                "Attestation out of window", lambda m: "attestation is out of window" in lc(m)
            ),
            RevertRule(
                "Attestation wrong block hash",
                lambda m: "attestation with wrong block hash" in lc(m),
            ),
            RevertRule("Invalid request ID", lambda m: "invalid request id" in lc(m)),
            RevertRule("argent/multicall-failed", lambda m: "argent/multicall-failed" in lc(m)),
            RevertRule("MIN_LIQUIDITY", lambda m: "min_liquidity" in lc(m)),
            RevertRule("EXCESSIVE_INPUT_AMOUNT", lambda m: "excessive_input_amount" in lc(m)),
            RevertRule("Invalid burn amount", lambda m: "invalid burn amount" in lc(m)),
            RevertRule(
                "Insufficient token balance", lambda m: "token from balance is too low" in lc(m)
            ),
            RevertRule("ASSERT_EQ failed", lambda m: "assert_eq" in lc(m)),
            RevertRule("Range check failed", lambda m: "range-check validation failed" in lc(m)),
            RevertRule("INVALID_PRICE_TIMESTAMP", lambda m: "invalid_price_timestamp" in lc(m)),
            RevertRule(
                "Out of steps",
                lambda m: "could not reach the end of the program" in lc(m)
                or "runresources has no remaining steps" in lc(m),
            ),
            RevertRule("AOTL/MSAOTL", lambda m: lc(m).strip() in ("aotl", "msaotl")),
            RevertRule("SPL_ZFO", lambda m: "spl_zfo" in lc(m)),
            RevertRule("Invariant violation", lambda m: "invariant" in lc(m)),
            RevertRule(
                "Insufficient max L1DataGas", lambda m: "insufficient max l1datagas" in lc(m)
            ),
            RevertRule("Hex code", lambda m: lc(m).startswith("0x") and len(lc(m)) < 16),
        ]


class RevertComparisonTextReport:
    def __init__(self, classifier: RevertClassifier) -> None:
        self._classifier = classifier

    def render(self, mainnet_reverts: Mapping[str, str], echonet_reverts: Mapping[str, str]) -> str:
        grouped_mainnet = self._classifier.group(mainnet_reverts)
        grouped_echonet = self._classifier.group(echonet_reverts)

        total_mainnet = sum(len(v) for v in grouped_mainnet.values())
        total_echonet = sum(len(v) for v in grouped_echonet.values())

        lines: list[str] = []
        lines.append("=== Revert counts ===")
        lines.append(f"Only on Mainnet: {total_mainnet}")
        lines.append(f"Only on Echonet: {total_echonet}")
        lines.append("")
        lines.extend(self._render_grouped("Reverted only on Mainnet", grouped_mainnet))
        lines.append("")
        lines.extend(self._render_grouped("Reverted only on Echonet", grouped_echonet))
        return "\n".join(lines).rstrip() + "\n"

    @staticmethod
    def _render_grouped(title: str, grouped: Mapping[str, list[tuple[str, str]]]) -> list[str]:
        total = sum(len(v) for v in grouped.values())
        lines: list[str] = [f"=== {title} ({total} txs) ==="]
        if total == 0:
            lines.append("(none)")
            return lines

        for error_type, txs in sorted(grouped.items(), key=lambda kv: (-len(kv[1]), kv[0])):
            lines.append("")
            lines.append(f"-- {error_type} ({len(txs)} txs, {_format_percent(len(txs), total)}) --")
            for tx_hash, leaf in sorted(txs, key=lambda x: x[0]):
                lines.append(f"{tx_hash}: {leaf}")
        return lines


class PreResyncReportWriter:
    """Write snapshot + revert comparison reports to `log_dir` using consistent headers."""

    def __init__(self, log_dir: Path, classifier: RevertClassifier | None = None) -> None:
        self._log_dir = log_dir
        self._classifier = classifier or RevertClassifier()

    def write(self, context: PreResyncContext, snapshot: SnapshotModel, logger=None) -> None:
        header = self._header(context)

        snapshot_text = header + SnapshotTextReport(snapshot).render()
        reverts_text = header + RevertComparisonTextReport(classifier=self._classifier).render(
            mainnet_reverts=snapshot.revert_errors_mainnet,
            echonet_reverts=snapshot.revert_errors_echonet,
        )

        if logger is not None:
            logger.info("Echonet report snapshot before resync:\n%s", snapshot_text.rstrip())

        self._log_dir.mkdir(parents=True, exist_ok=True)
        ts_suffix = context.timestamp_utc.strftime("%Y%m%dT%H%M%SZ")

        self._write_text(f"report_snapshot_{ts_suffix}.log", snapshot_text)
        self._write_text(f"report_reverts_{ts_suffix}.log", reverts_text)

    @staticmethod
    def _header(context: PreResyncContext) -> str:
        ts = context.timestamp_utc.isoformat().replace("+00:00", "Z")
        return "\n".join(
            [
                "===== Echonet reports before resync =====",
                f"timestamp: {ts}",
                f"trigger_tx_hash: {context.trigger_tx_hash}",
                f"trigger_block: {context.trigger_block}",
                f"trigger_reason: {context.trigger_reason}",
                "",
                "",
            ]
        )

    def _write_text(self, filename: str, content: str) -> None:
        path = self._log_dir / filename
        path.write_text(content + "\n===== End report =====\n", encoding="utf-8")


def write_pre_resync_reports(
    trigger_tx_hash: str,
    trigger_block: int,
    trigger_reason: str,
    snapshot: SnapshotModel,
    logger=None,
) -> None:
    context = PreResyncContext.create(
        trigger_tx_hash=trigger_tx_hash, trigger_block=trigger_block, trigger_reason=trigger_reason
    )
    PreResyncReportWriter(log_dir=CONFIG.paths.log_dir).write(
        context=context, snapshot=snapshot, logger=logger
    )


def print_snapshot(base_url: str) -> None:
    data = ReportHttpClient(base_url).fetch_report_snapshot()
    print(SnapshotTextReport(SnapshotModel.from_dict(data)).render(), end="")


def compare_reverts(base_url: str) -> None:
    data = ReportHttpClient(base_url).fetch_report_snapshot()
    s = SnapshotModel.from_dict(data)
    report = RevertComparisonTextReport(classifier=RevertClassifier())
    print(
        report.render(
            mainnet_reverts=s.revert_errors_mainnet, echonet_reverts=s.revert_errors_echonet
        ),
        end="",
    )


def show_block(base_url: str, block_number: int, kind: str) -> None:
    obj = ReportHttpClient(base_url).fetch_block_dump(block_number=block_number, kind=kind)
    print(f"=== Block {block_number} [{kind}] ===")
    print(json.dumps(obj, ensure_ascii=False, indent=2))


def main() -> None:
    parser = argparse.ArgumentParser(description="Echonet reports")
    parser.add_argument(
        "--snapshot",
        action="store_true",
        help="Fetch and print the in-memory tx snapshot from the running app",
    )
    parser.add_argument(
        "--compare-reverts",
        action="store_true",
        help="Compare revert errors from in-memory state (mainnet vs echonet)",
    )
    parser.add_argument(
        "--base-url",
        default="http://127.0.0.1",
        help="Echonet base URL (default: http://127.0.0.1).",
    )
    parser.add_argument(
        "--all", action="store_true", help="Run both snapshot and revert comparison"
    )
    parser.add_argument(
        "--show-block", type=int, help="Block number to fetch from the in-memory store"
    )
    parser.add_argument(
        "--kind",
        choices=["blob", "block", "state_update"],
        default="blob",
        help="Which payload to fetch for --show-block",
    )
    args = parser.parse_args()

    # If no explicit action flags provided, run --all by default.
    if not (args.snapshot or args.compare_reverts or args.all or args.show_block):
        args.all = True

    if args.snapshot or args.all:
        print_snapshot(args.base_url)
        if not args.all:
            return

    if args.compare_reverts:
        compare_reverts(args.base_url)
        return

    if args.show_block is not None:
        show_block(args.base_url, args.show_block, args.kind)
        return


if __name__ == "__main__":
    main()
