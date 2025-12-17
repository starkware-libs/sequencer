import json
from dataclasses import dataclass
from typing import Callable

import logging
from l1_blocks import L1Blocks
from l1_client import L1Client

from echonet.constants import (
    STATE_BLOCK_HASH_SELECTOR,
    STATE_BLOCK_NUMBER_SELECTOR,
)
from echonet.helpers import format_hex, rpc_response


class L1Manager:
    """
    Manages L1 block data indexed by block number and provides mock RPC responses.

    - get_block_number: returns the latest stored block number, or None if empty.
    - get_logs: returns logs for all stored blocks in the requested range, or empty logs list if empty.
    - get_block_by_number: returns block data and cleans up older blocks, or None if not found.
    """

    L1_SCRAPER_FINALITY_CONFIG_VALUE = 10

    @dataclass(frozen=True)
    class L1TxData:
        block_number: int
        block_data: dict
        logs_result: list[dict]

    def __init__(
        self, l1_client: L1Client, get_last_proved_block_callback: Callable[[], tuple[int, int]]
    ):
        self.logger = logging.getLogger("L1Manager")
        self.l1_client = l1_client
        self.blocks: dict[int, L1Manager.L1TxData] = {}
        self.get_last_proved_block_callback = get_last_proved_block_callback

    def set_new_tx(self, feeder_gateway_tx: dict, l2_block_timestamp: int) -> None:
        """
        Gets a feeder gateway transaction and its block timestamp,
        fetches the relevant L1 data, and stores it by block number.
        """
        l1_block_number = L1Blocks.find_l1_block_for_tx(
            feeder_gateway_tx, l2_block_timestamp, self.l1_client
        )
        if l1_block_number is None:
            return

        l1_block_data = self.l1_client.get_block_by_number(hex(l1_block_number))
        assert l1_block_data is not None, f"Block {l1_block_number} must exist"

        logs_response = self.l1_client.get_logs(l1_block_number, l1_block_number)
        assert logs_response, f"Logs must exist for block {l1_block_number}"

        logs = logs_response.get("result", [])
        self.blocks[l1_block_number] = L1Manager.L1TxData(l1_block_number, l1_block_data, logs)
        self.logger.debug(
            f"Stored L1 data for block {l1_block_number}, for L2 tx {feeder_gateway_tx['transaction_hash']}"
        )

    # Mock RPC responses.

    def get_logs(self, from_block: int, to_block: int) -> str:
        """Returns merged logs for stored blocks in [from_block, to_block], or empty logs list if empty."""
        logs = []
        for block_num in range(from_block, to_block + 1):
            block = self.blocks.get(block_num)
            if block:
                logs.extend(block.logs_result)

        self.logger.debug(f"get_logs({from_block}, {to_block}): returning {len(logs)} logs")
        return rpc_response(logs)

    def get_block_by_number(self, block_number_hex: str) -> str:
        """Returns block data for block_number, or None if not found. Removes all stored blocks < block_number."""
        block_number = int(block_number_hex, 16)
        # Cleanup older blocks
        blocks_to_remove = [bn for bn in self.blocks.keys() if bn < block_number]
        for bn in blocks_to_remove:
            del self.blocks[bn]

        if blocks_to_remove:
            self.logger.debug(f"get_block_by_number: cleaned up blocks {blocks_to_remove}")

        block_data = self.blocks.get(block_number)
        if block_data:
            self.logger.debug(f"get_block_by_number({block_number}): returning block data")
            return json.dumps(block_data.block_data)

        self.logger.debug(f"get_block_by_number({block_number}): block not found, returning None")
        return rpc_response(None)

    def get_block_number(self) -> str:
        """Returns the latest stored block number, or None if empty."""
        if not self.blocks:
            self.logger.debug("get_block_number: no blocks stored, returning None")
            return rpc_response(None)

        latest_block_number = max(self.blocks.keys())
        finalized_block_number = latest_block_number + L1Manager.L1_SCRAPER_FINALITY_CONFIG_VALUE
        self.logger.debug(
            f"get_block_number: returning finalized_block_number={finalized_block_number} "
            f"(latest_block_number={latest_block_number} + finality={L1Manager.L1_SCRAPER_FINALITY_CONFIG_VALUE})"
        )
        return rpc_response(hex(finalized_block_number))

    def get_call(self, params: dict) -> str:
        """
        Handles eth_call for stateBlockNumber/stateBlockHash based on function selector.
        """
        input_data = params.get("input", "")
        last_block_number, last_block_hash = self.get_last_proved_block_callback()

        if input_data.startswith(STATE_BLOCK_NUMBER_SELECTOR):
            result = format_hex(last_block_number)
        elif input_data.startswith(STATE_BLOCK_HASH_SELECTOR):
            result = format_hex(last_block_hash)
        else:
            result = "0x"

        return json.dumps({"jsonrpc": "2.0", "id": "1", "result": result})
