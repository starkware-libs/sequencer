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
    - get_block_by_number: returns block data and cleans up older blocks, or default block if not found.
    """

    @dataclass(frozen=True)
    class L1TxData:
        block_number: int
        block_data: dict
        logs_result: list[dict]

    @staticmethod
    def default_l1_block(block_number_hex: str) -> dict:
        return {
            "number": block_number_hex,
            "hash": format_hex(0),
            "parentHash": format_hex(0),
            "sha3Uncles": format_hex(0),
            "miner": format_hex(0, 40),
            "stateRoot": format_hex(0),
            "transactionsRoot": format_hex(0),
            "receiptsRoot": format_hex(0),
            "logsBloom": format_hex(0, 512),
            "difficulty": "0x0",
            "gasLimit": "0x0",
            "gasUsed": "0x0",
            "timestamp": "0x0",
            "extraData": "0x",
            "mixHash": format_hex(0),
            "nonce": format_hex(0, 16),
            "size": "0x0",
            "transactions": [],
            "uncles": [],
        }

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

    def get_logs(self, from_block: int, to_block: int) -> dict:
        """Returns merged logs for stored blocks in [from_block, to_block], or empty logs list if empty."""
        logs = []
        for block_num in range(from_block, to_block + 1):
            block = self.blocks.get(block_num)
            if block:
                logs.extend(block.logs_result)

        self.logger.debug(f"get_logs({from_block}, {to_block}): returning {len(logs)} logs")
        return rpc_response(logs)

    def get_block_by_number(self, block_number_hex: str) -> dict:
        """Returns block data for block_number, or default block if not found. Removes all stored blocks < block_number."""
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
            return block_data.block_data

        # Returns default values when the block is not found.
        # During initialization, blocks from ~1 hour ago are fetched (startup_rewind_time_seconds).
        self.logger.debug(
            f"get_block_by_number({block_number}): block not found, returning default block"
        )
        return rpc_response(self.default_l1_block(block_number_hex))

    def get_block_number(self) -> dict:
        """Returns the latest stored block number, or None if empty."""
        if not self.blocks:
            self.logger.debug("get_block_number: no blocks stored, returning None")
            return rpc_response(None)

        latest = max(self.blocks.keys())
        self.logger.debug(f"get_block_number: returning {latest}")
        return rpc_response(hex(latest))

    def get_call(self, params: dict) -> dict:
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

        return rpc_response(result)
