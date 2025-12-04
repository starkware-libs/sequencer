from typing import Optional

import logging
from l1_client import L1Client
from l1_events import L1Events

logger = logging.getLogger(__name__)


class L1Blocks:
    @staticmethod
    # TODO(Ayelet): Consider changing timestamp params to datetime.
    def find_l1_block_for_tx(
        feeder_tx: dict,
        l2_block_timestamp: int,
        client: L1Client,
        search_minutes_before: int = 5,
    ) -> Optional[int]:
        """
        Finds the L1 block number that contains the given L1 handler transaction.
        """
        if "transaction_hash" not in feeder_tx:
            logger.error("Feeder tx does not contain transaction_hash.")
            return None

        search_start_timestamp = l2_block_timestamp - (search_minutes_before * 60)
        search_end_timestamp = l2_block_timestamp

        start_block_data = client.get_block_number_by_timestamp(search_start_timestamp)
        end_block_data = client.get_block_number_by_timestamp(search_end_timestamp)

        if not start_block_data or not end_block_data:
            return None

        # TODO(Ayelet): Cache logs to avoid repeated calls.
        logs = client.get_logs(start_block_data, end_block_data)

        for log in logs:
            l1_event = L1Events.decode_log(log)

            if L1Events.l1_event_matches_feeder_tx(l1_event, feeder_tx):
                logger.info(
                    f"Found matching L1 block: {l1_event.block_number} for L1 tx: {feeder_tx['transaction_hash']}"
                )
                return l1_event.block_number

        # Not found in this range
        logger.info(f"No matching L1 block found for L1 tx: {feeder_tx['transaction_hash']}")
        return None
