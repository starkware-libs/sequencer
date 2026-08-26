"""
Per-transaction fee rows for the 0.14.3-vs-0.14.4 fee comparison.

One CSV per run, appended as each block is committed. Rows go straight to the PVC rather than
being read back from the block store afterwards: that store keeps only the most recent blocks in
memory and caps its on-disk archive, so a thousand-block run would have lost its early blocks by
the time it finished. Every row carries both what echonet charged and what mainnet charged for
the same transaction, so a single run's file already holds a comparison and the run at the other
version only has to disagree with it in the places the versioned constants changed.
"""

from __future__ import annotations

import csv
import re
import threading
from pathlib import Path
from typing import Any, Iterable, List, Optional, Sequence

from echonet.echonet_types import JsonObject
from echonet.logger import get_logger

logger = get_logger("fee_recorder")

CSV_COLUMNS: tuple[str, ...] = (
    "run_label",
    "starknet_version",
    "block_number",
    "source_block_number",
    "transaction_hash",
    "transaction_type",
    "sender_address",
    "called_contracts",
    "execution_status",
    "actual_fee",
    "l1_gas",
    "l1_data_gas",
    "l2_gas",
    "n_steps",
    "mainnet_execution_status",
    "mainnet_actual_fee",
    "mainnet_l1_gas",
    "mainnet_l1_data_gas",
    "mainnet_l2_gas",
    "l1_gas_price_fri",
    "l1_data_gas_price_fri",
    "l2_gas_price_fri",
    "tip_fri_per_l2_gas",
    "revert_error",
)

# The fee a transaction pays closes exactly as
# `l1_gas * l1_gas_price_fri + l1_data_gas * l1_data_gas_price_fri + l2_gas * (l2_gas_price_fri +
# tip_fri_per_l2_gas)`, so `actual_fee` stays decomposable into the part the protocol sets and the
# part the sender bid on top of it.

# A run label names a file on the PVC, so keep it to characters that cannot escape the directory.
_SAFE_RUN_LABEL = re.compile(r"\A[A-Za-z0-9][A-Za-z0-9._-]{0,63}\Z")

# Revert reasons run to hundreds of lines; a row only needs enough to tell the causes apart.
MAX_REVERT_ERROR_CHARS = 300

# Multicalls can carry dozens of targets. Enough to classify the transaction, not to reconstruct it.
MAX_CALLED_CONTRACTS = 8


class FeeRecorderError(RuntimeError):
    """Raised when the configured run label cannot name a file."""


def normalize_address(value: Any) -> str:
    """
    Minimal lowercase hex form of a felt, so addresses join across runs and against the
    address lists a category is defined by. Non-hex input is passed through unchanged.
    """
    text = str(value).strip()
    if not text.startswith("0x"):
        return text
    try:
        return hex(int(text, 16))
    except ValueError:
        return text


def called_contracts_from_calldata(calldata: Sequence[Any]) -> List[str]:
    """
    The contract addresses an account's `__execute__` calldata calls into.

    The call targets are read out of the multicall structure rather than scanned for anywhere in
    the buffer: an address also appears in a call's arguments — a transfer recipient, a spender,
    a market — and those are not what the transaction called. Both account layouts in use on
    mainnet are tried, and calldata that neither consumes exactly is reported as no calls rather
    than as a guess, since a wrong target is worse than a missing one.
    """
    felts: List[int] = []
    for entry in calldata:
        text = str(entry).strip()
        if not text.startswith("0x"):
            return []
        try:
            felts.append(int(text, 16))
        except ValueError:
            return []

    if not felts:
        return []
    # Every call costs at least 3 felts, so a plausible count cannot exceed the buffer itself.
    # Checked before either parse so a huge leading felt cannot drive a huge loop.
    if felts[0] > len(felts):
        return []

    targets = _inline_multicall_targets(felts)
    if targets is None:
        targets = _split_calldata_multicall_targets(felts)
    if targets is None:
        return []
    return [hex(target) for target in targets[:MAX_CALLED_CONTRACTS]]


def _inline_multicall_targets(felts: Sequence[int]) -> Optional[List[int]]:
    """
    Targets under the Cairo 1 layout, where each call carries its own arguments:
    `[n_calls, (to, selector, inner_calldata_len, *inner_calldata) * n_calls]`.
    """
    targets: List[int] = []
    offset = 1
    for _call_index in range(felts[0]):
        if offset + 3 > len(felts):
            return None
        target, _selector, inner_calldata_length = felts[offset : offset + 3]
        offset += 3 + inner_calldata_length
        if offset > len(felts):
            return None
        targets.append(target)
    return targets if offset == len(felts) else None


def _split_calldata_multicall_targets(felts: Sequence[int]) -> Optional[List[int]]:
    """
    Targets under the Cairo 0 layout, where the calls index into one shared argument buffer:
    `[n_calls, (to, selector, data_offset, data_len) * n_calls, calldata_len, *calldata]`.
    """
    n_calls = felts[0]
    calldata_length_offset = 1 + 4 * n_calls
    if calldata_length_offset >= len(felts):
        return None
    if calldata_length_offset + 1 + felts[calldata_length_offset] != len(felts):
        return None
    return [felts[1 + 4 * call_index] for call_index in range(n_calls)]


