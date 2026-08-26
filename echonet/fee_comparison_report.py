#!/usr/bin/env python3
"""
Compare the per-transaction fee CSVs of two echonet runs — one replaying at the version mainnet
is on, one at the version being rolled out — and report the fee change per category of
transaction.

Pull each run's CSV off the pod with the port-forward `deploy_echonet.py --port-forward` opens:

    curl -sf http://127.0.0.1:18080/echonet/fee_csv -o fees_v0_14_3.csv

Then:

    python3 -m echonet.fee_comparison_report \\
        --baseline fees_v0_14_3.csv --candidate fees_v0_14_4.csv \\
        --categories categories.json

`categories.json` maps a category name to the addresses that define it, matched against both the
sending account and the contracts its calldata calls into:

    {"extended": ["0x1234..."], "games": ["0xabcd...", "0xbeef..."]}

Transactions matching no category are reported as `other`.

See docs/echonet_fee_comparison.md for the full procedure, the replay artifacts to expect, and
why the per-transaction distribution matters more than the fee-weighted total.
"""

from __future__ import annotations

import argparse
import csv
import json
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Sequence

from echonet.fee_recorder import normalize_address

OTHER_CATEGORY = "other"

# Both runs replay the same mainnet blocks at the same recorded gas prices, so a transaction that
# reached the same outcome in both paid on equal terms and its two fees are comparable — a revert
# in both included, since it too paid for the work it did. One that reverted in only one run
# stopped at a different point, so its fees are not comparable; those are counted separately.
SUCCEEDED_EXECUTION_STATUS = "SUCCEEDED"

# The transaction type the feeder gateway names an invoke by, and the only type whose calldata
# carries call targets to classify on.
INVOKE_TRANSACTION_TYPE = "INVOKE_FUNCTION"


def parse_felt(value: str) -> Optional[int]:
    """Parse a CSV cell holding a felt, written either as `0x...` or in decimal. None if blank."""
    text = str(value).strip()
    if not text:
        return None
    try:
        return int(text, 16) if text.startswith("0x") else int(text)
    except ValueError:
        return None


@dataclass
class TransactionFees:
    """One transaction's fee and gas in a single run."""

    transaction_hash: str
    source_block_number: str
    transaction_type: str
    sender_address: str
    called_contracts: List[str]
    execution_status: str
    actual_fee: Optional[int]
    l2_gas: Optional[int]
    mainnet_actual_fee: Optional[int]

    @classmethod
    def from_row(cls, row: Dict[str, str]) -> "TransactionFees":
        called_contracts = [
            normalize_address(address)
            for address in row.get("called_contracts", "").split("|")
            if address
        ]
        return cls(
            transaction_hash=row["transaction_hash"],
            source_block_number=row.get("source_block_number", ""),
            transaction_type=row.get("transaction_type", ""),
            sender_address=normalize_address(row.get("sender_address", "")),
            called_contracts=called_contracts,
            execution_status=row.get("execution_status", ""),
            actual_fee=parse_felt(row.get("actual_fee", "")),
            l2_gas=parse_felt(row.get("l2_gas", "")),
            mainnet_actual_fee=parse_felt(row.get("mainnet_actual_fee", "")),
        )

    @property
    def addresses(self) -> List[str]:
        return [self.sender_address, *self.called_contracts]


PERCENTILE_FRACTIONS: tuple[tuple[str, float], ...] = (
    ("p10", 0.10),
    ("p25", 0.25),
    ("p50", 0.50),
    ("p75", 0.75),
    ("p90", 0.90),
)


def percentile(sorted_values: Sequence[float], fraction: float) -> float:
    """Linearly interpolated percentile of an already-sorted, non-empty sequence."""
    if len(sorted_values) == 1:
        return sorted_values[0]
    position = fraction * (len(sorted_values) - 1)
    lower_index = int(position)
    upper_index = min(lower_index + 1, len(sorted_values) - 1)
    weight = position - lower_index
    return sorted_values[lower_index] * (1.0 - weight) + sorted_values[upper_index] * weight


