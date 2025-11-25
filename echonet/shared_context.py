from typing import Any, Dict, List, Optional, Set

import threading


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
        # tx_hash -> response text
        self.gateway_errors: Dict[str, str] = {}
        # block_number -> {"blob": dict, "block": dict, "state_update": dict}
        self.blocks: Dict[int, Dict[str, Any]] = {}
        # Real FGW blocks by their FGW block number
        self.fgw_blocks: Dict[int, Any] = {}
        # Extend with other shared, live-updated values as needed
        # e.g., self.last_block: int = 0

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

    def mark_committed_tx(self, tx_hash: str, block_number: int) -> None:
        if not isinstance(tx_hash, str) or not tx_hash:
            return
        k = tx_hash.lower()
        with self.lock:
            self.committed_tx_hashes[k] = int(block_number)
            if k in self.sent_tx_hashes:
                self.sent_tx_hashes.pop(k, None)

    def record_gateway_error(self, tx_hash: str, status: int, response: str) -> None:
        if not isinstance(tx_hash, str) or not tx_hash:
            return
        k = tx_hash.lower()
        with self.lock:
            self.gateway_errors[k] = response

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
        return {
            "sent_tx_hashes": sent,
            "committed_count": committed_count,
            "revert_errors_mainnet": reverts_mainnet,
            "revert_errors_echonet": reverts_echonet,
            "gateway_errors": gateway_errors,
        }


shared = SharedContext()
