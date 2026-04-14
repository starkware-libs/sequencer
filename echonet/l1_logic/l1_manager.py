from dataclasses import dataclass
from typing import Callable, Optional

from echonet.constants import (
    STATE_BLOCK_HASH_SELECTOR,
    STATE_BLOCK_NUMBER_SELECTOR,
)
from echonet.helpers import format_hex, rpc_response
from echonet.l1_logic.l1_blocks import L1Blocks
from echonet.l1_logic.l1_client import L1Client
from echonet.logger import get_logger


class L1Manager:
    """
    Manages L1 block data indexed by block number and provides mock RPC responses.

    - get_block_number: returns the latest exposed block number + finality, or None if all blocks
      are still gated.
    - get_logs: returns L1_HANDLER logs for exposed blocks within the queried range. Blocks are
      exposed once echonet reaches source_block_number - 2.
    - get_block_by_number: returns block data and cleans up older blocks, or default block if not
      found.
    """

    L1_SCRAPER_FINALITY_CONFIG_VALUE = 10

    @dataclass(frozen=True)
    class L1TxData:
        block_number: int
        block_data: dict
        logs_result: list[dict]
        required_echonet_block: int

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
        self,
        l1_client: L1Client,
        get_last_proved_block_callback: Callable[[], tuple[int, int]],
        get_last_echonet_block_callback: Callable[[], Optional[int]],
    ):
        self.logger = get_logger("l1_manager")
        self.l1_client = l1_client
        self.blocks: dict[int, L1Manager.L1TxData] = {}
        self._gated_txs: list[L1Manager.L1TxData] = []
        self.get_last_proved_block_callback = get_last_proved_block_callback
        self.get_last_echonet_block_callback = get_last_echonet_block_callback

    def set_new_tx(
        self, feeder_gateway_tx: dict, l2_block_timestamp: int, source_block_number: int
    ) -> None:
        """
        Gets a feeder gateway transaction and its block timestamp,
        fetches the relevant L1 data, and stores it by block number.

        source_block_number is the mainnet L2 block where the L1_HANDLER tx appeared.
        The block is gated until echonet reaches source_block_number - 2, at which point it is
        exposed via get_block_number and get_logs.
        """
        required_echonet_block = source_block_number - 2

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

        # Guard against processing the same L1 block twice (e.g. two consecutive L1_HANDLER txs
        # from the same Ethereum block).
        if l1_block_number in self.blocks or any(
            tx.block_number == l1_block_number for tx in self._gated_txs
        ):
            self.logger.debug(f"Block {l1_block_number} already stored, skipping duplicate")
            return

        self._gated_txs.append(
            L1Manager.L1TxData(l1_block_number, l1_block_data, logs, required_echonet_block)
        )
        self.logger.debug(
            f"Gated L1 block {l1_block_number} for L2 tx {feeder_gateway_tx['transaction_hash']}"
            f" (required_echonet_block={required_echonet_block})"
        )

    def _promote_ready_gated_txs(self) -> None:
        """Move gated blocks whose echonet threshold has been reached into self.blocks."""
        last_echonet_block = self.get_last_echonet_block_callback()
        if not last_echonet_block:
            return
        still_gated = []
        for tx in self._gated_txs:
            if last_echonet_block >= tx.required_echonet_block:
                self.blocks[tx.block_number] = tx
                self.logger.debug(
                    f"Exposed L1 block {tx.block_number}"
                    f" (echonet={last_echonet_block} >= required={tx.required_echonet_block})"
                )
            else:
                still_gated.append(tx)
        self._gated_txs = still_gated

    def clear_stored_blocks(self) -> None:
        self.blocks.clear()
        self._gated_txs.clear()
        self.logger.debug("Cleared all stored L1 blocks")

    # Mock RPC responses.

    def get_logs(self, params: dict) -> dict:
        """Returns L1_HANDLER logs for exposed blocks within the queried range."""
        self._promote_ready_gated_txs()
        from_block = int(params["fromBlock"], 16)
        to_block = int(params["toBlock"], 16)

        logs: list[dict] = [
            log
            for block_num, tx_data in self.blocks.items()
            if from_block <= block_num <= to_block
            for log in tx_data.logs_result
        ]

        self.logger.debug(f"get_logs: range [{from_block}, {to_block}]: {len(logs)} logs")
        return rpc_response(logs)

    def get_block_by_number(self, block_number_hex: str) -> dict:
        """Returns block data for block_number, or default block if not found. Removes stored blocks that are much older than block_number."""
        self._promote_ready_gated_txs()
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
        """Returns the latest exposed block number + finality, or None if no blocks are exposed."""
        self._promote_ready_gated_txs()
        if not self.blocks:
            self.logger.debug("get_block_number: no blocks exposed, returning None")
            return rpc_response(None)

        latest_block_number = max(self.blocks.keys())
        finalized_block_number = latest_block_number + L1Manager.L1_SCRAPER_FINALITY_CONFIG_VALUE
        self.logger.debug(
            f"get_block_number: returning {finalized_block_number}"
            f" (latest={latest_block_number} + finality={L1Manager.L1_SCRAPER_FINALITY_CONFIG_VALUE})"
        )
        return rpc_response(hex(finalized_block_number))

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
