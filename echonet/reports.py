import argparse
import json
import sys

import consts
import requests


def print_snapshot(endpoint: str) -> None:
    data = _fetch_report_data(endpoint)
    _print_snapshot_from_data(data)


def _print_snapshot_from_data(data: dict) -> None:
    # High-level metadata
    initial_start_block = data.get("initial_start_block")
    current_start_block = data.get("current_start_block")
    current_block = data.get("current_block")
    if initial_start_block is not None:
        print(f"Initial start block: {initial_start_block}")
    if current_start_block is not None:
        print(f"Current start block: {current_start_block}")
    if current_block is not None:
        print(f"Current block: {current_block}")
    # Counters and derived metrics
    total_sent = data.get("total_sent_tx_count") or 0
    print(f"Total transactions forwarded: {total_sent}")
    blocks_sent = data.get("blocks_sent_count")
    if blocks_sent is not None:
        print(f"Blocks processed (current-initial): {blocks_sent}")
    # Timestamp diff between initial and current block
    ts_diff = data.get("timestamp_diff_seconds")
    if ts_diff is not None:

        def _fmt_duration(seconds: int) -> str:
            s = int(seconds)
            mins, sec = divmod(s, 60)
            hrs, mins = divmod(mins, 60)
            days, hrs = divmod(hrs, 24)
            parts = []
            if days:
                parts.append(f"{days}d")
            if hrs:
                parts.append(f"{hrs}h")
            if mins:
                parts.append(f"{mins}m")
            parts.append(f"{sec}s")
            return " ".join(parts)

        print(f"Time span (initial->current): {ts_diff} sec ({_fmt_duration(int(ts_diff))})")

    # Percentages and counts
    def _pct(part: int, total: int) -> str:
        if not total or total <= 0:
            return "0.00%"
        try:
            return f"{(100.0 * float(part) / float(total)):.2f}%"
        except Exception:
            return "0.00%"

    gw = data.get("gateway_errors") or {}
    sent = data.get("sent_tx_hashes") or {}
    # "Pending (not committed yet)" should represent txs that have *ever* been
    # pending and have not (yet) been observed as committed, even across resyncs.
    pending_count = int(data.get("pending_not_committed_count") or 0)
    # Gateway errors in the summary should start fresh after each resync, so use
    # only the current live gateway_errors map here.
    gw_count = len(gw)
    committed = int(data.get("committed_count") or 0)
    revs_mainnet = data.get("revert_errors_mainnet") or {}
    revs_echonet = data.get("revert_errors_echonet") or {}
    revs_mainnet_count = len(revs_mainnet)
    revs_echonet_count = len(revs_echonet)
    resync_causes = data.get("resync_causes") or {}
    resync_triggers_count = len(resync_causes)
    certain = data.get("certain_failures") or {}
    certain_failures_count = len(certain)

    print("=== Transaction status summary ===")
    print(f"Committed: {committed} ({_pct(committed, total_sent)})")
    print(f"Pending (not committed yet): {pending_count} ({_pct(pending_count, total_sent)})")
    print(f"Gateway errors: {gw_count} ({_pct(gw_count, total_sent)})")
    print(
        f"Reverted only on Mainnet: {revs_mainnet_count} ({_pct(revs_mainnet_count, total_sent)})"
    )
    print(
        f"Reverted only on Echonet: {revs_echonet_count} ({_pct(revs_echonet_count, total_sent)})"
    )
    print(f"Resync triggers (first failures): {resync_triggers_count}")
    print(f"Certain failures (repeated resyncs): {certain_failures_count}")

    print("=== Gateway errors (hash -> response) ===")
    if not gw:
        print("(none)")
    else:
        for k, v in gw.items():
            print(f"{k}: {v}")

    print("=== Non-committed tx hashes ===")
    if not sent:
        print("(none)")
    else:
        for k, v in sent.items():
            print(f"{k} @ {v}")
    resync_causes = data.get("resync_causes") or {}
    print("=== Resync triggers (first failures) ===")
    if not resync_causes:
        print("(none)")
    else:
        for k, v in resync_causes.items():
            print(
                f"{k}: bn={v.get('block_number')} reason={v.get('reason')} count={v.get('count')}"
            )
    certain = data.get("certain_failures") or {}
    print("=== Certain failures (repeated triggers) ===")
    if not certain:
        print("(none)")
    else:
        for k, v in certain.items():
            print(
                f"{k}: bn={v.get('block_number')} reason={v.get('reason')} count={v.get('count')}"
            )


