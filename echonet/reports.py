from __future__ import annotations

import logging
import re
from collections import defaultdict
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any, Callable, Sequence, TypeVar

from echonet import helpers
from echonet.echonet_types import CONFIG, RevertErrorInfo
from echonet.report_models import SnapshotModel

T = TypeVar("T")


def format_if_present(f: Callable[[T], str], v: T | None) -> str:
    return f(v) if v else "(unknown)"


def _severity_for_count(count: int) -> str:
    """Map a non-negative count to a severity label."""
    if count == 0:
        return "neutral"
    return "bad" if count >= CONFIG.severity.bad_count_threshold else "warn"


def _format_percent(part: int, total: int) -> str:
    if total == 0:
        return "0.00%"
    return f"{part / total:.2%}"


def _format_rate(numerator: int, denom_seconds: int | None, unit: str) -> str:
    if denom_seconds is None or denom_seconds <= 0:
        return "(n/a)"
    return f"{numerator / denom_seconds:.2f} {unit}"


def _format_duration(seconds: int | None) -> str:
    if seconds is None:
        return "(unknown)"
    s = seconds
    if s < 60:
        return f"{s}s"
    m, s = divmod(s, 60)
    if m < 60:
        return f"{m}m {s}s"
    h, m = divmod(m, 60)
    if h < 48:
        return f"{h}h {m}m"
    d, h = divmod(h, 24)
    return f"{d}d {h}h"


