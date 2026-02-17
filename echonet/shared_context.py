from __future__ import annotations

import json
import shutil
import threading
import time
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import ClassVar, Dict, List, Mapping, Optional, Set

from echonet.constants import IGNORED_REVERT_PATTERNS
from echonet.echonet_types import (
    BLOCK_STORE_TUNING,
    CONFIG,
    JsonObject,
    ResyncTriggerMap,
    RevertErrorInfo,
    create_revert_error_info,
)
from echonet.l1_logic.l1_client import L1Client
from echonet.l1_logic.l1_manager import L1Manager
from echonet.logger import get_logger
from echonet.report_models import SnapshotModel

logger = get_logger("shared_context")


@dataclass(slots=True)
class _TxTracker:
    """Transaction lifecycle and counters used by reporting."""

    currently_pending: Dict[str, int]  # tx_hash -> source block number
    ever_seen_pending: Set[str]  # cumulative set of tx hashes ever observed pending
    committed: Dict[str, int]  # cumulative map: tx_hash -> commit block number
    total_forwarded_tx_count: int  # count of forwarded txs (counted once per block)
    max_forwarded_block: int  # highest block included in total_forwarded_tx_count

    @classmethod
    def empty(cls) -> "_TxTracker":
        return cls(
            currently_pending={},
            ever_seen_pending=set(),
            committed={},
            total_forwarded_tx_count=0,
            max_forwarded_block=0,
        )

    def record_sent(self, tx_hash: str, source_block_number: int) -> None:
        """Record a transaction as sent - add to the pending set (transactions sent but not committed yet)"""
        self.currently_pending[tx_hash] = source_block_number
        if tx_hash not in self.ever_seen_pending and tx_hash not in self.committed:
            self.ever_seen_pending.add(tx_hash)

    def record_committed(self, tx_hash: str, block_number: int) -> None:
        """Record a transaction as committed - add to the committed set (transactions that have been committed) and remove from the pending set"""
        self.committed[tx_hash] = block_number
        self.currently_pending.pop(tx_hash, None)

    def record_forwarded_block(self, block_number: int, tx_count: int) -> None:
        if block_number > self.max_forwarded_block:
            self.max_forwarded_block = block_number
            self.total_forwarded_tx_count += tx_count


@dataclass(slots=True)
class _TxErrorTracker:
    """Gateway + revert error tracking (live vs cumulative) for reporting."""

    gateway_errors_live: Dict[str, JsonObject]  # reset on resync
    revert_errors_mainnet: Dict[str, RevertErrorInfo]  # tx_hash -> {block_number, error}
    revert_errors_echonet: Dict[str, RevertErrorInfo]  # tx_hash -> {block_number, error}

    @classmethod
    def empty(cls) -> "_TxErrorTracker":
        return cls(
            gateway_errors_live={},
            revert_errors_mainnet={},
            revert_errors_echonet={},
        )

    def record_gateway_error(
        self, tx_hash: str, status: int, response: str, block_number: int
    ) -> None:
        self.gateway_errors_live[tx_hash] = {
            "status": status,
            "response": response,
            "block_number": block_number,
        }

    def record_mainnet_revert_error(self, tx_hash: str, error: str, block_number: int) -> None:
        self.revert_errors_mainnet[tx_hash] = create_revert_error_info(
            block_number=block_number, error=error
        )

    def record_echonet_revert_error(
        self, tx_hash: str, error: str, source_block_number: int
    ) -> None:
        def matches_ignored_revert_error(revert_error: str) -> bool:
            return any(pattern in revert_error.lower() for pattern in IGNORED_REVERT_PATTERNS)

        # Ignore expected revert errors. Exclude from both mainnet and echonet reports.
        if matches_ignored_revert_error(error):
            if tx_hash in self.revert_errors_mainnet:
                self.revert_errors_mainnet.pop(tx_hash, None)
            return

        # If we already have a mainnet revert for this tx, treat as matched and drop it.
        if tx_hash in self.revert_errors_mainnet:
            self.revert_errors_mainnet.pop(tx_hash, None)
        else:
            self.revert_errors_echonet[tx_hash] = create_revert_error_info(
                block_number=source_block_number, error=error
            )

    def clear_live(self) -> None:
        self.gateway_errors_live.clear()