import re
import sys
from typing import Dict, List, Tuple

import requests
from collections import defaultdict


def extract_leaf_message(raw: str) -> str:
    """
    Try to pull out the most specific human-readable reason from an error trace.
    Heuristics:
      - look from the bottom up for '(... 'SOME_STRING')'
      - otherwise look for "SOME_STRING" in quotes
      - fallback: last non-empty line
    """
    if not raw:
        return ""

    lines = [ln.strip() for ln in raw.splitlines() if ln.strip()]
    if not lines:
        return raw

    # search from bottom: most specific is usually last
    for line in reversed(lines):
        # pattern like: (... ('SOME_REASON')).
        m = re.search(r"\('([^']*)'\)", line)
        if m and m.group(1):
            return m.group(1)

        # pattern like: "NEGATIVE: [...] RATES: []"
        m = re.search(r'"([^"]+)"', line)
        if m and m.group(1):
            return m.group(1)

    # fallback to the last line if nothing matched
    return lines[-1]


def classify_reason(msg: str) -> str:
    """
    Map a leaf message to a coarse-grained error type.
    You can keep adding/customizing rules as you learn more patterns.
    """
    if not msg:
        return "Unknown"

    # Normalize for easier matching
    lower = msg.lower()

    # Specific codes (normalize cases on checks)
    if "clear_at_least_minimum" in lower:
        return "CLEAR_AT_LEAST_MINIMUM"

    if "no_profit" in msg.lower():
        return "NO_PROFIT"

    # Nonce issues
    if "invalid_nonce" in lower:
        return "INVALID_NONCE"

    # Overflows (various widths)
    if "overflow" in lower and (
        "u256_sub" in lower or "u128_sub" in lower or "u64_sub" in lower or "u32_sub" in lower
    ):
        return "Overflow"

    if "result::unwrap failed" in lower:
        return "Result::unwrap failed"

    # Generic "NEGATIVE: [...]" style messages
    if "negative:" in msg:
        return "NEGATIVE"

    if "not player" in msg:
        return "Not player"

    if "player not won last round" in lower:
        return "Player not won last round"

    if "attestation is out of window" in lower:
        return "Attestation out of window"

    if "attestation with wrong block hash" in lower:
        return "Attestation wrong block hash"

    if "invalid request id" in lower:
        return "Invalid request ID"

    if "argent/multicall-failed" in lower:
        return "argent/multicall-failed"

    if "min_liquidity" in lower:
        return "MIN_LIQUIDITY"

    if "excessive_input_amount" in lower:
        return "EXCESSIVE_INPUT_AMOUNT"

    if "invalid burn amount" in lower:
        return "Invalid burn amount"

    if "token from balance is too low" in lower:
        return "Insufficient token balance"

    if "assert_eq" in lower:
        return "ASSERT_EQ failed"

    if "range-check validation failed" in lower:
        return "Range check failed"

    if "invalid_price_timestamp" in lower:
        return "INVALID_PRICE_TIMESTAMP"

    if (
        "could not reach the end of the program" in lower
        or "runresources has no remaining steps" in lower
    ):
        return "Out of steps"

    if lower.strip() in ("aotl", "msaotl"):
        return msg.strip()

    if "spl_zfo" in lower:
        return "SPL_ZFO"

    # Hex error codes like "0x29a." -> bucket under the code
    if lower.startswith("0x") and len(lower) < 16:
        return msg.strip().rstrip(".")

    if "invariant" in lower:
        return "Invariant violation"

    if "insufficient max l1datagas" in lower:
        return "Insufficient max L1DataGas"

    # Catch-all
    return "Other"


def group_reverts(reverts: Dict[str, str]) -> Dict[str, List[Tuple[str, str]]]:
    """
    Group reverts by classified error type.

    Returns:
        { error_type: [(tx_hash, leaf_message), ...], ... }
    """
    grouped: Dict[str, List[Tuple[str, str]]] = defaultdict(list)
    for tx_hash, raw_msg in reverts.items():
        leaf = extract_leaf_message(raw_msg)
        error_type = classify_reason(leaf)
        grouped[error_type].append((tx_hash, leaf))
    return grouped


