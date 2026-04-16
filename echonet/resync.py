from __future__ import annotations

import asyncio
from typing import Callable, Dict, Optional

from echonet import reports
from echonet.echonet_types import JsonObject, ResyncTriggerPayload, RevertErrorInfo
from echonet.logger import get_logger
from echonet.sequencer_manager import SequencerManager
from echonet.shared_context import shared

logger = get_logger("transaction_sender")


class ResyncPolicy:
    """Decide whether the system has accumulated enough evidence to resync."""

    def __init__(self, blocks_to_wait_before_failing_tx: int) -> None:
        self._blocks_to_wait_before_failing_tx = blocks_to_wait_before_failing_tx

    def evaluate(
        self,
        gateway_errors: Dict[str, JsonObject],
        sent_tx_hashes: Dict[str, int],
        echonet_only_reverts: Dict[str, RevertErrorInfo],
        current_block: int,
        block_hash_mismatch_block: Optional[int] = None,
    ) -> Optional[ResyncTriggerPayload]:
        threshold_block = current_block - self._blocks_to_wait_before_failing_tx

        # We combine all error sources into one list so we can pick a single earliest
        # failure trigger across gateway errors, Echonet-only reverts, stale pending txs,
        # and block hash mismatches.
        candidates: list[tuple[str, int, str]] = []

        # Gateway errors are added.
        for tx_hash, error in gateway_errors.items():
            candidates.append(
                (tx_hash, error["block_number"], f"Gateway error: {error['response']}")
            )

        # Echonet-only reverts are added.
        for tx_hash, info in echonet_only_reverts.items():
            candidates.append(
                (
                    tx_hash,
                    info["block_number"],
                    f"Echonet-only revert: {info['error']}",
                )
            )

        # Pending transactions are added only after crossing the waiting threshold;
        # younger pending transactions are ignored.
        for tx_hash, block_number in sent_tx_hashes.items():
            if block_number <= threshold_block:
                candidates.append(
                    (
                        tx_hash,
                        block_number,
                        f"Still pending after >= {self._blocks_to_wait_before_failing_tx} blocks",
                    )
                )

        # Block hash mismatches are added using a synthetic key.
        if block_hash_mismatch_block is not None:
            candidates.append(
                (
                    f"block_hash_mismatch:{block_hash_mismatch_block}",
                    block_hash_mismatch_block,
                    f"Block hash mismatch at block {block_hash_mismatch_block}",
                )
            )

        # No candidates accumulated yet -> do not trigger resync.
        if not candidates:
            return None

        # The trigger is the earliest failing block.
        tx_hash_trigger, failure_block_number, reason_trigger = min(
            candidates, key=lambda item: item[1]
        )
        pending_min_block = min(sent_tx_hashes.values()) if sent_tx_hashes else None

        # - If there are pending txs, rewind to the earlier of:
        #   (a) trigger failure block, or (b) oldest pending tx block.
        # - If none are pending, rewind to the trigger failure block.
        revert_target_block_number = (
            min(failure_block_number, pending_min_block)
            if pending_min_block is not None
            else failure_block_number
        )

        return {
            "tx_hash": tx_hash_trigger,
            "failure_block_number": failure_block_number,
            "revert_target_block_number": revert_target_block_number,
            "reason": reason_trigger,
        }


class ResyncExecutor:
    """Run the resync flow and update shared/global start-block state."""

    def __init__(self, get_sequencer_manager: Callable[[], SequencerManager]) -> None:
        self._get_sequencer_manager = get_sequencer_manager

    async def execute(self, trigger: ResyncTriggerPayload) -> int:
        is_repeated_trigger, next_start_block = shared.record_resync_cause(
            trigger["tx_hash"],
            trigger["failure_block_number"],
            trigger["revert_target_block_number"],
            trigger["reason"],
        )

        self._get_sequencer_manager().scale_to_zero()
        reports.write_pre_resync_reports(
            trigger_tx_hash=trigger["tx_hash"],
            trigger_block=trigger["failure_block_number"],
            trigger_reason=trigger["reason"],
            snapshot=shared.get_report_snapshot(),
            logger=logger,
        )
        shared.clear_for_resync()

        loop = asyncio.get_running_loop()
        await loop.run_in_executor(
            None,
            lambda: self._get_sequencer_manager().resync(block_number=next_start_block),
        )

        shared.set_current_start_block(next_start_block)
        return next_start_block
