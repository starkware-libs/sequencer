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
    ) -> Optional[ResyncTriggerPayload]:
        threshold_block = current_block - self._blocks_to_wait_before_failing_tx

        candidates: list[tuple[str, int, str]] = []
        pending_min_block = min(sent_tx_hashes.values()) if sent_tx_hashes else None

        for tx_hash, error in gateway_errors.items():
            block_number = error["block_number"]
            candidates.append((tx_hash, block_number, f"Gateway error: {error['response']}"))

        for tx_hash, info in echonet_only_reverts.items():
            src_block = info["block_number"]
            candidates.append(
                (
                    tx_hash,
                    src_block,
                    f"Echonet-only revert: {info['error']}",
                )
            )

        for tx_hash, block_number in sent_tx_hashes.items():
            if block_number <= threshold_block:
                candidates.append(
                    (
                        tx_hash,
                        block_number,
                        f"Still pending after >= {self._blocks_to_wait_before_failing_tx} blocks",
                    )
                )

        if not candidates:
            return None

        tx_hash_trigger, failing_tx_block, reason_trigger = min(
            candidates, key=lambda item: item[1]
        )
        rollback_block = failing_tx_block
        if pending_min_block is not None:
            rollback_block = min(rollback_block, pending_min_block)

        return {
            "tx_hash": tx_hash_trigger,
            "block_number": rollback_block,
            "source_block_number": failing_tx_block,
            "reason": (
                f"{reason_trigger}; failing_bn={failing_tx_block}; "
                f"pending_min_bn={pending_min_block}; rollback_bn={rollback_block}"
            ),
        }


class ResyncExecutor:
    """Run the resync flow and update shared/global start-block state."""

    def __init__(self, get_sequencer_manager: Callable[[], SequencerManager]) -> None:
        self._get_sequencer_manager = get_sequencer_manager

    @staticmethod
    def _next_start_block(trigger: ResyncTriggerPayload, is_repeated_trigger: bool) -> int:
        """
        On repeated triggers for the same transaction, skip to the block after
        the failing transaction's source block.
        """
        if not is_repeated_trigger:
            return trigger["block_number"]

        source_block = trigger.get("source_block_number")
        if source_block is not None:
            return source_block + 1

        return trigger["block_number"] + 1

    async def execute(self, trigger: ResyncTriggerPayload) -> int:
        failing_tx_block = int(trigger.get("source_block_number", trigger["block_number"]))
        is_repeated_trigger = shared.record_resync_cause(
            trigger["tx_hash"], failing_tx_block, trigger["reason"]
        )
        next_start_block = self._next_start_block(
            trigger=trigger, is_repeated_trigger=is_repeated_trigger
        )

        self._get_sequencer_manager().scale_to_zero()
        reports.write_pre_resync_reports(
            trigger_tx_hash=trigger["tx_hash"],
            trigger_block=trigger["block_number"],
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