@dataclass(slots=True)
class _ResyncTracker:
    """Tracks resync triggers and promotes repeated errors to 'certain failures'."""

    resync_causes: ResyncTriggerMap
    certain_failures: ResyncTriggerMap

    @classmethod
    def empty(cls) -> "_ResyncTracker":
        return cls(resync_causes={}, certain_failures={})

    def record_cause(self, tx_hash: str, block_number: int, reason: str) -> bool:
        entry = self.certain_failures.get(tx_hash)
        if entry:
            entry["count"] += 1
            entry["block_number"] = block_number
            entry["reason"] = reason
            return True

        entry = dict(self.resync_causes.pop(tx_hash, {}))
        if entry:
            entry["count"] += 1
            entry["block_number"] = block_number
            entry["reason"] = reason
            self.certain_failures[tx_hash] = entry
            return True

        self.resync_causes[tx_hash] = {
            "tx_hash": tx_hash,
            "block_number": block_number,
            "reason": reason,
            "count": 1,
        }
        return False


@dataclass(slots=True)
class _BlockStore:
    """In-memory storage for echo_center outputs and raw feeder blocks."""

    _MAX_BLOCKS_ARCHIVES_BYTES: ClassVar[int] = 30 * 1024 * 1024 * 1024  # 30 GiB
    _CLEANUP_INTERVAL_SECONDS: ClassVar[int] = 5 * 60  # avoid expensive scans too frequently
    _last_cleanup_monotonic: ClassVar[float] = 0.0

    blocks: Dict[int, JsonObject]  # block_number -> {blob, block, state_update}
    fgw_blocks: Dict[int, JsonObject]  # feeder-gateway block_number -> raw block object
    archive_dir: Optional[Path]  # lazily created on first eviction; reused for the run

    @classmethod
    def empty(cls) -> "_BlockStore":
        return cls(blocks={}, fgw_blocks={}, archive_dir=None)

    def clear_live(self) -> None:
        self.blocks.clear()
        self.fgw_blocks.clear()
        self.archive_dir = None

    def snapshot_items(self) -> List[tuple[int, JsonObject]]:
        return sorted(((bn, dict(entry)) for bn, entry in self.blocks.items()), key=lambda p: p[0])

    def _ensure_archive_dir(self) -> Path:
        if self.archive_dir:
            return self.archive_dir

        ts_suffix = datetime.utcnow().strftime("%Y%m%dT%H%M%SZ")
        candidate = CONFIG.paths.log_dir / f"blocks_{ts_suffix}"
        candidate.mkdir(parents=True, exist_ok=True)
        self.archive_dir = candidate
        return candidate

    @classmethod
    def _enforce_blocks_archives_size_cap(cls) -> None:
        """Delete the oldest blocks_* folder(s) in CONFIG.paths.log_dir if total size exceeds cap."""
        now = time.monotonic()
        if (now - cls._last_cleanup_monotonic) < cls._CLEANUP_INTERVAL_SECONDS:
            return
        cls._last_cleanup_monotonic = now

        root_dir = CONFIG.paths.log_dir
        archives = sorted(
            (p for p in root_dir.iterdir() if p.is_dir() and p.name.startswith("blocks_")),
            key=lambda p: p.name,  # blocks_YYYYmmddTHHMMSSZ sorts oldest->newest
        )

        sizes = {p: sum(f.stat().st_size for f in p.iterdir() if f.is_file()) for p in archives}
        total_size = sum(sizes.values())

        for p, size in sizes.items():
            if total_size <= cls._MAX_BLOCKS_ARCHIVES_BYTES:
                break
            shutil.rmtree(p)
            total_size -= size
            logger.warning(
                "Deleted old blocks archive folder to enforce cap "
                f"({(cls._MAX_BLOCKS_ARCHIVES_BYTES / (1024**3)):.1f} GiB): {p}"
            )

    @staticmethod
    def _evict_old_items(
        store: Dict[int, JsonObject], current_block_number: int
    ) -> List[tuple[int, JsonObject]]:
        cutoff = current_block_number - BLOCK_STORE_TUNING.max_blocks_to_keep_in_memory
        evict_bns = [bn for bn in store.keys() if bn < cutoff]
        return [(bn, store.pop(bn)) for bn in sorted(evict_bns)]

    # --- Block store API ---
    def store_block(
        self, block_number: int, blob: JsonObject, fgw_block: JsonObject, state_update: JsonObject
    ) -> List[tuple[int, JsonObject]]:
        self.blocks[block_number] = {
            "blob": blob,
            "block": fgw_block,
            "state_update": state_update,
        }
        return self._evict_old_items(self.blocks, current_block_number=block_number)

    def store_fgw_block(self, block_number: int, block_obj: JsonObject) -> None:
        self.fgw_blocks[block_number] = block_obj
        self._evict_old_items(self.fgw_blocks, current_block_number=block_number)

    def get_fgw_block(self, block_number: int) -> Optional[JsonObject]:
        return self.fgw_blocks.get(block_number)

    def get_block_numbers_sorted(self) -> List[int]:
        return sorted(self.blocks.keys())

    def get_block_field(self, block_number: int, field: str) -> Optional[JsonObject]:
        entry = self.blocks.get(block_number)
        return None if not entry else entry.get(field)

    def get_latest_block_number(self) -> Optional[int]:
        return max(self.blocks.keys()) if self.blocks else None

    def has_block(self, block_number: int) -> bool:
        return block_number in self.blocks

    def has_any_blocks(self) -> bool:
        return bool(self.blocks)

    @staticmethod
    def write_snapshot_items_to_disk(
        snapshot_items: List[tuple[int, JsonObject]], base_dir: Path
    ) -> None:
        try:
            for bn, entry in snapshot_items:
                (base_dir / f"blob_{bn}.json").write_text(
                    json.dumps(entry["blob"], ensure_ascii=False),
                    encoding="utf-8",
                )
                (base_dir / f"block_{bn}.json").write_text(
                    json.dumps(entry["block"], ensure_ascii=False),
                    encoding="utf-8",
                )
                (base_dir / f"state_update_{bn}.json").write_text(
                    json.dumps(entry["state_update"], ensure_ascii=False),
                    encoding="utf-8",
                )
            _BlockStore._enforce_blocks_archives_size_cap()
        except Exception as e:
            logger.error(f"Failed to snapshot blocks to disk: {e}")


