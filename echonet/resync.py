from __future__ import annotations

import asyncio
from typing import Callable, Dict, Optional

from echonet import reports
from echonet.echonet_types import CONFIG, JsonObject, ResyncTriggerPayload
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
        current_block: int,
    ) -> Optional[ResyncTriggerPayload]:
        threshold_block = current_block - self._blocks_to_wait_before_failing_tx

        candidates: list[tuple[str, int, str]] = []

        for tx_hash, error in gateway_errors.items():
            block_number = error["block_number"]
            if block_number <= threshold_block:
                candidates.append((tx_hash, block_number, f"Gateway error: {error['response']}"))

        for tx_hash, block_number in sent_tx_hashes.items():
            if block_number <= threshold_block:
                candidates.append(
                    (
                        tx_hash,
                        block_number,
                        f"Still pending after >= {self._blocks_to_wait_before_failing_tx} blocks",
                    )
                )

        if len(candidates) < CONFIG.resync.error_threshold:
            return None

        tx_hash_trigger, block_number_trigger, reason_trigger = min(
            candidates, key=lambda item: item[1]
        )
        return {
            "tx_hash": tx_hash_trigger,
            "block_number": block_number_trigger,
            "reason": f"Resync after >= {CONFIG.resync.error_threshold} errors: {reason_trigger}",
        }


class ResyncExecutor:
    """Run the resync flow and update shared/global start-block state."""

    def __init__(self, get_sequencer_manager: Callable[[], SequencerManager]) -> None:
        self._get_sequencer_manager = get_sequencer_manager

    async def execute(self, trigger: ResyncTriggerPayload) -> int:
        is_repeated_trigger = shared.record_resync_cause(
            trigger["tx_hash"], trigger["block_number"], trigger["reason"]
        )
        next_start_block = (
            trigger["block_number"] + 1 if is_repeated_trigger else trigger["block_number"]
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
