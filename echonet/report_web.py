from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any

from echonet.echonet_types import JsonObject, RevertErrorInfo
from echonet.report_models import SnapshotModel
from echonet.reports import RevertClassifier


def _severity_for_count(
    count: int,
    *,
    zero: str = "neutral",
    nonzero: str = "warn",
    bad_threshold: int | None = None,
) -> str:
    c = int(count)
    if c <= 0:
        return zero
    if bad_threshold is not None and c >= int(bad_threshold):
        return "bad"
    return nonzero


def _fmt_int(v: int | None) -> str:
    return "(unknown)" if v is None else str(int(v))


def _fmt_percent(part: int, total: int) -> str:
    if total <= 0:
        return "0.00%"
    return f"{part / total:.2%}"


def _fmt_rate(numerator: int, denom_seconds: int | None, unit: str) -> str:
    if denom_seconds is None or denom_seconds <= 0:
        return "(n/a)"
    return f"{numerator / denom_seconds:.2f} {unit}"


def _ts_to_iso(ts_seconds: int | None) -> str:
    if ts_seconds is None:
        return "(unknown)"
    dt = datetime.fromtimestamp(int(ts_seconds), tz=timezone.utc)
    # Keep it compact + consistent, always UTC.
    return dt.isoformat(timespec="seconds").replace("+00:00", "Z")


def _fmt_duration(seconds: int | None) -> str:
    if seconds is None or seconds < 0:
        return "(n/a)"
    s = int(seconds)
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


def _sorted_items(d: dict[str, Any]) -> list[tuple[str, Any]]:
    return sorted(d.items(), key=lambda kv: str(kv[0]))


def _sorted_tx_hashes_with_src_bn(sent_tx_hashes: JsonObject) -> list[tuple[str, int]]:
    items: list[tuple[str, int]] = []
    for k, v in sent_tx_hashes.items():
        try:
            items.append((str(k), int(v)))
        except Exception:
            # Keep weird entries visible rather than crashing the UI.
            items.append((str(k), -1))
    return sorted(items, key=lambda kv: (kv[1], kv[0]))


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
    max_rows_per_group: int = 200,
) -> list[RevertGroup]:
    grouped: dict[str, list[RevertRow]] = {}
    for tx_hash, info in reverts.items():
        raw = str(info.get("error") or "")
        reason = classifier.extract_revert_reason(raw)
        group = classifier.classify(reason)
        try:
            bn = int(info.get("block_number"))  # type: ignore[arg-type]
        except Exception:
            bn = -1
        grouped.setdefault(group, []).append(
            RevertRow(tx_hash=str(tx_hash), block_number=bn, reason=reason, raw_error=raw)
        )

    total = sum(len(v) for v in grouped.values())
    out: list[RevertGroup] = []
    for group_name, rows in grouped.items():
        rows_sorted = sorted(rows, key=lambda r: (r.block_number, r.tx_hash))
        out.append(
            RevertGroup(
                name=group_name,
                count=len(rows_sorted),
                pct_of_section=_fmt_percent(len(rows_sorted), total),
                rows=rows_sorted[: max(0, int(max_rows_per_group))],
            )
        )
    out.sort(key=lambda g: (-g.count, g.name))
    return out