def _total_gas(receipt: JsonObject, resource: str) -> Any:
    return receipt.get("execution_resources", {}).get("total_gas_consumed", {}).get(resource, "")


def _price_in_fri(block_document: JsonObject, key: str) -> Any:
    return block_document.get(key, {}).get("price_in_fri", "")


def _short_revert_error(receipt: JsonObject) -> str:
    error = receipt.get("revert_error") or ""
    return " ".join(str(error).split())[:MAX_REVERT_ERROR_CHARS]


class FeeRecorder:
    """
    Appends one row per committed transaction to `fees_<run_label>.csv` under the echonet
    log directory.
    """

    def __init__(self, run_label: str, log_dir: Path) -> None:
        if not _SAFE_RUN_LABEL.match(run_label):
            raise FeeRecorderError(
                f"Fee CSV run label {run_label!r} must match {_SAFE_RUN_LABEL.pattern}."
            )
        self._run_label = run_label
        self._csv_path = log_dir / f"fees_{run_label}.csv"
        self._write_lock = threading.Lock()
        log_dir.mkdir(parents=True, exist_ok=True)
        self._write_header_if_new()
        logger.info(f"Recording per-transaction fees for run {run_label!r} to {self._csv_path}")

    @property
    def csv_path(self) -> Path:
        return self._csv_path

    def record_block(
        self,
        block_document: JsonObject,
        source_block_number_by_tx_hash: dict[str, int],
        mainnet_receipt_by_tx_hash: dict[str, JsonObject],
    ) -> None:
        """
        Append the block's transactions. `source_block_number_by_tx_hash` and
        `mainnet_receipt_by_tx_hash` supply the mainnet side; transactions missing from either
        are still recorded, with those columns left empty.
        """
        rows = list(
            self._build_rows(
                block_document, source_block_number_by_tx_hash, mainnet_receipt_by_tx_hash
            )
        )
        if not rows:
            return
        with self._write_lock, open(self._csv_path, "a", encoding="utf-8", newline="") as csv_file:
            csv.writer(csv_file).writerows(rows)

    def _build_rows(
        self,
        block_document: JsonObject,
        source_block_number_by_tx_hash: dict[str, int],
        mainnet_receipt_by_tx_hash: dict[str, JsonObject],
    ) -> Iterable[list[Any]]:
        block_number = block_document["block_number"]
        starknet_version = block_document.get("starknet_version", "")
        l1_gas_price_fri = _price_in_fri(block_document, "l1_gas_price")
        l1_data_gas_price_fri = _price_in_fri(block_document, "l1_data_gas_price")
        l2_gas_price_fri = _price_in_fri(block_document, "l2_gas_price")

        transactions = block_document.get("transactions", [])
        receipts = block_document.get("transaction_receipts", [])
        for tx, receipt in zip(transactions, receipts):
            tx_hash = tx["transaction_hash"]
            mainnet_receipt: JsonObject = mainnet_receipt_by_tx_hash.get(tx_hash, {})
            yield [
                self._run_label,
                starknet_version,
                block_number,
                source_block_number_by_tx_hash.get(tx_hash, ""),
                tx_hash,
                tx.get("type", ""),
                normalize_address(tx.get("sender_address", "")),
                "|".join(called_contracts_from_calldata(tx.get("calldata", []))),
                receipt.get("execution_status", ""),
                receipt.get("actual_fee", ""),
                _total_gas(receipt, "l1_gas"),
                _total_gas(receipt, "l1_data_gas"),
                _total_gas(receipt, "l2_gas"),
                receipt.get("execution_resources", {}).get("n_steps", ""),
                mainnet_receipt.get("execution_status", ""),
                mainnet_receipt.get("actual_fee", ""),
                _total_gas(mainnet_receipt, "l1_gas") if mainnet_receipt else "",
                _total_gas(mainnet_receipt, "l1_data_gas") if mainnet_receipt else "",
                _total_gas(mainnet_receipt, "l2_gas") if mainnet_receipt else "",
                l1_gas_price_fri,
                l1_data_gas_price_fri,
                l2_gas_price_fri,
                tx.get("tip", ""),
                _short_revert_error(receipt),
            ]

    def _write_header_if_new(self) -> None:
        """
        Write the header only for a file that does not exist yet, so a restarted pod appends to
        the run it was already recording instead of starting over.
        """
        if self._csv_path.exists():
            return
        with open(self._csv_path, "w", encoding="utf-8", newline="") as csv_file:
            csv.writer(csv_file).writerow(CSV_COLUMNS)


def create_fee_recorder(run_label: str, log_dir: Path) -> Optional[FeeRecorder]:
    """
    Build a recorder, or return None when the run label is empty (the experiment is off) or
    unusable. Recording is a diagnostic; it must never keep echonet from starting.
    """
    if not run_label:
        return None
    try:
        return FeeRecorder(run_label=run_label, log_dir=log_dir)
    except (FeeRecorderError, OSError) as setup_error:
        logger.error(f"Fee recording disabled: {setup_error}")
        return None
