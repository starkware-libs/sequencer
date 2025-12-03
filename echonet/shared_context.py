from typing import Any, Dict, List, Optional, Set

import consts
import threading
from l1_manager import L1Manager


class SharedContext:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.blocked_senders: Set[str] = set()
        # tx_hash -> block number when forwarded
        self.sent_tx_hashes: Dict[str, int] = {}
        # tx_hash -> block number where it was committed
        self.committed_tx_hashes: Dict[str, int] = {}
        # tx_hash -> error message (from mainnet receipts)
        self.revert_errors_mainnet: Dict[str, str] = {}
        # tx_hash -> error message (from echonet execution infos)
        self.revert_errors_echonet: Dict[str, str] = {}
        # tx_hash -> { "status": int, "response": str, "block_number": int }
        self.gateway_errors: Dict[str, Any] = {}
        # block_number -> {"blob": dict, "block": dict, "state_update": dict}
        self.blocks: Dict[int, Dict[str, Any]] = {}
        # Real FGW blocks by their FGW block number
        self.fgw_blocks: Dict[int, Any] = {}
        # tx_hash -> { "tx_hash": str, "block_number": int, "reason": str, "count": int }
        # First time a tx triggers a resync, it is recorded here with count=1.
        self.resync_causes: Dict[str, Dict[str, Any]] = {}
        # tx_hash -> { "tx_hash": str, "block_number": int, "reason": str, "count": int }
        # If a tx triggers resync twice, it is moved here (certain failure).
        self.certain_failures: Dict[str, Dict[str, Any]] = {}
        # Extend with other shared, live-updated values as needed
        # Track the last block number processed by echo_center
        self.last_block: Optional[int] = None
        # Track the current block number being processed by transaction_sender
        self.sender_current_block: Optional[int] = None
        # Cumulative number of transactions forwarded by transaction_sender
        self.total_sent_tx_count: int = 0
        # Persisted across resyncs: the very first starting block the sender began with
        self.initial_start_block: Optional[int] = None
        # Updated whenever resync changes the starting point (mirrors current effective start)
        self.current_start_block: Optional[int] = None
        # Timestamps (as block timestamps, seconds since epoch) of first and latest processed blocks
        self.first_block_timestamp: Optional[int] = None
        self.latest_block_timestamp: Optional[int] = None

    def get_blocked_senders(self) -> Set[str]:
        with self.lock:
            return set(self.blocked_senders)

    # --- Transaction tracking helpers ---
    def record_sent_tx(self, tx_hash: str, source_block_number: int) -> None:
        if not isinstance(tx_hash, str) or not tx_hash:
            return
        k = tx_hash.lower()
        with self.lock:
            self.sent_tx_hashes[k] = int(source_block_number)

    def increment_total_sent(self, count: int) -> None:
        if not isinstance(count, int) or count <= 0:
            return
        with self.lock:
            self.total_sent_tx_count += count

    def mark_committed_tx(self, tx_hash: str, block_number: int) -> None:
        if not isinstance(tx_hash, str) or not tx_hash:
            return
        k = tx_hash.lower()
        with self.lock:
            self.committed_tx_hashes[k] = int(block_number)
            if k in self.sent_tx_hashes:
                self.sent_tx_hashes.pop(k, None)

    def record_gateway_error(
        self, tx_hash: str, status: int, response: str, *, block_number: int
    ) -> None:
        if not isinstance(tx_hash, str) or not tx_hash:
            return
        k = tx_hash.lower()
        with self.lock:
            self.gateway_errors[k] = {
                "status": int(status),
                "response": response,
                "block_number": int(block_number),
            }

    def add_mainnet_revert_error(self, tx_hash: str, error: str) -> None:
        if not isinstance(tx_hash, str) or not tx_hash:
            return
        k = tx_hash.lower()
        with self.lock:
            self.revert_errors_mainnet[k] = error

    def add_echonet_revert_error(self, tx_hash: str, error: str) -> None:
        if not isinstance(tx_hash, str) or not tx_hash:
            return
        k = tx_hash.lower()
        with self.lock:
            # If we already have a mainnet revert for this tx, treat as matched and remove it.
            if k in self.revert_errors_mainnet:
                self.revert_errors_mainnet.pop(k, None)
                # Do not record under echonet map in this case
                return
            # Otherwise record as echonet-only revert
            self.revert_errors_echonet[k] = error

    def get_sent_block_number(self, tx_hash: str) -> Optional[int]:
        k = tx_hash.lower()
        with self.lock:
            return self.sent_tx_hashes.get(k)

    # --- Resync tracking ---
    def record_resync_cause(self, tx_hash: str, block_number: int, reason: str) -> bool:
        """
        Record a resync cause by tx_hash. Returns True if this is a repeated failure
        (the tx already caused a resync before), in which case the entry is moved
        to certain_failures. Returns False if this is the first time.
        """
        if not isinstance(tx_hash, str) or not tx_hash:
            return False
        k = tx_hash.lower()
        with self.lock:
            if k in self.certain_failures:
                # Already certain failure; just bump count
                self.certain_failures[k]["count"] = (
                    int(self.certain_failures[k].get("count", 1)) + 1
                )
                return True
            if k in self.resync_causes:
                # Second time -> move to certain_failures
                entry = dict(self.resync_causes.pop(k))
                entry["count"] = int(entry.get("count", 1)) + 1
                entry["block_number"] = int(block_number)
                entry["reason"] = reason
                self.certain_failures[k] = entry
                return True
            # First time
            self.resync_causes[k] = {
                "tx_hash": k,
                "block_number": int(block_number),
                "reason": reason,
                "count": 1,
            }
            return False

    def clear_for_resync(self) -> None:
        """Clear transient tracking maps after a resync is initiated."""
        with self.lock:
            self.sent_tx_hashes.clear()
            self.committed_tx_hashes.clear()
            self.revert_errors_mainnet.clear()
            self.revert_errors_echonet.clear()
            self.gateway_errors.clear()
            self.blocks.clear()
            self.fgw_blocks.clear()
            self.last_block = None
            self.sender_current_block = None

    # --- Block storage helpers ---
    def store_block(
        self,
        block_number: int,
        *,
        blob: Dict[str, Any],
        block: Dict[str, Any],
        state_update: Dict[str, Any],
    ) -> None:
        with self.lock:
            self.blocks[int(block_number)] = {
                "blob": blob,
                "block": block,
                "state_update": state_update,
            }

    def store_fgw_block(self, block_number: int, block_obj: Any) -> None:
        with self.lock:
            self.fgw_blocks[int(block_number)] = block_obj

    def get_fgw_block(self, block_number: int) -> Optional[Any]:
        with self.lock:
            return self.fgw_blocks.get(int(block_number))

    def get_block_numbers_sorted(self) -> List[int]:
        with self.lock:
            return sorted(self.blocks.keys())

    def get_block_field(self, block_number: int, field: str) -> Optional[Any]:
        with self.lock:
            entry = self.blocks.get(int(block_number))
            if not entry:
                return None
            return entry.get(field)

    def get_latest_block_number(self) -> Optional[int]:
        with self.lock:
            if not self.blocks:
                return None
            return max(self.blocks.keys())

    def has_block(self, block_number: int) -> bool:
        with self.lock:
            return int(block_number) in self.blocks

    def has_any_blocks(self) -> bool:
        with self.lock:
            return bool(self.blocks)

    def get_report_snapshot(self) -> Dict[str, Any]:
        with self.lock:
            sent = dict(self.sent_tx_hashes)
            committed_count = len(self.committed_tx_hashes)
            # Copy the maps to avoid external mutation
            reverts_mainnet = dict(self.revert_errors_mainnet)
            reverts_echonet = dict(self.revert_errors_echonet)
            gateway_errors = dict(self.gateway_errors)
            resync_causes = dict(self.resync_causes)
            certain_failures = dict(self.certain_failures)
            # Effective current and start blocks
            current_block = self.sender_current_block
            # Persisted initial start; fallback to consts if not yet initialized
            initial_start_block = (
                self.initial_start_block
                if self.initial_start_block is not None
                else consts.START_BLOCK_DEFAULT
            )
            current_start_block = (
                self.current_start_block
                if self.current_start_block is not None
                else consts.START_BLOCK_DEFAULT
            )
            # Compute blocks processed as (current - initial_start), if both known
            blocks_sent_count = None  # number of blocks processed since initial start
            try:
                if current_block is not None and initial_start_block is not None:
                    blocks_sent_count = max(0, int(current_block) - int(initial_start_block))
            except Exception:
                blocks_sent_count = None
            total_sent_tx_count = int(self.total_sent_tx_count)
            # Timestamps (diff computed only if both known)
            first_ts = self.first_block_timestamp
            latest_ts = self.latest_block_timestamp
            timestamp_diff_seconds: Optional[int]
            try:
                timestamp_diff_seconds = (
                    int(latest_ts) - int(first_ts)
                    if (first_ts is not None and latest_ts is not None)
                    else None
                )
            except Exception:
                timestamp_diff_seconds = None
        return {
            "sent_tx_hashes": sent,
            "committed_count": committed_count,
            "revert_errors_mainnet": reverts_mainnet,
            "revert_errors_echonet": reverts_echonet,
            "gateway_errors": gateway_errors,
            "resync_causes": resync_causes,
            "certain_failures": certain_failures,
            "total_sent_tx_count": total_sent_tx_count,
            "blocks_sent_count": blocks_sent_count,
            "start_block": consts.START_BLOCK_DEFAULT,
            "current_block": current_block,
            # Report both initial and current effective start blocks
            "initial_start_block": initial_start_block,
            "current_start_block": current_start_block,
            # Timestamps and derived diff
            "first_block_timestamp": first_ts,
            "latest_block_timestamp": latest_ts,
            "timestamp_diff_seconds": timestamp_diff_seconds,
        }

    # --- Echo Center helpers ---
    def set_last_block(self, block_number: int) -> None:
        with self.lock:
            self.last_block = int(block_number)

    def get_last_block(self) -> Optional[int]:
        with self.lock:
            return self.last_block

    def set_sender_current_block(self, block_number: int) -> None:
        with self.lock:
            self.sender_current_block = int(block_number)

    def get_sender_current_block(self) -> Optional[int]:
        with self.lock:
            return self.sender_current_block

    # --- Start blocks (initial/current) helpers ---
    def set_initial_start_block_if_absent(self, block_number: int) -> None:
        with self.lock:
            if self.initial_start_block is None:
                self.initial_start_block = int(block_number)
            # Also initialize current start if not set
            if self.current_start_block is None:
                self.current_start_block = int(block_number)

    def set_current_start_block(self, block_number: int) -> None:
        with self.lock:
            self.current_start_block = int(block_number)

    # --- Timestamp helpers (block timestamps) ---
    def set_first_block_timestamp_if_absent(self, timestamp_seconds: int) -> None:
        with self.lock:
            if self.first_block_timestamp is None:
                self.first_block_timestamp = int(timestamp_seconds)

    def set_latest_block_timestamp(self, timestamp_seconds: int) -> None:
        with self.lock:
            self.latest_block_timestamp = int(timestamp_seconds)


shared = SharedContext()
# Global L1Manager instance shared across modules.
l1_manager: L1Manager = L1Manager()