def build_report_view_model(
    snapshot: SnapshotModel, diagnostics: JsonObject | None = None
) -> dict[str, Any]:
    """
    Prepare a UI-friendly dict for the HTML report template.

    Keep this logic pure (no Flask imports) so it can be tested/reused easily.
    """
    s = snapshot
    classifier = RevertClassifier()

    gateway_errors = (
        s.gateway_errors if isinstance(s.gateway_errors, dict) else dict(s.gateway_errors)
    )
    gateway_errors_rows: list[dict[str, Any]] = []
    for tx_hash, payload in _sorted_items(gateway_errors):
        p = payload if isinstance(payload, dict) else {}
        status = p.get("status")
        resp = p.get("response")
        bn = p.get("block_number")
        status_int: int | None = None
        try:
            if isinstance(status, int):
                status_int = int(status)
            elif isinstance(status, str) and status.strip().isdigit():
                status_int = int(status.strip())
        except Exception:
            status_int = None

        if status_int is None:
            sev = "warn"
        elif status_int >= 500:
            sev = "bad"
        elif status_int >= 400:
            sev = "warn"
        else:
            sev = "ok"

        gateway_errors_rows.append(
            {
                "tx_hash": str(tx_hash),
                "status": status_int if status_int is not None else status,
                "block_number": int(bn)
                if isinstance(bn, int) or (isinstance(bn, str) and str(bn).isdigit())
                else bn,
                "response": "" if resp is None else str(resp),
                "severity": sev,
            }
        )

    # Reverts (mainnet-only vs echonet-only) are already segregated in SharedContext.
    mainnet_reverts = dict(s.revert_errors_mainnet)
    echonet_reverts = dict(s.revert_errors_echonet)
    grouped_mainnet = _group_reverts(mainnet_reverts, classifier=classifier)
    grouped_echonet = _group_reverts(echonet_reverts, classifier=classifier)

    resync_causes_rows = sorted(
        (dict(v) for v in (s.resync_causes or {}).values()),
        key=lambda x: (int(x.get("block_number", 0)), str(x.get("tx_hash", ""))),
    )
    certain_failures_rows = sorted(
        (dict(v) for v in (s.certain_failures or {}).values()),
        key=lambda x: (-int(x.get("count", 0)), int(x.get("block_number", 0))),
    )

    total_sent = int(s.total_sent_tx_count)
    pending_total = int(s.pending_total_count)
    committed = int(s.committed_count)
    pending_commission = int(s.pending_commission_count)

    # This is close to reports.py's "Forward rate" (tx/s over timestamp span).
    forward_rate = _fmt_rate(total_sent, s.timestamp_diff_seconds, unit="tx/s")

    gateway_errors_count = len(gateway_errors_rows)
    reverts_mainnet_count = len(mainnet_reverts)
    reverts_echonet_count = len(echonet_reverts)
    resync_causes_count = len(s.resync_causes or {})
    certain_failures_count = len(s.certain_failures or {})

    diag = diagnostics or {}
    ts_mismatches_recent = list(diag.get("timestamp_mismatches_recent") or [])
    l2_gas_mismatches_recent = list(diag.get("l2_gas_mismatches_recent") or [])
    ts_mismatches_total = int(diag.get("timestamp_mismatches_total") or 0)
    l2_gas_mismatches_total = int(diag.get("l2_gas_mismatches_total") or 0)

    echonet_revert_rate_pct = (
        (100.0 * reverts_echonet_count / total_sent) if total_sent > 0 else 0.0
    )
    # 0% => green, 5%+ => red (clamped).
    echonet_revert_risk_0_1 = min(max(echonet_revert_rate_pct / 1.0, 0.0), 1.0)

    # Severity thresholds requested by Ron:
    # - pending < 15 => green, else red
    # - resync/failure => red if non-zero
    # - gateway/reverts => yellow if non-zero, red if > 10
    sev = {
        # Transaction lifecycle
        "pending": ("neutral" if pending_commission < 15 else "bad"),
        "committed": ("ok" if committed > 0 else "neutral"),
        # Alerts
        "gateway_errors": _severity_for_count(
            gateway_errors_count, nonzero="warn", bad_threshold=11
        ),
        "reverts_mainnet": _severity_for_count(
            reverts_mainnet_count, nonzero="warn", bad_threshold=11
        ),
        "reverts_echonet": _severity_for_count(
            reverts_echonet_count, nonzero="warn", bad_threshold=11
        ),
        "resync_causes": ("bad" if resync_causes_count > 0 else "neutral"),
        "certain_failures": ("bad" if certain_failures_count > 0 else "neutral"),
        "timestamp_mismatches": _severity_for_count(
            ts_mismatches_total, nonzero="warn", bad_threshold=11
        ),
        "l2_gas_mismatches": _severity_for_count(
            l2_gas_mismatches_total, nonzero="warn", bad_threshold=11
        ),
    }

    return {
        "snapshot": s,
        "meta": {
            "generated_at_utc": datetime.now(tz=timezone.utc)
            .isoformat(timespec="seconds")
            .replace("+00:00", "Z"),
            "first_block_timestamp_iso": _ts_to_iso(s.first_block_timestamp),
            "latest_block_timestamp_iso": _ts_to_iso(s.latest_block_timestamp),
            "span_human": _fmt_duration(s.timestamp_diff_seconds),
            "forward_rate": forward_rate,
            "echonet_revert_rate_pct": round(echonet_revert_rate_pct, 3),
            "echonet_revert_risk_0_1": round(echonet_revert_risk_0_1, 6),
        },
        "kpis": {
            "total_sent_tx_count": total_sent,
            "committed_count": committed,
            "pending_commission_count": pending_commission,
            "pending_total_count": pending_total,
            "gateway_errors_count": gateway_errors_count,
            "reverts_mainnet_count": reverts_mainnet_count,
            "reverts_echonet_count": reverts_echonet_count,
            "resync_causes_count": resync_causes_count,
            "certain_failures_count": certain_failures_count,
            "timestamp_mismatches_count": ts_mismatches_total,
            "l2_gas_mismatches_count": l2_gas_mismatches_total,
            "committed_pct_of_pending_total": _fmt_percent(committed, pending_total),
            "pending_pct_of_pending_total": _fmt_percent(pending_commission, pending_total),
        },
        "severity": sev,
        "progress": {
            "initial_start_block": _fmt_int(s.initial_start_block),
            "current_start_block": _fmt_int(s.current_start_block),
            "current_block": _fmt_int(s.current_block),
            "blocks_processed": _fmt_int(s.blocks_sent_count),
        },
        "pending_txs": _sorted_tx_hashes_with_src_bn(s.sent_tx_hashes),
        "gateway_errors": gateway_errors_rows,
        "reverts": {
            "mainnet_only": {
                "total": len(mainnet_reverts),
                "pct_of_total_sent": _fmt_percent(len(mainnet_reverts), total_sent),
                "groups": grouped_mainnet,
            },
            "echonet_only": {
                "total": len(echonet_reverts),
                "pct_of_total_sent": _fmt_percent(len(echonet_reverts), total_sent),
                "groups": grouped_echonet,
            },
        },
        "resync": {
            "causes": resync_causes_rows,
            "certain_failures": certain_failures_rows,
        },
        "diagnostics": {
            "timestamp_mismatches_total": ts_mismatches_total,
            "l2_gas_mismatches_total": l2_gas_mismatches_total,
            "timestamp_mismatches_recent": ts_mismatches_recent,
            "l2_gas_mismatches_recent": l2_gas_mismatches_recent,
        },
        # Helpful constants for template logic.
        "limits": {
            "max_rows_per_revert_group": 200,
        },
    }