@dataclass(slots=True)
class _ProgressMarkers:
    """Progress markers for reporting and L1Manager callbacks."""

    last_echo_center_block: Optional[int]
    sender_current_block: Optional[int]
    initial_start_block: Optional[int]
    current_start_block: Optional[int]
    first_block_timestamp: Optional[int]
    latest_block_timestamp: Optional[int]
    base_block_hash_hex: Optional[str]  # The hash of the current start block - 1

    @classmethod
    def empty(cls) -> "_ProgressMarkers":
        return cls(
            last_echo_center_block=None,
            sender_current_block=None,
            initial_start_block=None,
            current_start_block=None,
            first_block_timestamp=None,
            latest_block_timestamp=None,
            base_block_hash_hex=None,
        )

    # --- Progress API ---
    def set_last_block(self, block_number: int) -> None:
        self.last_echo_center_block = block_number

    def get_last_block(self) -> Optional[int]:
        return self.last_echo_center_block

    def set_sender_current_block(self, block_number: int) -> None:
        self.sender_current_block = block_number

    def get_sender_current_block(self) -> Optional[int]:
        return self.sender_current_block

    def set_initial_start_block_if_absent(self, block_number: int) -> None:
        if self.initial_start_block is None:
            self.initial_start_block = block_number
        if self.current_start_block is None:
            self.current_start_block = block_number

    def set_current_start_block(self, block_number: int) -> None:
        self.current_start_block = block_number

    def set_block_timestamp(self, timestamp_seconds: int) -> None:
        if self.first_block_timestamp is None:
            self.first_block_timestamp = timestamp_seconds
        self.latest_block_timestamp = timestamp_seconds

    def set_base_block_hash(self, base_block_hash_hex: str) -> None:
        self.base_block_hash_hex = base_block_hash_hex

    def get_base_block_info(self, default_start_block: int) -> tuple[int, int]:
        """
        Returns (last_proved_block_number, last_proved_block_hash_int).
        """
        bn = self.get_current_start_block(default_start_block=default_start_block)
        return bn - 1, int(self.base_block_hash_hex, 16) if self.base_block_hash_hex else 0

    def get_current_start_block(self, default_start_block: int) -> int:
        return self.current_start_block if self.current_start_block else default_start_block

    def get_initial_start_block(self, default_start_block: int) -> int:
        return self.initial_start_block if self.initial_start_block else default_start_block


