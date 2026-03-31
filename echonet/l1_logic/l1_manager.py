import threading
from dataclasses import dataclass
from typing import Callable, Optional

from echonet.constants import (
    DEFAULT_L1_BLOCK_NUMBER,
    STATE_BLOCK_HASH_SELECTOR,
    STATE_BLOCK_NUMBER_SELECTOR,
)
from echonet.helpers import format_hex, rpc_response
from echonet.l1_logic.l1_blocks import L1Blocks
from echonet.l1_logic.l1_client import L1Client
from echonet.l1_logic.l1_gas_price import L1GasPrice
from echonet.logger import get_logger


class L1Manager:
    """
    Manages L1 block data indexed by block number and provides mock RPC responses.

    - get_block_number: returns _mock_l1_head_number (eth_blockNumber; always increasing).
    - get_logs: returns logs for all stored blocks in the requested range, or empty logs list if empty.
    - get_block_by_number: stored L1 tx data or default block.

    The mock L1 head number is the source of truth for the gas price scraper.
    It is incremented by set_gas_price_target (once per L2 block) and synced upward by set_new_tx
    when real L1 block numbers are stored.
    """

    L1_SCRAPER_FINALITY_CONFIG_VALUE = 10

    @dataclass(frozen=True)
    class L1TxData:
        block_number: int
        block_data: dict
        logs_result: list[dict]

    def default_l1_block(self, block_number_hex: str) -> dict:
        block: dict = {
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
            # set_gas_price_target is called with the NEXT feeder block's timestamp
            # (T_next ≈ T_current + block_interval). The mock L1 timestamp must be ≤ T_current
            # so the Rust lag-margin lookup (lag=0) finds a qualifying block. 30s is safely
            # above the ~2s block interval and well below the 900s max_time_gap.
            "timestamp": hex(max(0, self._l2_block_timestamp - 30))
            if self._l2_block_timestamp
            else "0x0",
            "extraData": "0x",
            "mixHash": format_hex(0),
            "nonce": format_hex(0, 16),
            "size": "0x0",
            "transactions": [],
            "uncles": [],
        }
        if self._gas_price_target:
            base_fee_wei, blob_fee_wei = self._gas_price_target
            block["baseFeePerGas"] = hex(base_fee_wei)
            block["excessBlobGas"] = hex(L1GasPrice.excess_blob_gas_for_fee(blob_fee_wei))
            block["blobGasUsed"] = "0x0"
        return block

    def __init__(
        self,
        l1_client: L1Client,
        get_last_proved_block_callback: Callable[[], tuple[int, int]],
    ):
        self.logger = get_logger("l1_manager")
        self.l1_client = l1_client
        self.blocks: dict[int, L1Manager.L1TxData] = {}
        self.get_last_proved_block_callback = get_last_proved_block_callback
        self._gas_price_target: Optional[tuple[int, int]] = None  # (base_fee_wei, blob_fee_wei)
        # Mock L1 head (eth_blockNumber) when no real L1 blocks are stored.
        # Incremented each time set_gas_price_target is called so the L1 gas price scraper
        # sees a new block number and re-fetches, picking up the updated prices.
        self._mock_l1_head_number: int = DEFAULT_L1_BLOCK_NUMBER
        # L2 block timestamp corresponding to the current gas price target.
        self._l2_block_timestamp: int = 0
        # Condition used to implement long-polling in get_block_number.
        # Notified whenever _mock_l1_head_number changes so waiting scrapers wake up
        # immediately instead of hammering echonet with rapid polls.
        self._mock_l1_head_updated = threading.Condition()

    def _set_mock_l1_head_number(self, value: int) -> None:
        with self._mock_l1_head_updated:
            self._mock_l1_head_number = value
            self._mock_l1_head_updated.notify_all()

    def set_gas_price_target(
        self, base_fee_wei: int, blob_fee_wei: int, l2_timestamp: int = 0
    ) -> None:
        """Set the gas prices returned by default_l1_block for the current sequencing target."""
        self._gas_price_target = (base_fee_wei, blob_fee_wei)
        self._l2_block_timestamp = l2_timestamp
        self._set_mock_l1_head_number(self._mock_l1_head_number + 1)
        self.logger.info(
            f"Gas price target updated: base_fee_wei={base_fee_wei}, blob_fee_wei={blob_fee_wei}, "
            f"l2_timestamp={l2_timestamp}, mock_l1_head_number={self._mock_l1_head_number}"
        )

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

        l1_block_data = self.l1_client.get_block_by_number(l1_block_number)
        assert l1_block_data is not None, f"Block {l1_block_number} must exist"

        logs_response = self.l1_client.get_logs(l1_block_number, l1_block_number)
        assert logs_response, f"Logs must exist for block {l1_block_number}"

        logs = logs_response.get("result", [])
        self.blocks[l1_block_number] = L1Manager.L1TxData(l1_block_number, l1_block_data, logs)

        # Sync mock L1 head upward to cover the real L1 block so the gas price scraper advances.
        synced_mock_l1_head = l1_block_number + L1Manager.L1_SCRAPER_FINALITY_CONFIG_VALUE
        if synced_mock_l1_head > self._mock_l1_head_number:
            self._set_mock_l1_head_number(synced_mock_l1_head)

        self.logger.debug(
            f"Stored L1 data for block {l1_block_number}, for L2 tx {feeder_gateway_tx['transaction_hash']}"
        )

    def clear_stored_blocks(self) -> None:
        self.blocks.clear()
        self._set_mock_l1_head_number(DEFAULT_L1_BLOCK_NUMBER)
        self.logger.debug("Cleared all stored L1 blocks and reset mock L1 head number")

    # Mock RPC responses.

    def get_logs(self, params: dict) -> dict:
        """Returns merged logs for stored blocks in [fromBlock, toBlock], or empty logs list if empty."""
        from_block = int(params["fromBlock"], 16)
        to_block = int(params["toBlock"], 16)

        logs = []
        for block_num in range(from_block, to_block + 1):
            block = self.blocks.get(block_num)
            if block:
                logs.extend(block.logs_result)

        self.logger.debug(
            f"get_logs: range [{from_block}, {to_block}] ({to_block - from_block + 1} blocks): {len(logs)} logs"
        )
        return rpc_response(logs)

    def get_block_by_number(self, block_number_hex: str) -> dict:
        """Returns block data for block_number, or default block if not found. Removes stored blocks that are much older than block_number."""
        block_number = int(block_number_hex, 16)
        # Cleanup older blocks, but keep a buffer to avoid deleting blocks that haven't been scraped yet.
        CLEANUP_BUFFER = L1Manager.L1_SCRAPER_FINALITY_CONFIG_VALUE * 2
        blocks_to_remove = [bn for bn in self.blocks.keys() if bn < block_number - CLEANUP_BUFFER]
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
        """Return mock L1 head (eth_blockNumber), blocking until it changes or timeout elapses.

        Long-polling: the scraper (polling_interval=0) calls this in a tight loop. By waiting
        here instead of returning immediately, we avoid hammering echonet with hundreds of
        requests per second while still waking up within milliseconds of a new block.
        notify_all() is used so both the gas price scraper and the events scraper unblock.
        """
        with self._mock_l1_head_updated:
            self._mock_l1_head_updated.wait(timeout=0.5)
            head = self._mock_l1_head_number
        self.logger.debug(f"get_block_number: returning mock_l1_head_number={head}")
        return rpc_response(hex(head))

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