@dataclass
class CategoryTotals:
    """Accumulated fees for one category across the transactions both runs executed."""

    name: str
    n_transactions: int = 0
    baseline_fee: int = 0
    candidate_fee: int = 0
    baseline_l2_gas: int = 0
    candidate_l2_gas: int = 0
    per_transaction_fee_ratios: List[float] = field(default_factory=list)
    n_reverted_only_in_candidate: int = 0
    n_fee_increased: int = 0
    n_fee_decreased: int = 0
    n_fee_unchanged: int = 0

    def add(self, baseline: TransactionFees, candidate: TransactionFees) -> None:
        if baseline.execution_status != candidate.execution_status:
            if candidate.execution_status != SUCCEEDED_EXECUTION_STATUS:
                self.n_reverted_only_in_candidate += 1
            return
        if baseline.actual_fee is None or candidate.actual_fee is None:
            return

        self.n_transactions += 1
        self.baseline_fee += baseline.actual_fee
        self.candidate_fee += candidate.actual_fee
        if baseline.l2_gas is not None and candidate.l2_gas is not None:
            self.baseline_l2_gas += baseline.l2_gas
            self.candidate_l2_gas += candidate.l2_gas
        if candidate.actual_fee > baseline.actual_fee:
            self.n_fee_increased += 1
        elif candidate.actual_fee < baseline.actual_fee:
            self.n_fee_decreased += 1
        else:
            self.n_fee_unchanged += 1
        if baseline.actual_fee > 0:
            self.per_transaction_fee_ratios.append(candidate.actual_fee / baseline.actual_fee)

    @property
    def fee_change_percent(self) -> Optional[float]:
        return percent_change(self.baseline_fee, self.candidate_fee)

    @property
    def l2_gas_change_percent(self) -> Optional[float]:
        return percent_change(self.baseline_l2_gas, self.candidate_l2_gas)

    @property
    def fee_change_percentiles(self) -> Dict[str, Optional[float]]:
        """Per-transaction fee change at each percentile, as a percent. Empty maps to all None."""
        if not self.per_transaction_fee_ratios:
            return {name: None for name, _fraction in PERCENTILE_FRACTIONS}
        ratios = sorted(self.per_transaction_fee_ratios)
        return {
            name: (percentile(ratios, fraction) - 1.0) * 100.0
            for name, fraction in PERCENTILE_FRACTIONS
        }


def percent_change(baseline: int, candidate: int) -> Optional[float]:
    if baseline == 0:
        return None
    return (candidate - baseline) / baseline * 100.0


def read_run(csv_path: Path) -> Dict[str, TransactionFees]:
    """Read one run's CSV, keyed by transaction hash. Later rows win, as a re-execution should."""
    with open(csv_path, newline="", encoding="utf-8") as csv_file:
        rows = list(csv.DictReader(csv_file))
    return {row["transaction_hash"]: TransactionFees.from_row(row) for row in rows}


def load_categories(categories_path: Optional[Path]) -> Dict[str, frozenset]:
    if categories_path is None:
        return {}
    with open(categories_path, encoding="utf-8") as categories_file:
        raw = json.load(categories_file)
    return {
        str(name): frozenset(normalize_address(address) for address in addresses)
        for name, addresses in raw.items()
    }


def classify(transaction: TransactionFees, categories: Dict[str, frozenset]) -> str:
    """
    The first category any of the transaction's addresses belongs to. Ordering matters only when
    address lists overlap, in which case the earlier entry in the categories file wins.
    """
    addresses = set(transaction.addresses)
    for name, category_addresses in categories.items():
        if addresses & category_addresses:
            return name
    return OTHER_CATEGORY


@dataclass
class ComparisonResult:
    totals_by_category: Dict[str, CategoryTotals]
    n_only_in_baseline: int
    n_only_in_candidate: int
    n_baseline_rows: int
    n_candidate_rows: int
    n_baseline_matching_mainnet_fee: int
    n_baseline_with_mainnet_fee: int
    n_invoke_transactions: int
    n_invoke_with_called_contracts: int