class SharedContext:
    """
    Thread-safe in-memory state shared between echonet components.

    The public methods form an API used by `transaction_sender` and `echo_center`.
    """

    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._tx = _TxTracker.empty()
        self._errors = _TxErrorTracker.empty()
        self._resync = _ResyncTracker.empty()
        self._blocks = _BlockStore.empty()
        self._progress = _ProgressMarkers.empty()

    # --- Tx lifecycle ---
    def record_sent_tx(self, tx_hash: str, source_block_number: int) -> None:
        with self._lock:
            self._tx.record_sent(tx_hash, source_block_number)

    def record_forwarded_block(self, block_number: int, tx_count: int) -> None:
        with self._lock:
            self._tx.record_forwarded_block(block_number, tx_count)

    def record_committed_tx(self, tx_hash: str, block_number: int) -> None:
        with self._lock:
            self._tx.record_committed(tx_hash, block_number)

    def is_pending_tx(self, tx_hash: str) -> bool:
        with self._lock:
            return tx_hash in self._tx.currently_pending

    def get_pending_tx_count(self) -> int:
        with self._lock:
            return len(self._tx.currently_pending)

    def get_sent_block_number(self, tx_hash: str) -> int:
        with self._lock:
            return self._tx.currently_pending[tx_hash]

    def get_resync_evaluation_inputs(self) -> tuple[Dict[str, JsonObject], Dict[str, int]]:
        """
        Return the minimal live state needed by transaction_sender's resync policy:
        - gateway_errors_live (tx_hash -> {status, response, block_number})
        - currently_pending (tx_hash -> source block number)
        """
        with self._lock:
            return dict(self._errors.gateway_errors_live), dict(self._tx.currently_pending)

    # --- Errors ---
    def record_gateway_error(
        self, tx_hash: str, status: int, response: str, block_number: int
    ) -> None:
        with self._lock:
            self._errors.record_gateway_error(tx_hash, status, response, block_number=block_number)

    def record_mainnet_revert_error(self, tx_hash: str, error: str, block_number: int) -> None:
        with self._lock:
            self._errors.record_mainnet_revert_error(tx_hash, error, block_number=block_number)

    def record_mainnet_revert_errors(self, block_number: int, errors: Mapping[str, str]) -> None:
        with self._lock:
            for tx_hash, err in errors.items():
                self._errors.record_mainnet_revert_error(tx_hash, err, block_number=block_number)

    def record_echonet_revert_error(self, tx_hash: str, error: str) -> None:
        with self._lock:
            self._errors.record_echonet_revert_error(
                tx_hash, error, source_block_number=self._tx.currently_pending[tx_hash]
            )

    # --- Resync causes ---
    def record_resync_cause(self, tx_hash: str, block_number: int, reason: str) -> bool:
        with self._lock:
            return self._resync.record_cause(tx_hash, block_number, reason)

    def clear_for_resync(self) -> None:
        """Clear live state for a new run while preserving cumulative stats."""
        with self._lock:
            snapshot_items = self._blocks.snapshot_items()
            archive_dir = self._blocks._ensure_archive_dir()
            self._tx.currently_pending.clear()
            self._errors.clear_live()
            self._blocks.clear_live()
            self._progress.last_echo_center_block = None
            self._progress.sender_current_block = None
        _BlockStore.write_snapshot_items_to_disk(snapshot_items, base_dir=archive_dir)
        l1_manager.clear_stored_blocks()

    # --- Block storage (echo_center output + raw FGW blocks) ---
    def store_block(
        self, block_number: int, blob: JsonObject, fgw_block: JsonObject, state_update: JsonObject
    ) -> None:
        with self._lock:
            evicted_items = self._blocks.store_block(
                block_number, blob=blob, fgw_block=fgw_block, state_update=state_update
            )
        if evicted_items:
            _BlockStore.write_snapshot_items_to_disk(
                evicted_items, base_dir=self._blocks._ensure_archive_dir()
            )

    def store_fgw_block(self, block_number: int, block_obj: JsonObject) -> None:
        with self._lock:
            self._blocks.store_fgw_block(block_number, block_obj)

    def get_fgw_block(self, block_number: int) -> Optional[JsonObject]:
        with self._lock:
            return self._blocks.get_fgw_block(block_number)

    def get_block_numbers_sorted(self) -> List[int]:
        with self._lock:
            return self._blocks.get_block_numbers_sorted()

    def get_block_field(self, block_number: int, field: str) -> Optional[JsonObject]:
        with self._lock:
            return self._blocks.get_block_field(block_number, field)

    def get_latest_block_number(self) -> Optional[int]:
        with self._lock:
            return self._blocks.get_latest_block_number()

    def has_block(self, block_number: int) -> bool:
        with self._lock:
            return self._blocks.has_block(block_number)

    def has_any_blocks(self) -> bool:
        with self._lock:
            return self._blocks.has_any_blocks()

    # --- Reporting ---
    def get_report_snapshot(self) -> SnapshotModel:
        with self._lock:
            configured_start_block = CONFIG.blocks.start_block
            current_block = self._progress.sender_current_block
            initial_start_block = self._progress.get_initial_start_block(configured_start_block)
            current_start_block = self._progress.get_current_start_block(configured_start_block)
            blocks_sent_count = (
                max(0, current_block - initial_start_block)
                if (current_block and initial_start_block)
                else None
            )
            first_ts = self._progress.first_block_timestamp
            latest_ts = self._progress.latest_block_timestamp
            timestamp_diff_seconds = latest_ts - first_ts if (first_ts and latest_ts) else None

            return SnapshotModel(
                start_block=configured_start_block,
                initial_start_block=initial_start_block,
                current_start_block=current_start_block,
                current_block=current_block,
                blocks_sent_count=blocks_sent_count,
                first_block_timestamp=first_ts,
                latest_block_timestamp=latest_ts,
                timestamp_diff_seconds=timestamp_diff_seconds,
                total_sent_tx_count=self._tx.total_forwarded_tx_count,
                committed_count=len(self._tx.committed),
                pending_total_count=len(self._tx.ever_seen_pending),
                pending_commission_count=len(self._tx.ever_seen_pending) - len(self._tx.committed),
                sent_tx_hashes=dict(self._tx.currently_pending),
                gateway_errors=dict(self._errors.gateway_errors_live),
                revert_errors_mainnet=dict(self._errors.revert_errors_mainnet),
                revert_errors_echonet=dict(self._errors.revert_errors_echonet),
                resync_causes=dict(self._resync.resync_causes),
                certain_failures=dict(self._resync.certain_failures),
            )

    # --- Progress markers ---
    def set_last_block(self, block_number: int) -> None:
        with self._lock:
            self._progress.set_last_block(block_number)

    def get_last_block(self) -> Optional[int]:
        with self._lock:
            return self._progress.get_last_block()

    def set_sender_current_block(self, block_number: int) -> None:
        with self._lock:
            self._progress.set_sender_current_block(block_number)

    def get_sender_current_block(self) -> Optional[int]:
        with self._lock:
            return self._progress.get_sender_current_block()

    def set_initial_start_block_if_absent(self, block_number: int) -> None:
        with self._lock:
            self._progress.set_initial_start_block_if_absent(block_number)

    def set_current_start_block(self, block_number: int) -> None:
        with self._lock:
            self._progress.set_current_start_block(block_number)

    def set_block_timestamp(self, timestamp_seconds: int) -> None:
        with self._lock:
            self._progress.set_block_timestamp(timestamp_seconds)

    # --- Base block info (L1Manager callback) ---
    def set_base_block_hash(self, base_block_hash_hex: str) -> None:
        with self._lock:
            self._progress.set_base_block_hash(base_block_hash_hex)

    def get_base_block_info(self) -> tuple[int, int]:
        with self._lock:
            return self._progress.get_base_block_info(default_start_block=CONFIG.blocks.start_block)

    def get_current_start_block(self, default_start_block: int) -> int:
        with self._lock:
            return self._progress.get_current_start_block(default_start_block=default_start_block)

    def get_last_proved_block_callback(self) -> tuple[int, int]:
        return self.get_base_block_info()


_shared: Optional["SharedContext"] = None


def get_shared_context() -> "SharedContext":
    """
    Lazily return the process-wide SharedContext instance.
    """
    global _shared
    if _shared is None:
        _shared = SharedContext()
    return _shared


shared = get_shared_context()

_l1_client = L1Client(api_key=CONFIG.l1.l1_provider_api_key)
l1_manager: L1Manager = L1Manager(_l1_client, shared.get_last_proved_block_callback)
