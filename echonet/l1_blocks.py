from typing import Optional

from l1_client import L1Client
from l1_utils import timestamp_to_iso
from logger import get_logger

# Use the shared echonet logger namespace so L1 logs are visible with others.
logger = get_logger("l1_blocks")


class L1Blocks:
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
        logger.info(
            f"Finding L1 block for tx: {feeder_tx}, l2_block_timestamp: {l2_block_timestamp}"
        )
        if "transaction_hash" not in feeder_tx:
            logger.error("Feeder tx does not contain transaction_hash.")
            return None

        search_start_timestamp = l2_block_timestamp - (search_minutes_before * 60)
        search_end_timestamp = l2_block_timestamp

        start_block_data = client.get_block_number_by_timestamp(search_start_timestamp)
        end_block_data = client.get_block_number_by_timestamp(search_end_timestamp)

        if not start_block_data or not end_block_data:
            return None

        logs_response = client.get_logs(start_block_data, end_block_data)
        if logs_response is None:
            logger.error(f"No logs found for block {start_block_data} to {end_block_data}")
            return None

        results = logs_response.get("result", [])
        logger.info(f"Found {len(results)} logs")
        for log in results:
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