def compare(
    baseline_run: Dict[str, TransactionFees],
    candidate_run: Dict[str, TransactionFees],
    categories: Dict[str, frozenset],
) -> ComparisonResult:
    totals_by_category: Dict[str, CategoryTotals] = {
        name: CategoryTotals(name=name) for name in [*categories, OTHER_CATEGORY]
    }

    shared_hashes = baseline_run.keys() & candidate_run.keys()
    for transaction_hash in shared_hashes:
        baseline = baseline_run[transaction_hash]
        candidate = candidate_run[transaction_hash]
        totals_by_category[classify(baseline, categories)].add(baseline, candidate)

    n_baseline_with_mainnet_fee = 0
    n_baseline_matching_mainnet_fee = 0
    for baseline in baseline_run.values():
        if baseline.mainnet_actual_fee is None:
            continue
        n_baseline_with_mainnet_fee += 1
        if baseline.actual_fee == baseline.mainnet_actual_fee:
            n_baseline_matching_mainnet_fee += 1

    invoke_transactions = [
        baseline
        for baseline in baseline_run.values()
        if baseline.transaction_type == INVOKE_TRANSACTION_TYPE
    ]
    return ComparisonResult(
        totals_by_category=totals_by_category,
        n_only_in_baseline=len(baseline_run.keys() - candidate_run.keys()),
        n_only_in_candidate=len(candidate_run.keys() - baseline_run.keys()),
        n_baseline_rows=len(baseline_run),
        n_candidate_rows=len(candidate_run),
        n_baseline_matching_mainnet_fee=n_baseline_matching_mainnet_fee,
        n_baseline_with_mainnet_fee=n_baseline_with_mainnet_fee,
        n_invoke_transactions=len(invoke_transactions),
        n_invoke_with_called_contracts=sum(
            1 for baseline in invoke_transactions if baseline.called_contracts
        ),
    )


def format_percent(value: Optional[float]) -> str:
    return "n/a" if value is None else f"{value:+.3f}%"


def format_strk(fri: int) -> str:
    """FRI is 1e-18 STRK; totals are easier to sanity-check in whole STRK."""
    return f"{fri / 1e18:.6f}"


def render(result: ComparisonResult) -> str:
    lines: List[str] = []
    lines.append("Harness health")
    lines.append(f"  baseline rows:  {result.n_baseline_rows}")
    lines.append(f"  candidate rows: {result.n_candidate_rows}")
    lines.append(f"  only in baseline:  {result.n_only_in_baseline}")
    lines.append(f"  only in candidate: {result.n_only_in_candidate}")
    if result.n_baseline_with_mainnet_fee:
        matching_share = (
            result.n_baseline_matching_mainnet_fee / result.n_baseline_with_mainnet_fee * 100.0
        )
        lines.append(
            f"  baseline fee == mainnet fee: {result.n_baseline_matching_mainnet_fee}"
            f"/{result.n_baseline_with_mainnet_fee} ({matching_share:.2f}%)"
        )
    if result.n_invoke_transactions:
        parsed_share = result.n_invoke_with_called_contracts / result.n_invoke_transactions * 100.0
        lines.append(
            f"  invokes with parsed call targets: {result.n_invoke_with_called_contracts}"
            f"/{result.n_invoke_transactions} ({parsed_share:.2f}%)"
        )
    lines.append("")

    lines.append("Totals over the transactions both runs executed to the same outcome")
    header = (
        f"{'category':<12}{'txs':>7}{'baseline STRK':>16}{'candidate STRK':>16}"
        f"{'total fee':>11}{'l2 gas':>11}{'rev. only':>11}"
    )
    lines.append(header)
    lines.append("-" * len(header))
    for totals in result.totals_by_category.values():
        lines.append(
            f"{totals.name:<12}{totals.n_transactions:>7}"
            f"{format_strk(totals.baseline_fee):>16}{format_strk(totals.candidate_fee):>16}"
            f"{format_percent(totals.fee_change_percent):>11}"
            f"{format_percent(totals.l2_gas_change_percent):>11}"
            f"{totals.n_reverted_only_in_candidate:>11}"
        )

    lines.append("")
    lines.append("Per-transaction fee change")
    header = (
        f"{'category':<12}{'p10':>10}{'p25':>10}{'p50':>10}{'p75':>10}{'p90':>10}"
        f"{'up':>7}{'flat':>7}{'down':>7}"
    )
    lines.append(header)
    lines.append("-" * len(header))
    for totals in result.totals_by_category.values():
        percentiles = totals.fee_change_percentiles
        cells = "".join(
            f"{format_percent(percentiles[name]):>10}" for name, _fraction in PERCENTILE_FRACTIONS
        )
        lines.append(
            f"{totals.name:<12}{cells}"
            f"{totals.n_fee_increased:>7}{totals.n_fee_unchanged:>7}{totals.n_fee_decreased:>7}"
        )
    return "\n".join(lines)


