from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping

JsonObject = dict[str, Any]


@dataclass(frozen=True, slots=True)
class SnapshotModel:
    """
    Typed view of the `/echonet/report` payload.

    This model is shared between:
    - `shared_context`: the producer of in-memory report state
    - `echo_center`: the HTTP endpoint that serves the snapshot
    - `reports`: CLI + report renderers + pre-resync writers
    """

    # Progress / context
    start_block: int
    initial_start_block: int | None
    current_start_block: int | None
    current_block: int | None
    blocks_sent_count: int | None

    first_block_timestamp: int | None
    latest_block_timestamp: int | None
    timestamp_diff_seconds: int | None

    # Counters
    total_sent_tx_count: int
    committed_count: int
    pending_total_count: int
    pending_not_committed_count: int

    # Maps
    sent_tx_hashes: Mapping[str, Any]
    gateway_errors: Mapping[str, Any]
    revert_errors_mainnet: Mapping[str, str]
    revert_errors_echonet: Mapping[str, str]
    resync_causes: Mapping[str, Any]
    certain_failures: Mapping[str, Any]

    def to_dict(self) -> JsonObject:
        """
        Convert to a JSON-serializable dict compatible with `/echonet/report`.

        Note: callers may still include additional keys in the HTTP response if desired,
        but this method defines the canonical payload.
        """
        return {
            "sent_tx_hashes": self.sent_tx_hashes,
            "committed_count": self.committed_count,
            "revert_errors_mainnet": self.revert_errors_mainnet,
            "revert_errors_echonet": self.revert_errors_echonet,
            "gateway_errors": self.gateway_errors,
            "resync_causes": self.resync_causes,
            "certain_failures": self.certain_failures,
            "total_sent_tx_count": self.total_sent_tx_count,
            "pending_total_count": self.pending_total_count,
            "pending_not_committed_count": self.pending_not_committed_count,
            "blocks_sent_count": self.blocks_sent_count,
            "start_block": self.start_block,
            "current_block": self.current_block,
            "initial_start_block": self.initial_start_block,
            "current_start_block": self.current_start_block,
            "first_block_timestamp": self.first_block_timestamp,
            "latest_block_timestamp": self.latest_block_timestamp,
            "timestamp_diff_seconds": self.timestamp_diff_seconds,
        }

    @classmethod
    def from_dict(cls, data: Mapping[str, Any]) -> "SnapshotModel":
        return cls(
            start_block=data["start_block"],
            initial_start_block=data["initial_start_block"],
            current_start_block=data["current_start_block"],
            current_block=data["current_block"],
            blocks_sent_count=data["blocks_sent_count"],
            first_block_timestamp=data["first_block_timestamp"],
            latest_block_timestamp=data["latest_block_timestamp"],
            timestamp_diff_seconds=data["timestamp_diff_seconds"],
            total_sent_tx_count=data["total_sent_tx_count"],
            committed_count=data["committed_count"],
            pending_total_count=data["pending_total_count"],
            pending_not_committed_count=data["pending_not_committed_count"],
            sent_tx_hashes=data["sent_tx_hashes"],
            gateway_errors=data["gateway_errors"],
            revert_errors_mainnet=data["revert_errors_mainnet"],
            revert_errors_echonet=data["revert_errors_echonet"],
            resync_causes=data["resync_causes"],
            certain_failures=data["certain_failures"],
        )