def append_key_value_lines(
    lines: list[str], items: Sequence[tuple[str, str]], pad: int = 26
) -> None:
    for k, v in items:
        lines.append(f"{k:<{pad}} {v}")


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
        r = _SnapshotReportRollup.from_snapshot(s)
        lines: list[str] = []

        lines.append("=== Echonet snapshot ===")
        append_key_value_lines(
            lines,
            [
                ("Initial start block", format_if_present(str, s.initial_start_block)),
                ("Current start block", format_if_present(str, s.current_start_block)),
                ("Current block", format_if_present(str, s.current_block)),
                (
                    "First block timestamp",
                    format_if_present(helpers.timestamp_to_iso, s.first_block_timestamp),
                ),
                (
                    "Latest block timestamp",
                    format_if_present(helpers.timestamp_to_iso, s.latest_block_timestamp),
                ),
                (
                    "Time span (first->latest)",
                    format_if_present(
                        lambda secs: f"{secs}s ({_format_duration(secs)})",
                        s.timestamp_diff_seconds,
                    ),
                ),
                ("Blocks processed", format_if_present(str, s.blocks_sent_count)),
                (
                    "Forward rate",
                    r.forward_rate,
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
                    f"{s.pending_commission_count} ({_format_percent(s.pending_commission_count, s.pending_total_count)})",
                ),
                (
                    "Gateway errors",
                    f"{r.gateway_errors_count} ({_format_percent(r.gateway_errors_count, r.total_sent)})",
                ),
                (
                    "Reverted only on Mainnet",
                    f"{r.reverts_mainnet_count} ({_format_percent(r.reverts_mainnet_count, r.total_sent)})",
                ),
                (
                    "Reverted only on Echonet",
                    f"{r.reverts_echonet_count} ({_format_percent(r.reverts_echonet_count, r.total_sent)})",
                ),
                (
                    "L2 gas mismatches",
                    f"{r.l2_gas_mismatches_count} ({_format_percent(r.l2_gas_mismatches_count, r.total_sent)})",
                ),
                ("Block hash mismatches", str(r.block_hash_mismatches_count)),
                (
                    "Transaction mismatches",
                    str(r.transaction_commitment_mismatches_count),
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
            total_pending = len(s.sent_tx_hashes)
            for tx_hash, src_bn in list(s.sent_tx_hashes.items())[:10]:
                lines.append(f"{tx_hash} @ {src_bn}")
            if total_pending > 10:
                lines.append(f"... ({total_pending} total)")

        lines.append("")
        lines.append("=== Resync triggers (first failures) ===")
        if not s.resync_causes:
            lines.append("(none)")
        else:
            for tx_hash, meta in sorted(
                s.resync_causes.items(), key=lambda kv: kv[1]["failure_block_number"]
            ):
                lines.append(
                    f"{tx_hash}: failure_block_number={meta['failure_block_number']} "
                    f"revert_target_block_number={meta['revert_target_block_number']} "
                    f"reason={meta['reason']} count={meta['count']}"
                )

        lines.append("")
        lines.append("=== Certain failures (repeated triggers) ===")
        if not s.certain_failures:
            lines.append("(none)")
        else:
            for tx_hash, meta in sorted(
                s.certain_failures.items(), key=lambda kv: kv[1]["failure_block_number"]
            ):
                lines.append(
                    f"{tx_hash}: failure_block_number={meta['failure_block_number']} "
                    f"revert_target_block_number={meta['revert_target_block_number']} "
                    f"reason={meta['reason']} count={meta['count']}"
                )

        lines.append("")
        lines.append("=== L2 gas mismatches ===")
        if not s.l2_gas_mismatches:
            lines.append("(none)")
        else:
            for meta in s.l2_gas_mismatches:
                lines.append(
                    f"{meta['tx_hash']}: echo_bn={meta['echo_block']} src_bn={meta['source_block']} "
                    f"blob_total_gas_l2={meta['blob_total_gas_l2']} "
                    f"fgw_total_gas_consumed_l2={meta['fgw_total_gas_consumed_l2']}"
                )

        lines.append("")
        lines.append("=== Block hash mismatches ===")
        if not s.block_hash_mismatches:
            lines.append("(none)")
        else:
            for entry in s.block_hash_mismatches:
                lines.append(
                    f"block={entry['block_number']} echonet={entry['echonet']} mainnet={entry['mainnet']}"
                )

        lines.append("")
        lines.append("=== Transaction commitment mismatches ===")
        if not s.transaction_commitment_mismatches:
            lines.append("(none)")
        else:
            for entry in s.transaction_commitment_mismatches:
                lines.append(
                    f"block={entry['block_number']} echonet={entry['echonet']} mainnet={entry['mainnet']}"
                )

        return "\n".join(lines).rstrip() + "\n"


def filter_mainnet_reverts_for_reporting(snapshot: SnapshotModel) -> dict[str, RevertErrorInfo]:
    """
    Filter mainnet-only reverts for *reporting* (UI/text/pre-resync reports).
    """
    cutoff_bn = next(iter(snapshot.sent_tx_hashes.values()), None)
    if cutoff_bn is None:
        return dict(snapshot.revert_errors_mainnet)
    return {
        tx_hash: info
        for tx_hash, info in snapshot.revert_errors_mainnet.items()
        if info["block_number"] <= cutoff_bn
    }


@dataclass(frozen=True, slots=True)
class RevertRule:
    name: str
    predicate: Callable[[str], bool]


class RevertClassifier:
    """Classify revert errors into coarse-grained buckets."""

    _WRAPPER_REASONS_LOWER = frozenset({"argent/multicall-failed", "entrypoint_failed"})
    _reason_in_parentheses_regex = re.compile(r"\('([^']*)'\)")
    _reason_in_quotes_regex = re.compile(r'"([^"]+)"')

    def __init__(self, rules: list[RevertRule] | None = None) -> None:
        self._rules = rules or self._default_rules()

    @staticmethod
    def _select_specific_reason(candidates: Sequence[str]) -> str | None:
        for s in candidates:
            t = s.strip()
            if t and t.lower() not in RevertClassifier._WRAPPER_REASONS_LOWER:
                return t
        return None

    def extract_revert_reason(self, raw: str) -> str:
        if not raw:
            return ""

        lines = [ln.strip() for ln in raw.splitlines() if ln.strip()]
        if not lines:
            return raw

        for line in reversed(lines):
            for extractor in (self._reason_in_quotes_regex, self._reason_in_parentheses_regex):
                hit = self._select_specific_reason(extractor.findall(line))
                if hit:
                    return hit
        return lines[-1]

    def classify(self, revert_reason: str) -> str:
        if not revert_reason:
            return "Unknown"
        for rule in self._rules:
            if rule.predicate(revert_reason):
                return rule.name
        return "Other"

    def group(self, reverts: dict[str, RevertErrorInfo]) -> dict[str, list[tuple[str, int, str]]]:
        grouped: dict[str, list[tuple[str, int, str]]] = defaultdict(list)
        for tx_hash, info in reverts.items():
            revert_reason = self.extract_revert_reason(info["error"])
            grouped[self.classify(revert_reason)].append(
                (tx_hash, info["block_number"], revert_reason)
            )
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
            RevertRule("NEGATIVE", lambda m: "NEGATIVE:" in m),
            RevertRule("Not player", lambda m: "not player" in m),
            RevertRule("Player not won last round", lambda m: "player not won last round" in lc(m)),
            RevertRule(
                "Attestation out of window", lambda m: "attestation is out of window" in lc(m)
            ),
            RevertRule(
                "Attestation wrong block hash",
                lambda m: "attestation with wrong block hash" in lc(m),
            ),
            RevertRule(
                "Attestation is done for this epoch",
                lambda m: "attestation is done for this epoch" in lc(m),
            ),
            RevertRule("Insufficient max L2Gas", lambda m: "insufficient max l2gas" in lc(m)),
            RevertRule(
                "ERC20: insufficient balance", lambda m: "erc20: insufficient balance" in lc(m)
            ),
            RevertRule(
                "insufficient unassigned stake", lambda m: "insufficient unassigned stake" in lc(m)
            ),
            RevertRule(
                "Caller is not owner of token", lambda m: "caller is not owner of token" in lc(m)
            ),
            RevertRule("ERC721: invalid token ID", lambda m: "erc721: invalid token id" in lc(m)),
            RevertRule("Insufficient from_token", lambda m: "insufficient from_token" in lc(m)),
            RevertRule("Tile already minted", lambda m: "tile already minted" in lc(m)),
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

    def render(
        self,
        mainnet_reverts: dict[str, RevertErrorInfo],
        echonet_reverts: dict[str, RevertErrorInfo],
    ) -> str:
        total_mainnet = len(mainnet_reverts)
        total_echonet = len(echonet_reverts)

        lines: list[str] = []
        lines.append("=== Revert counts ===")
        lines.append(f"Only on Mainnet: {total_mainnet}")
        lines.append(f"Only on Echonet: {total_echonet}")
        lines.append("")
        lines.extend(self._render_section("Reverted only on Mainnet", mainnet_reverts))
        lines.append("")
        lines.extend(self._render_section("Reverted only on Echonet", echonet_reverts))
        return "\n".join(lines).rstrip() + "\n"

    def _render_section(self, title: str, reverts: dict[str, RevertErrorInfo]) -> list[str]:
        grouped = self._classifier.group(reverts)
        total = sum(len(v) for v in grouped.values())
        lines: list[str] = [f"=== {title} ({total} txs) ==="]
        if total == 0:
            lines.append("(none)")
            return lines

        for error_type, txs in sorted(grouped.items(), key=lambda kv: (-len(kv[1]), kv[0])):
            lines.append("")
            lines.append(f"-- {error_type} ({len(txs)} txs, {_format_percent(len(txs), total)}) --")
            for tx_hash, src_bn, revert_reason in sorted(txs, key=lambda x: x[1]):  # src_bn
                lines.append(f"bn={str(src_bn)} {tx_hash}: {revert_reason}")
        return lines


class PreResyncReportWriter:
    """Write snapshot + revert comparison reports to `log_dir` using consistent headers."""

    def __init__(self, log_dir: Path, classifier: RevertClassifier | None = None) -> None:
        self._log_dir = log_dir
        self._classifier = classifier or RevertClassifier()

    def write(
        self, context: PreResyncContext, snapshot: SnapshotModel, logger: logging.Logger
    ) -> None:
        header = self._header(context)

        snapshot_text = header + SnapshotTextReport(snapshot).render()
        reverts_text = header + RevertComparisonTextReport(classifier=self._classifier).render(
            mainnet_reverts=filter_mainnet_reverts_for_reporting(snapshot),
            echonet_reverts=snapshot.revert_errors_echonet,
        )

        logger.info(f"Echonet report snapshot before resync:\n{snapshot_text.rstrip()}")

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
    logger: logging.Logger,
) -> None:
    context = PreResyncContext.create(
        trigger_tx_hash=trigger_tx_hash, trigger_block=trigger_block, trigger_reason=trigger_reason
    )
    PreResyncReportWriter(log_dir=CONFIG.paths.log_dir).write(
        context=context, snapshot=snapshot, logger=logger
    )


@dataclass(frozen=True, slots=True)
class RevertRow:
    tx_hash: str
    block_number: int
    reason: str
    raw_error: str


@dataclass(frozen=True, slots=True)
class RevertGroup:
    name: str
    count: int
    pct_of_section: str
    rows: list[RevertRow]


def _group_reverts(
    reverts: dict[str, RevertErrorInfo],
    *,
    classifier: RevertClassifier,
) -> list[RevertGroup]:
    grouped: dict[str, list[RevertRow]] = defaultdict(list)
    for tx_hash, info in reverts.items():
        reason = classifier.extract_revert_reason(info["error"])
        grouped[classifier.classify(reason)].append(
            RevertRow(
                tx_hash=str(tx_hash),
                block_number=info["block_number"],
                reason=reason,
                raw_error=info["error"],
            )
        )

    total = sum(len(rows) for rows in grouped.values())

    out = [
        RevertGroup(
            name=group_name,
            count=len(rows),
            pct_of_section=_format_percent(len(rows), total),
            rows=sorted(rows, key=lambda r: (r.block_number, r.tx_hash)),
        )
        for group_name, rows in grouped.items()
    ]
    return sorted(out, key=lambda g: (-g.count, g.name))


@dataclass(frozen=True, slots=True)
class _SnapshotReportRollup:
    s: SnapshotModel

    total_sent: int
    pending_total: int
    committed: int
    pending_commission: int

    gateway_errors_count: int
    reverts_mainnet_count: int
    reverts_echonet_count: int
    resync_causes_count: int
    certain_failures_count: int
    l2_gas_mismatches_count: int
    block_hash_mismatches_count: int
    transaction_commitment_mismatches_count: int

    forward_rate: str
    echonet_revert_rate_pct: float
    echonet_revert_risk_0_1: float
    now_utc: str

    sev: dict[str, str]

    pending_txs: list[tuple[str, int]]
    gateway_errors_rows: list[dict[str, Any]]
    grouped_mainnet: list[RevertGroup]
    grouped_echonet: list[RevertGroup]
    resync_causes_rows: list[dict[str, Any]]
    certain_failures_rows: list[dict[str, Any]]
    l2_gas_mismatches_rows: list[dict[str, Any]]
    block_hash_mismatches_rows: list[dict[str, Any]]
    transaction_commitment_mismatches_rows: list[dict[str, Any]]

    @classmethod
    def from_snapshot(cls, s: SnapshotModel) -> "_SnapshotReportRollup":
        classifier = RevertClassifier()

        total_sent = s.total_sent_tx_count
        pending_total = s.pending_total_count
        committed = s.committed_count
        pending_commission = s.pending_commission_count

        gateway_errors_count = len(s.gateway_errors)
        mainnet_reverts_filtered = filter_mainnet_reverts_for_reporting(s)
        reverts_mainnet_count = len(mainnet_reverts_filtered)
        reverts_echonet_count = len(s.revert_errors_echonet)
        resync_causes_count = len(s.resync_causes)
        certain_failures_count = len(s.certain_failures)
        l2_gas_mismatches_count = len(s.l2_gas_mismatches)
        block_hash_mismatches_count = len(s.block_hash_mismatches)
        transaction_commitment_mismatches_count = len(s.transaction_commitment_mismatches)

        sev = {
            "pending": "neutral",
            "committed": ("ok" if committed > 0 else "neutral"),
            "gateway_errors": _severity_for_count(gateway_errors_count),
            "reverts_mainnet": _severity_for_count(reverts_mainnet_count),
            "reverts_echonet": _severity_for_count(reverts_echonet_count),
            "resync_causes": ("bad" if resync_causes_count > 0 else "neutral"),
            "certain_failures": ("bad" if certain_failures_count > 0 else "neutral"),
            "l2_gas_mismatches": _severity_for_count(l2_gas_mismatches_count),
            "block_hash_mismatches": _severity_for_count(block_hash_mismatches_count),
            "transaction_commitment_mismatches": _severity_for_count(
                transaction_commitment_mismatches_count
            ),
        }

        pending_txs = [(tx_hash, src_bn) for tx_hash, src_bn in s.sent_tx_hashes.items()]

        gateway_errors_rows = [
            {
                "tx_hash": str(tx_hash),
                "status": payload["status"],
                "block_number": payload["block_number"],
                "response": str(payload["response"]),
            }
            for tx_hash, payload in s.gateway_errors.items()
        ]

        grouped_mainnet = _group_reverts(dict(mainnet_reverts_filtered), classifier=classifier)
        grouped_echonet = _group_reverts(dict(s.revert_errors_echonet), classifier=classifier)

        resync_causes_rows = sorted(
            (dict(v) for v in s.resync_causes.values()),
            key=lambda x: (x["failure_block_number"], x["tx_hash"]),
        )
        certain_failures_rows = sorted(
            (dict(v) for v in s.certain_failures.values()),
            key=lambda x: (-x["count"], x["failure_block_number"]),
        )
        l2_gas_mismatches_rows = [dict(v) for v in s.l2_gas_mismatches]
        block_hash_mismatches_rows = [dict(v) for v in s.block_hash_mismatches]
        transaction_commitment_mismatches_rows = [
            dict(v) for v in s.transaction_commitment_mismatches
        ]
        forward_rate = _format_rate(total_sent, s.uptime_seconds, unit="TPS")
        echonet_revert_rate_pct = (
            (100.0 * reverts_echonet_count / total_sent) if total_sent > 0 else 0.0
        )
        echonet_revert_risk_0_1 = min(echonet_revert_rate_pct, 1.0)

        return cls(
            s=s,
            total_sent=total_sent,
            pending_total=pending_total,
            committed=committed,
            pending_commission=pending_commission,
            gateway_errors_count=gateway_errors_count,
            reverts_mainnet_count=reverts_mainnet_count,
            reverts_echonet_count=reverts_echonet_count,
            resync_causes_count=resync_causes_count,
            certain_failures_count=certain_failures_count,
            l2_gas_mismatches_count=l2_gas_mismatches_count,
            block_hash_mismatches_count=block_hash_mismatches_count,
            transaction_commitment_mismatches_count=transaction_commitment_mismatches_count,
            forward_rate=forward_rate,
            echonet_revert_rate_pct=echonet_revert_rate_pct,
            echonet_revert_risk_0_1=echonet_revert_risk_0_1,
            now_utc=helpers.timestamp_to_iso(int(helpers.utc_now().timestamp())),
            sev=sev,
            pending_txs=pending_txs,
            gateway_errors_rows=gateway_errors_rows,
            grouped_mainnet=grouped_mainnet,
            grouped_echonet=grouped_echonet,
            resync_causes_rows=resync_causes_rows,
            certain_failures_rows=certain_failures_rows,
            l2_gas_mismatches_rows=l2_gas_mismatches_rows,
            block_hash_mismatches_rows=block_hash_mismatches_rows,
            transaction_commitment_mismatches_rows=transaction_commitment_mismatches_rows,
        )


def build_report_view_model(
    snapshot: SnapshotModel,
) -> dict[str, Any]:
    """Prepare a UI-friendly dict for the HTML report template."""
    r = _SnapshotReportRollup.from_snapshot(snapshot)

    return {
        "feeder_base_url": str(CONFIG.feeder.base_url).rstrip("/"),
        "snapshot": r.s,
        "meta": {
            "generated_at_utc": r.now_utc,
            "first_block_timestamp_iso": format_if_present(
                helpers.timestamp_to_iso, r.s.first_block_timestamp
            ),
            "latest_block_timestamp_iso": format_if_present(
                helpers.timestamp_to_iso, r.s.latest_block_timestamp
            ),
            "span_human": _format_duration(r.s.timestamp_diff_seconds),
            "forward_rate": r.forward_rate,
            "echonet_revert_rate_pct": round(r.echonet_revert_rate_pct, 3),
            "echonet_revert_risk_0_1": round(r.echonet_revert_risk_0_1, 6),
        },
        "kpis": {
            "total_sent_tx_count": r.total_sent,
            "committed_count": r.committed,
            "pending_commission_count": r.pending_commission,
            "pending_total_count": r.pending_total,
            "gateway_errors_count": r.gateway_errors_count,
            "reverts_mainnet_count": r.reverts_mainnet_count,
            "reverts_echonet_count": r.reverts_echonet_count,
            "resync_causes_count": r.resync_causes_count,
            "certain_failures_count": r.certain_failures_count,
            "l2_gas_mismatches_count": r.l2_gas_mismatches_count,
            "block_hash_mismatches_count": r.block_hash_mismatches_count,
            "transaction_commitment_mismatches_count": r.transaction_commitment_mismatches_count,
            "committed_pct_of_pending_total": _format_percent(r.committed, r.pending_total),
            "pending_pct_of_pending_total": _format_percent(r.pending_commission, r.pending_total),
        },
        "severity": r.sev,
        "progress": {
            "initial_start_block": format_if_present(str, r.s.initial_start_block),
            "current_start_block": format_if_present(str, r.s.current_start_block),
            "current_block": format_if_present(str, r.s.current_block),
            "blocks_processed": format_if_present(str, r.s.blocks_sent_count),
        },
        "pending_txs": r.pending_txs[:10],
        "pending_txs_total": len(r.pending_txs),
        "gateway_errors": r.gateway_errors_rows,
        "reverts": {
            "mainnet_only": {
                "total": r.reverts_mainnet_count,
                "pct_of_total_sent": _format_percent(r.reverts_mainnet_count, r.total_sent),
                "groups": r.grouped_mainnet,
            },
            "echonet_only": {
                "total": r.reverts_echonet_count,
                "pct_of_total_sent": _format_percent(r.reverts_echonet_count, r.total_sent),
                "groups": r.grouped_echonet,
            },
        },
        "resync": {
            "causes": r.resync_causes_rows,
            "certain_failures": r.certain_failures_rows,
        },
        "l2_gas_mismatches": r.l2_gas_mismatches_rows,
        "block_hash_mismatches": r.block_hash_mismatches_rows,
        "transaction_commitment_mismatches": r.transaction_commitment_mismatches_rows,
    }
