from __future__ import annotations

import glob
import json
import shutil
import threading
import time
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import ClassVar, Dict, List, Mapping, Optional, Sequence, Set

from echonet.echonet_types import (
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


def _find_archived_block_path(*, block_number: int, field: str) -> Optional[Path]:
    """
    Find the newest archived `{field}_{block_number}.json` under `CONFIG.paths.log_dir/blocks_*`.
    """
    filename = f"{field}_{int(block_number)}.json"
    root = CONFIG.paths.log_dir
    if not root.exists():
        return None

    pattern = str(root / "blocks_*" / filename)
    matches = [Path(p) for p in glob.glob(pattern)]

    candidates = [p for p in matches if p.is_file()]
    if not candidates:
        return None
    return max(candidates, key=lambda p: p.parent.name)


@dataclass(slots=True)
class _TxTracker:
    """Transaction lifecycle and counters used by reporting."""

    currently_pending: Dict[str, int]  # tx_hash -> source block number
    tx_block_metadata: Dict[
        str, Dict[str, int]
    ]  # tx_hash -> {"timestamp", "block_number"}; live only
    ever_seen_pending: Set[str]  # cumulative set of tx hashes ever observed pending
    committed: Dict[str, int]  # cumulative map: tx_hash -> commit block number
    total_forwarded_tx_count: int  # count of forwarded txs (counted once per block)
    max_forwarded_block: int  # highest block included in total_forwarded_tx_count

    @classmethod
    def empty(cls) -> "_TxTracker":
        return cls(
            currently_pending={},
            tx_block_metadata={},
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
        self.tx_block_metadata.pop(tx_hash, None)

    def record_forwarded_block(self, block_number: int, tx_count: int) -> None:
        if block_number > self.max_forwarded_block:
            self.max_forwarded_block = block_number
            self.total_forwarded_tx_count += tx_count


@dataclass(slots=True)
class _TxErrorTracker:
    """Gateway + revert error tracking (live vs cumulative) for reporting."""

    # TODO(Ron): Have all report objects contain epoch, and as such duplicate info could be removed (this is already in progress)
    gateway_errors_live: Dict[str, JsonObject]  # reset on resync
    gateway_errors: Dict[str, JsonObject]  # cumulative across resyncs
    echonet_only_reverts_live: Dict[str, RevertErrorInfo]  # reset on resync
    revert_errors_mainnet: Dict[str, RevertErrorInfo]  # tx_hash -> {block_number, error}
    revert_errors_echonet: Dict[str, RevertErrorInfo]  # tx_hash -> {block_number, error}

    @classmethod
    def empty(cls) -> "_TxErrorTracker":
        return cls(
            gateway_errors_live={},
            gateway_errors={},
            echonet_only_reverts_live={},
            revert_errors_mainnet={},
            revert_errors_echonet={},
        )

    def record_gateway_error(
        self, tx_hash: str, status: int, response: str, block_number: int
    ) -> None:
        payload = {
            "status": status,
            "response": response,
            "block_number": block_number,
        }
        self.gateway_errors_live[tx_hash] = payload
        self.gateway_errors[tx_hash] = payload

    def record_mainnet_revert_error(self, tx_hash: str, error: str, block_number: int) -> None:
        self.revert_errors_mainnet[tx_hash] = create_revert_error_info(
            block_number=block_number, error=error
        )

    def record_echonet_revert_error(
        self, tx_hash: str, error: str, source_block_number: int
    ) -> None:
        # If we already have a mainnet revert for this tx, treat as matched and drop it.
        if tx_hash in self.revert_errors_mainnet:
            self.revert_errors_mainnet.pop(tx_hash, None)
        else:
            info = create_revert_error_info(block_number=source_block_number, error=error)
            self.revert_errors_echonet[tx_hash] = info
            self.echonet_only_reverts_live[tx_hash] = info

    def clear_live(self) -> None:
        self.gateway_errors_live.clear()
        self.echonet_only_reverts_live.clear()


@dataclass(slots=True)
class _ResyncTracker:
    """Tracks resync triggers and promotes repeated errors to 'certain failures'."""

    resync_causes: ResyncTriggerMap
    certain_failures: ResyncTriggerMap

    @classmethod
    def empty(cls) -> "_ResyncTracker":
        return cls(resync_causes={}, certain_failures={})

    def record_cause(
        self,
        tx_hash: str,
        failure_block_number: int,
        revert_target_block_number: int,
        reason: str,
    ) -> tuple[bool, int]:
        def _selected_start_block() -> int:
            # Repeated trigger (same tx_hash seen again) means the previous resync
            # did not clear the issue; move start to just after the latest failing
            # block to avoid replaying the known-bad failing block endlessly.
            return failure_block_number + 1 if is_repeated_trigger else revert_target_block_number

        entry = self.certain_failures.get(tx_hash)
        is_repeated_trigger = entry is not None
        if entry:
            revert_target_block_number = _selected_start_block()
            entry["count"] += 1
            entry["failure_block_number"] = failure_block_number
            entry["revert_target_block_number"] = revert_target_block_number
            entry["reason"] = reason
            return True, revert_target_block_number

        entry = dict(self.resync_causes.pop(tx_hash, {}))
        is_repeated_trigger = bool(entry)
        if entry:
            revert_target_block_number = _selected_start_block()
            entry["count"] += 1
            entry["failure_block_number"] = failure_block_number
            entry["revert_target_block_number"] = revert_target_block_number
            entry["reason"] = reason
            self.certain_failures[tx_hash] = entry
            return True, revert_target_block_number

        revert_target_block_number = _selected_start_block()

        self.resync_causes[tx_hash] = {
            "tx_hash": tx_hash,
            "failure_block_number": failure_block_number,
            "revert_target_block_number": revert_target_block_number,
            "reason": reason,
            "count": 1,
        }
        return False, revert_target_block_number


@dataclass(slots=True)
class _ReportL2GasMismatchTracker:
    """Tracks L2 gas mismatch rows for reporting."""

    l2_gas_mismatches: List[JsonObject]

    @classmethod
    def empty(cls) -> "_ReportL2GasMismatchTracker":
        return cls(
            l2_gas_mismatches=[],
        )

    def record_l2_gas_mismatch(
        self,
        *,
        tx_hash: str,
        echo_block: int,
        source_block: int,
        blob_total_gas_l2: int,
        fgw_total_gas_consumed_l2: int | None,
    ) -> None:
        self.l2_gas_mismatches.append(
            {
                "tx_hash": tx_hash,
                "echo_block": echo_block,
                "source_block": source_block,
                "blob_total_gas_l2": blob_total_gas_l2,
                "fgw_total_gas_consumed_l2": fgw_total_gas_consumed_l2,
            }
        )


@dataclass(slots=True)
class _BlockHashMismatchTracker:
    """Tracks block hash and transaction commitment mismatch rows for reporting."""

    block_hash_mismatches: List[JsonObject]
    transaction_commitment_mismatches: List[JsonObject]
    # Earliest mismatch block since last resync; None if no mismatches since last resync.
    pending_mismatch_block: Optional[int]

    @classmethod
    def empty(cls) -> "_BlockHashMismatchTracker":
        return cls(
            block_hash_mismatches=[],
            transaction_commitment_mismatches=[],
            pending_mismatch_block=None,
        )

    def record_block_hash_mismatch(self, block_number: int, echonet: str, mainnet: str) -> None:
        self.block_hash_mismatches.append(
            {"block_number": block_number, "echonet": echonet, "mainnet": mainnet}
        )
        if self.pending_mismatch_block is None:
            self.pending_mismatch_block = block_number
        else:
            self.pending_mismatch_block = min(self.pending_mismatch_block, block_number)

    def clear_pending_mismatch(self) -> None:
        self.pending_mismatch_block = None

    def record_transaction_commitment_mismatch(
        self, block_number: int, echonet: str, mainnet: str
    ) -> None:
        self.transaction_commitment_mismatches.append(
            {"block_number": block_number, "echonet": echonet, "mainnet": mainnet}
        )


@dataclass(slots=True)
class _BlockStore:
    """In-memory storage for echo_center outputs and raw feeder blocks."""

    _MAX_BLOCKS_ARCHIVES_BYTES: ClassVar[int] = 90 * 1024 * 1024 * 1024  # 30 GiB
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
            (p for p in map(Path, glob.glob(str(root_dir / "blocks_*"))) if p.is_dir()),
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
        cutoff = current_block_number - CONFIG.block_store.max_blocks_to_keep_in_memory
        evict_bns = [bn for bn in store.keys() if bn < cutoff]
        return [(bn, store.pop(bn)) for bn in sorted(evict_bns)]

    def _evict_old_blob_bodies(self, current_block_number: int) -> List[tuple[int, bytes]]:
        """
        Drop in-memory `blob_body` for entries older than the blob retention
        window (the entry itself stays — block doc + state_update remain
        available for the tx sender). Returns the (block_number, blob_bytes)
        pairs that were dropped so the caller can archive them.
        """
        cutoff = current_block_number - CONFIG.block_store.max_blob_bodies_to_keep_in_memory
        evicted: List[tuple[int, bytes]] = []
        for bn, entry in self.blocks.items():
            if bn >= cutoff:
                continue
            blob_body = entry.get("blob_body")
            if blob_body is None:
                continue
            evicted.append((bn, blob_body))
            entry["blob_body"] = None
        return evicted

    # --- Block store API ---
    def store_block(
        self,
        block_number: int,
        blob_body: bytes,
        fgw_block: JsonObject,
        state_update: JsonObject,
        block_commitments: JsonObject,
    ) -> tuple[List[tuple[int, JsonObject]], List[tuple[int, bytes]]]:
        # MEMORY: store the blob as raw JSON bytes, not the parsed Python
        # dict. A 6 MB blob parses to 30-50 MB of Python heap (millions of
        # tiny dict/list/str/int objects, each with ~85 byte CPython
        # overhead) which glibc then backs with 4-6× that in arena pages.
        # At max_blocks_to_keep_in_memory=250 the parsed-dict variant
        # reliably OOMs the pod above 16 GiB. Bytes give a 5-8× reduction
        # without losing any information — consumers parse just-in-time.
        #
        # `block_commitments` is the 5-felt `BlockHeaderCommitments` dict
        # (returned by the block-hash CLI at ingest); stashed here so the
        # OS runner worker can splice it into its input without re-parsing
        # `blob_body` just to recompute the same values.
        self.blocks[block_number] = {
            "blob_body": blob_body,
            "block": fgw_block,
            "state_update": state_update,
            "block_commitments": block_commitments,
        }
        evicted_items = self._evict_old_items(self.blocks, current_block_number=block_number)
        # Blob-body retention is tighter than block retention; drop in-memory
        # blob_body for entries that have aged past `max_blob_bodies_to_keep_in_memory`
        # but are still within `max_blocks_to_keep_in_memory`.
        evicted_blob_bodies = self._evict_old_blob_bodies(current_block_number=block_number)
        return evicted_items, evicted_blob_bodies

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
                blob_body = entry.get("blob_body")
                if blob_body is not None:
                    (base_dir / f"blob_{bn}.json").write_bytes(blob_body)
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

    @staticmethod
    def write_blob_bodies_to_disk(blob_body_items: List[tuple[int, bytes]], base_dir: Path) -> None:
        """
        Archive blob bodies that were evicted from memory while their parent
        entry (block doc + state_update) stays cached. The OS runner reads them
        back via `get_blob_body_with_disk_fallback`.
        """
        try:
            for bn, blob_body in blob_body_items:
                (base_dir / f"blob_{bn}.json").write_bytes(blob_body)
            _BlockStore._enforce_blocks_archives_size_cap()
        except Exception as e:
            logger.error(f"Failed to archive evicted blob bodies to disk: {e}")


@dataclass(slots=True)
class _OsRunStats:
    """
    Cumulative counters for OS-runner outcomes, surfaced in the echonet report.
    Match the log-line categories emitted from `echo_center._maybe_run_os` /
    `_try_enqueue_os_run` / `_os_run_worker`.

    `recent_failures` is a rolling list of `{block_number, ts, error}` entries
    for the last N hard failures — exposed in the report so a quick read can
    map a failure count to actual blocks + the panic line that caused them.
    """

    completed: int
    failed: int
    dropped: int
    deferred_events: int
    abandoned: int
    skipped: int
    recent_failures: List[JsonObject]

    @classmethod
    def empty(cls) -> "_OsRunStats":
        return cls(
            completed=0,
            failed=0,
            dropped=0,
            deferred_events=0,
            abandoned=0,
            skipped=0,
            recent_failures=[],
        )


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
        self._started_at_monotonic = time.monotonic()
        self._tx = _TxTracker.empty()
        self._errors = _TxErrorTracker.empty()
        self._resync = _ResyncTracker.empty()
        self._l2_gas_mismatches = _ReportL2GasMismatchTracker.empty()
        self._hash_mismatches = _BlockHashMismatchTracker.empty()
        self._blocks = _BlockStore.empty()
        self._progress = _ProgressMarkers.empty()
        self._os_runs = _OsRunStats.empty()
        self._os_run_live = {"queue_depth": 0, "queue_max": 0, "deferred_count": 0}
        # State-commitment-info store, populated from every incoming blob's
        # `recent_state_commitment_infos` vector. Two purposes:
        #
        # 1. OS-run input building: with the sequencer's cende-commitment-delta
        #    optimization, blob N+1 may no longer carry N's commit (since we
        #    told the sequencer we already have it). So we keep our own copy
        #    instead of re-extracting from the blob at OS-run time.
        # 2. The `last_stored_commitment_height` query — the sequencer skips
        #    every commit at-or-below the height we return, so we must report
        #    the highest *contiguous* block we hold (no internal gaps), else
        #    the missing block's commit is dropped forever from the wire.
        #
        # Cap the dict so it doesn't grow unbounded; trim oldest entries beyond
        # `_COMMITS_RETENTION` heights. Not persisted across pod restart — the
        # sequencer falls back to its full window when we return null/None.
        self._commits_by_block: Dict[int, JsonObject] = {}
        self._last_stored_commitment_height: Optional[int] = None
        self._epoch = 0

    def get_uptime_seconds(self) -> int:
        return int(time.monotonic() - self._started_at_monotonic)

    def get_epoch(self) -> int:
        with self._lock:
            return self._epoch

    # --- OS-runner stats ---
    def record_os_run_completed(self) -> None:
        with self._lock:
            self._os_runs.completed += 1

    # Cap on per-block-failure entries kept for the report. Older entries get
    # dropped FIFO; the report shows "most recent N failures" — enough to spot
    # patterns without growing unbounded.
    _OS_RUN_FAILURES_RETENTION: int = 50

    def record_os_run_failed(self, block_number: int, error: str) -> None:
        # `error` is the exception message + worker subprocess stderr; capped
        # generously so a full Cairo traceback fits but the report stays
        # bounded. Full text always survives in pod logs.
        entry = {
            "block_number": int(block_number),
            "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "error": error[:4096],
        }
        with self._lock:
            self._os_runs.failed += 1
            self._os_runs.recent_failures.append(entry)
            overflow = len(self._os_runs.recent_failures) - SharedContext._OS_RUN_FAILURES_RETENTION
            if overflow > 0:
                del self._os_runs.recent_failures[:overflow]

    def record_os_run_dropped(self) -> None:
        with self._lock:
            self._os_runs.dropped += 1

    def record_os_run_deferred(self) -> None:
        with self._lock:
            self._os_runs.deferred_events += 1

    def record_os_run_abandoned(self) -> None:
        with self._lock:
            self._os_runs.abandoned += 1

    def record_os_run_skipped(self) -> None:
        with self._lock:
            self._os_runs.skipped += 1

    def set_os_run_live_state(self, queue_depth: int, queue_max: int, deferred_count: int) -> None:
        with self._lock:
            self._os_run_live["queue_depth"] = queue_depth
            self._os_run_live["queue_max"] = queue_max
            self._os_run_live["deferred_count"] = deferred_count

    def _os_run_stats_dict_locked(self) -> JsonObject:
        """Caller must hold `self._lock`. Shape mirrored by
        `get_report_snapshot`'s `os_run_stats` field — keep in sync."""
        return {
            "completed": self._os_runs.completed,
            "failed": self._os_runs.failed,
            "dropped": self._os_runs.dropped,
            "deferred_events": self._os_runs.deferred_events,
            "abandoned": self._os_runs.abandoned,
            "skipped": self._os_runs.skipped,
            "queue_depth": self._os_run_live["queue_depth"],
            "queue_max": self._os_run_live["queue_max"],
            "currently_deferred": self._os_run_live["deferred_count"],
            "recent_failures": list(self._os_runs.recent_failures),
        }

    def get_os_run_stats(self) -> JsonObject:
        with self._lock:
            return self._os_run_stats_dict_locked()

    # --- Cende-recorder commitment-info tracking ---
    # Retain just the deferred-retry window's worth of commits. Once a block
    # ages past `_RECENT_STATE_COMMITMENT_WINDOW` (10) heights it's permanently
    # abandoned for OS runs (see `_maybe_run_os` in echo_center), so older
    # commits are dead weight. Workers hold their task's commit directly after
    # enqueue, so the store doesn't need to span the OS-run duration either.
    _COMMITS_RETENTION: int = 10

    def get_last_stored_commitment_height(self) -> Optional[int]:
        with self._lock:
            return self._last_stored_commitment_height

    def get_state_commitment_infos(self, block_number: int) -> Optional[JsonObject]:
        """Return the stored `state_commitment_infos` for `block_number`, or None."""
        with self._lock:
            return self._commits_by_block.get(int(block_number))

    def record_commits_from_blob(self, blob: JsonObject) -> None:
        """
        Persist every `state_commitment_infos` entry in `blob`'s
        `recent_state_commitment_infos` vector so OS-runs can look them up
        directly (instead of re-extracting from the blob, which post
        cende-commitment-delta may not carry them anymore).

        Also bumps `last_stored_commitment_height` only by *contiguous*
        extension — never claiming a height beyond which we have a gap. The
        sequencer skips every commit at-or-below the returned height, so
        announcing a non-contiguous max would permanently lose any held-back
        commit in the gap.
        """
        entries = blob.get("recent_state_commitment_infos") or []
        if not entries:
            return
        with self._lock:
            # Store every entry we don't already have.
            for entry in entries:
                bn = int(entry["block_number"])
                if bn in self._commits_by_block:
                    continue
                self._commits_by_block[bn] = entry["state_commitment_infos"]

            # Bump the contiguous-stored-height by extending forward block by
            # block while present in the dict. First-time path: anchor at the
            # min block_number we just inserted.
            if self._last_stored_commitment_height is None:
                self._last_stored_commitment_height = min(self._commits_by_block) - 1
            while (self._last_stored_commitment_height + 1) in self._commits_by_block:
                self._last_stored_commitment_height += 1

            # Trim entries we no longer need (older than the contiguous head by
            # `_COMMITS_RETENTION`). We never trim above the head — that's the
            # window the OS-runner actually consumes.
            if len(self._commits_by_block) > SharedContext._COMMITS_RETENTION:
                cutoff = self._last_stored_commitment_height - SharedContext._COMMITS_RETENTION
                stale = [bn for bn in self._commits_by_block if bn < cutoff]
                for bn in stale:
                    self._commits_by_block.pop(bn, None)

    # --- Tx lifecycle ---
    def record_sent_tx(self, tx_hash: str, source_block_number: int) -> None:
        with self._lock:
            self._tx.record_sent(tx_hash, source_block_number)

    def record_sent_tx_block_metadata_for_block(
        self, txs: Sequence[JsonObject], timestamp: int, block_number: int
    ) -> None:
        with self._lock:
            for tx in txs:
                self._tx.tx_block_metadata[tx["transaction_hash"]] = {
                    "timestamp": timestamp,
                    "block_number": block_number,
                }

    def get_sent_tx_timestamp_and_block_number(self, tx_hash: str) -> Dict[str, int]:
        with self._lock:
            return self._tx.tx_block_metadata[tx_hash]

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

    def get_resync_evaluation_inputs(
        self,
    ) -> tuple[Dict[str, JsonObject], Dict[str, int], Dict[str, RevertErrorInfo], Optional[int]]:
        """
        Return the minimal live state needed by transaction_sender's resync policy:
        - gateway_errors_live (tx_hash -> {status, response, block_number})
        - currently_pending (tx_hash -> source block number)
        - echonet_only_reverts_live (tx_hash -> {block_number, error})
        - pending_mismatch_block: earliest block with a hash mismatch since last resync, or None
        """
        with self._lock:
            return (
                dict(self._errors.gateway_errors_live),
                dict(self._tx.currently_pending),
                dict(self._errors.echonet_only_reverts_live),
                self._hash_mismatches.pending_mismatch_block,
            )

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
    def record_resync_cause(
        self,
        tx_hash: str,
        failure_block_number: int,
        revert_target_block_number: int,
        reason: str,
    ) -> tuple[bool, int]:
        with self._lock:
            return self._resync.record_cause(
                tx_hash, failure_block_number, revert_target_block_number, reason
            )

    def clear_for_resync(self) -> None:
        """Clear live state for a new run while preserving cumulative stats."""
        with self._lock:
            self._epoch += 1
            snapshot_items = self._blocks.snapshot_items()
            archive_dir = self._blocks._ensure_archive_dir()
            self._tx.currently_pending.clear()
            self._tx.tx_block_metadata.clear()
            self._errors.clear_live()
            self._blocks.clear_live()
            self._hash_mismatches.clear_pending_mismatch()
            self._progress.last_echo_center_block = None
            self._progress.sender_current_block = None
        _BlockStore.write_snapshot_items_to_disk(snapshot_items, base_dir=archive_dir)
        l1_manager.clear_stored_blocks()

    # --- Report extras (cumulative; preserved across resync) ---
    def record_l2_gas_mismatch(
        self,
        *,
        tx_hash: str,
        echo_block: int,
        source_block: int,
        blob_total_gas_l2: int,
        fgw_total_gas_consumed_l2: int | None,
    ) -> None:
        with self._lock:
            self._l2_gas_mismatches.record_l2_gas_mismatch(
                tx_hash=tx_hash,
                echo_block=echo_block,
                source_block=source_block,
                blob_total_gas_l2=blob_total_gas_l2,
                fgw_total_gas_consumed_l2=fgw_total_gas_consumed_l2,
            )

    # --- Block hash and commitment mismatch tracking ---
    def record_block_hash_mismatch(self, block_number: int, echonet: str, mainnet: str) -> None:
        with self._lock:
            self._hash_mismatches.record_block_hash_mismatch(block_number, echonet, mainnet)

    def record_transaction_commitment_mismatch(
        self, block_number: int, echonet: str, mainnet: str
    ) -> None:
        with self._lock:
            self._hash_mismatches.record_transaction_commitment_mismatch(
                block_number, echonet, mainnet
            )

    # --- Block storage (echo_center output + raw FGW blocks) ---
    def store_block(
        self,
        block_number: int,
        blob_body: bytes,
        fgw_block: JsonObject,
        state_update: JsonObject,
        block_commitments: JsonObject,
    ) -> None:
        with self._lock:
            evicted_items, evicted_blob_bodies = self._blocks.store_block(
                block_number,
                blob_body=blob_body,
                fgw_block=fgw_block,
                state_update=state_update,
                block_commitments=block_commitments,
            )
        if evicted_items:
            _BlockStore.write_snapshot_items_to_disk(
                evicted_items, base_dir=self._blocks._ensure_archive_dir()
            )
        if evicted_blob_bodies:
            _BlockStore.write_blob_bodies_to_disk(
                evicted_blob_bodies, base_dir=self._blocks._ensure_archive_dir()
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

    def get_block_field_with_disk_fallback(
        self, block_number: int, field: str
    ) -> Optional[JsonObject]:
        """Return an in-memory stored block payload, falling back to on-disk archives."""
        in_mem = self.get_block_field(block_number, field)
        if in_mem:
            return in_mem

        path = _find_archived_block_path(block_number=block_number, field=field)
        if not path:
            return None
        try:
            return json.loads(path.read_text(encoding="utf-8"))
        except Exception as e:
            logger.warning(f"Failed reading archived block dump {path}: {e}")
            return None

    def get_blob_body_with_disk_fallback(self, block_number: int) -> Optional[bytes]:
        """
        Return the raw blob bytes for `block_number` — from memory if cached,
        else from the on-disk archive (`blob_<N>.json`, which contains the
        JSON body verbatim per `_BlockStore.write_snapshot_items_to_disk`).
        """
        in_mem = self.get_block_field(block_number, "blob_body")
        if in_mem is not None:
            if isinstance(in_mem, str):
                return in_mem.encode("utf-8")
            return in_mem
        path = _find_archived_block_path(block_number=block_number, field="blob")
        if not path:
            return None
        try:
            return path.read_bytes()
        except Exception as e:
            logger.warning(f"Failed reading archived blob {path}: {e}")
            return None

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
            uptime_seconds = int(time.monotonic() - self._started_at_monotonic)

            return SnapshotModel(
                start_block=configured_start_block,
                initial_start_block=initial_start_block,
                current_start_block=current_start_block,
                current_block=current_block,
                blocks_sent_count=blocks_sent_count,
                first_block_timestamp=first_ts,
                latest_block_timestamp=latest_ts,
                timestamp_diff_seconds=timestamp_diff_seconds,
                uptime_seconds=uptime_seconds,
                total_sent_tx_count=self._tx.total_forwarded_tx_count,
                committed_count=len(self._tx.committed),
                pending_total_count=len(self._tx.ever_seen_pending),
                pending_commission_count=len(self._tx.ever_seen_pending) - len(self._tx.committed),
                sent_tx_hashes=dict(self._tx.currently_pending),
                gateway_errors=dict(self._errors.gateway_errors),
                revert_errors_mainnet=dict(self._errors.revert_errors_mainnet),
                revert_errors_echonet=dict(self._errors.revert_errors_echonet),
                resync_causes=dict(self._resync.resync_causes),
                certain_failures=dict(self._resync.certain_failures),
                l2_gas_mismatches=list(self._l2_gas_mismatches.l2_gas_mismatches),
                block_hash_mismatches=list(self._hash_mismatches.block_hash_mismatches),
                transaction_commitment_mismatches=list(
                    self._hash_mismatches.transaction_commitment_mismatches
                ),
                os_run_stats=self._os_run_stats_dict_locked(),
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

_l1_client = L1Client(api_key=CONFIG.l1.l1_events_provider_api_key)
l1_manager: L1Manager = L1Manager(
    _l1_client, shared.get_last_proved_block_callback, shared.get_last_block
)