def write_per_transaction_csv(
    output_path: Path,
    baseline_run: Dict[str, TransactionFees],
    candidate_run: Dict[str, TransactionFees],
    categories: Dict[str, frozenset],
) -> None:
    """The joined rows behind the summary, for slicing the comparison a different way."""
    columns = (
        "transaction_hash",
        "source_block_number",
        "transaction_type",
        "category",
        "sender_address",
        "called_contracts",
        "baseline_execution_status",
        "candidate_execution_status",
        "baseline_actual_fee",
        "candidate_actual_fee",
        "fee_change_percent",
        "baseline_l2_gas",
        "candidate_l2_gas",
    )
    with open(output_path, "w", newline="", encoding="utf-8") as csv_file:
        writer = csv.writer(csv_file)
        writer.writerow(columns)
        for transaction_hash in sorted(baseline_run.keys() & candidate_run.keys()):
            baseline = baseline_run[transaction_hash]
            candidate = candidate_run[transaction_hash]
            fee_change = (
                percent_change(baseline.actual_fee, candidate.actual_fee)
                if baseline.actual_fee is not None and candidate.actual_fee is not None
                else None
            )
            writer.writerow(
                [
                    transaction_hash,
                    baseline.source_block_number,
                    baseline.transaction_type,
                    classify(baseline, categories),
                    baseline.sender_address,
                    "|".join(baseline.called_contracts),
                    baseline.execution_status,
                    candidate.execution_status,
                    baseline.actual_fee if baseline.actual_fee is not None else "",
                    candidate.actual_fee if candidate.actual_fee is not None else "",
                    "" if fee_change is None else f"{fee_change:.6f}",
                    baseline.l2_gas if baseline.l2_gas is not None else "",
                    candidate.l2_gas if candidate.l2_gas is not None else "",
                ]
            )


def parse_args(argv: Optional[Sequence[str]] = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawTextHelpFormatter
    )
    parser.add_argument(
        "--baseline", required=True, type=Path, help="Fee CSV of the run at mainnet's version."
    )
    parser.add_argument(
        "--candidate", required=True, type=Path, help="Fee CSV of the run at the new version."
    )
    parser.add_argument(
        "--categories",
        type=Path,
        default=None,
        help="JSON mapping a category name to its addresses. Everything else is 'other'.",
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=None,
        help="Write the joined per-transaction rows behind the summary to this CSV.",
    )
    return parser.parse_args(argv)


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = parse_args(argv)
    baseline_run = read_run(args.baseline)
    candidate_run = read_run(args.candidate)
    categories = load_categories(args.categories)

    print(render(compare(baseline_run, candidate_run, categories)))
    if args.out:
        write_per_transaction_csv(args.out, baseline_run, candidate_run, categories)
        print(f"\nPer-transaction rows: {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