def print_grouped_reverts(title: str, grouped: Dict[str, List[Tuple[str, str]]]) -> None:
    total = sum(len(v) for v in grouped.values())
    print(f"=== {title} ({total} txs) ===")
    if total == 0:
        print("(none)")
        return

    # Sort groups by descending size, then by error_type name
    for error_type, txs in sorted(
        grouped.items(),
        key=lambda kv: (-len(kv[1]), kv[0]),
    ):
        print(f"\n-- {error_type} ({len(txs)} txs) --")
        if not txs:
            print("(none)")
        else:
            for tx_hash, leaf in sorted(txs, key=lambda x: x[0]):
                print(f"{tx_hash}: {leaf}")


def compare_reverts(endpoint: str) -> None:
    data = _fetch_report_data(endpoint)
    _compare_reverts_from_data(data)


def _compare_reverts_from_data(data: dict) -> None:
    revs_mainnet = data.get("revert_errors_mainnet") or {}
    revs_echonet = data.get("revert_errors_echonet") or {}

    grouped_mainnet = group_reverts(revs_mainnet)
    grouped_echonet = group_reverts(revs_echonet)

    total_mainnet = sum(len(v) for v in grouped_mainnet.values())
    total_echonet = sum(len(v) for v in grouped_echonet.values())
    print(f"=== Revert counts ===")
    print(f"Only on Mainnet: {total_mainnet}")
    print(f"Only on Echonet: {total_echonet}")
    print()
    print_grouped_reverts("Reverted only on Mainnet", grouped_mainnet)
    print()
    print_grouped_reverts("Reverted only on Echonet", grouped_echonet)


def _fetch_report_data(endpoint: str) -> dict:
    resp = requests.get(endpoint, timeout=5)
    resp.raise_for_status()
    return resp.json()


def show_block(endpoint: str, block_number: int, kind: str) -> None:
    url = f"{endpoint.rsplit('/echonet/report', 1)[0]}/echonet/block_dump?blockNumber={block_number}&kind={kind}"
    try:
        resp = requests.get(url, timeout=5)
        if resp.status_code == consts.HTTP_NOT_FOUND:
            print(f"Block {block_number} not found")
            sys.exit(1)
        resp.raise_for_status()
        obj = resp.json()
    except Exception as e:
        print(
            f"Failed to fetch block {block_number} ({kind}) from {url}: {e}",
            file=sys.stderr,
        )
        sys.exit(1)
    print(f"=== Block {block_number} [{kind}] ===")
    print(json.dumps(obj, ensure_ascii=False, indent=2))


def main() -> None:
    parser = argparse.ArgumentParser(description="Echonet reports")
    parser.add_argument(
        "--snapshot",
        action="store_true",
        help="Fetch and print in-memory tx snapshot from the running app",
    )
    parser.add_argument(
        "--compare-reverts",
        action="store_true",
        help="Compare revert errors from in-memory state (mainnet vs echonet)",
    )
    parser.add_argument(
        "--endpoint",
        default="http://127.0.0.1/echonet/report",
        help="Report endpoint to query (default: http://127.0.0.1/echonet/report). "
        "If it fails, a fallback to port 8000 will be attempted automatically.",
    )
    parser.add_argument(
        "--all", action="store_true", help="Run both snapshot and revert comparison"
    )
    parser.add_argument("--show-block", type=int, help="Block number to fetch from in-memory store")
    parser.add_argument(
        "--kind",
        choices=["blob", "block", "state_update"],
        default="blob",
        help="Which payload to fetch for --show-block",
    )
    args = parser.parse_args()

    # If no explicit action flags provided, run --all by default
    if not (args.snapshot or args.compare_reverts or args.all or args.show_block):
        args.all = True

    if args.snapshot or args.all:
        print_snapshot(args.endpoint)
        if not args.all:
            return

    if args.compare_reverts:
        compare_reverts(args.endpoint)
        return

    if args.show_block is not None:
        show_block(args.endpoint, args.show_block, args.kind)
        return


if __name__ == "__main__":
    main()
