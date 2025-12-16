from dataclasses import dataclass
from typing import Optional

import logging
from l1_client import L1Client

from echonet.constants import ETHEREUM_AVERAGE_SECONDS_PER_BLOCK
from echonet.helpers import timestamp_to_iso

logger = logging.getLogger(__name__)


class L1Blocks:
    _MAX_BLOCK_SEARCH_ITERATIONS = 10
    _MAX_ALLOWED_TIMESTAMP_DIFF_SECONDS = 20

    @dataclass(frozen=True)
    class BlockInfo:
        number: int
        timestamp: int

    @staticmethod
    def _get_latest_block_info(client: L1Client) -> Optional["L1Blocks.BlockInfo"]:
        block_number_response = client.get_block_number()
        if not block_number_response or not block_number_response.get("result"):
            logger.error("Failed to get latest L1 block number")
            return None

        block_number = int(block_number_response["result"], 16)

        timestamp = client.get_timestamp_of_block(hex(block_number))
        if timestamp is None:
            logger.error(f"Failed to get timestamp for block {block_number}")
            return None

        return L1Blocks.BlockInfo(number=block_number, timestamp=timestamp)

    @staticmethod
    def _find_block_near_timestamp(
        client: L1Client,
        target_timestamp: int,
        reference_block: "L1Blocks.BlockInfo",
    ) -> Optional[int]:
        time_diff = reference_block.timestamp - target_timestamp
        estimated_block = reference_block.number - (time_diff // ETHEREUM_AVERAGE_SECONDS_PER_BLOCK)
        estimated_block = max(0, estimated_block)

        for iteration in range(L1Blocks._MAX_BLOCK_SEARCH_ITERATIONS):
            block_timestamp = client.get_timestamp_of_block(hex(estimated_block))
            if block_timestamp is None:
                logger.error(f"Failed to get timestamp for block {estimated_block}")
                return None

            timestamp_diff = block_timestamp - target_timestamp

            if abs(timestamp_diff) <= L1Blocks._MAX_ALLOWED_TIMESTAMP_DIFF_SECONDS:
                logger.debug(
                    f"Found block {estimated_block} (diff: {timestamp_diff}s) after {iteration + 1} iterations"
                )
                return estimated_block

            # Adjust: positive diff = block too new (go back), negative diff = block too old (go forward).
            block_adjustment = timestamp_diff // ETHEREUM_AVERAGE_SECONDS_PER_BLOCK
            estimated_block -= block_adjustment
            estimated_block = max(0, estimated_block)

            logger.debug(
                f"Iteration {iteration + 1}: diff={timestamp_diff}s, moved to block {estimated_block}"
            )

        logger.error(f"Block search reached max iterations for timestamp {target_timestamp}")
        return None

    @staticmethod
    def l1_event_matches_feeder_tx(l1_event: L1Client.L1Event, feeder_tx: dict) -> bool:
        """
        Compares L1Event with an L1_HANDLER feeder tx using only contract_address, entry_point_selector, nonce, and calldata.
        Transaction hashes are ignored.
        """
        if feeder_tx.get("type") != "L1_HANDLER":
            return False

        feeder_contract = hex(int(feeder_tx["contract_address"], 16))
        if l1_event.contract_address != feeder_contract:
            return False

        feeder_selector = int(feeder_tx["entry_point_selector"], 16)
        if l1_event.entry_point_selector != feeder_selector:
            return False

        feeder_nonce = int(feeder_tx["nonce"], 16)
        if l1_event.nonce != feeder_nonce:
            return False

        feeder_calldata = [int(item, 16) for item in feeder_tx["calldata"]]
        if l1_event.calldata != feeder_calldata:
            return False

        return True

    @staticmethod
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

        reference_block = L1Blocks._get_latest_block_info(client)
        if reference_block is None:
            return None

        search_start_timestamp = l2_block_timestamp - (search_minutes_before * 60)
        search_end_timestamp = l2_block_timestamp

        start_block = L1Blocks._find_block_near_timestamp(
            client, search_start_timestamp, reference_block
        )
        end_block = L1Blocks._find_block_near_timestamp(
            client, search_end_timestamp, reference_block
        )

        if not start_block or not end_block:
            return None

        logs_response = client.get_logs(start_block, end_block)
        if logs_response is None:
            return None

        for log in logs_response.get("result", []):
            l1_event = L1Client.decode_log_response(log)

            if L1Blocks.l1_event_matches_feeder_tx(l1_event, feeder_tx):
                logger.info(
                    f"Found matching L1 tx {l1_event.l1_tx_hash}, in block: {l1_event.block_number} for L2 tx: {feeder_tx['transaction_hash']}"
                )
                return l1_event.block_number

        # Not found in this range
        logger.info(
            f"No matching L1 block found for L2 tx: {feeder_tx['transaction_hash']} "
            f"in the range {timestamp_to_iso(search_start_timestamp)} to {timestamp_to_iso(search_end_timestamp)}"
        )
        return None
