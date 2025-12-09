import json
from dataclasses import dataclass
from typing import Any, Optional

import logging
from collections import deque
from l1_blocks import L1Blocks
from l1_client import L1Client


class L1Manager:
    """
    Manages a queue of L1 transaction data and provides mock RPC responses.
    Advances to next tx after 4 calls (as in the node's flow).

    Node flow per transaction (see fn fetch_events, l1_scraper.rs):
    1. latest_l1_block_number() → get_block_number
    2. l1_block_at() → get_block_by_number
    3. events() → get_logs
    4. events() → get_block_header() → get_block_by_number
    """

    @dataclass(frozen=True)
    class L1TxData:
        block_number: int
        block_data: dict
        logs: dict

    def __init__(self, l1_client: L1Client):
        self.logger = logging.getLogger("L1Manager")
        self.l1_client = l1_client
        self.queue: deque[L1Manager.L1TxData] = deque()
        self._call_count = 0

    def _current(self) -> Optional[L1TxData]:
        self._advance()
        return self.queue[0] if self.queue else None

    def _advance(self) -> None:
        """Advance to next transaction after 4 calls (see node's flow)."""
        self._call_count += 1
        self.logger.debug(f"Call count: {self._call_count}")
        if self._call_count >= 4 and self.queue:
            # self.queue.popleft()
            self._call_count = 0
            self.logger.debug("Advanced to next tx in queue")

    def _rpc_response(self, result: Any) -> str:
        return json.dumps({"jsonrpc": "2.0", "id": "1", "result": result})

    def set_new_tx(self, feeder_gateway_tx: dict, l2_block_timestamp: int) -> None:
        """
        Gets a feeder gateway transaction and its block timestamp,
        fetches the relevant L1 data, and queues it.
        """
        l1_block_number = L1Blocks.find_l1_block_for_tx(
            feeder_gateway_tx, l2_block_timestamp, self.l1_client
        )
        if l1_block_number is None:
            return

        l1_block_data = self.l1_client.get_block_by_number(hex(l1_block_number))
        assert l1_block_data is not None, f"Block {l1_block_number} must exist"

        logs = self.l1_client.get_logs(l1_block_number, l1_block_number)
        assert logs, f"Logs must exist for block {l1_block_number}"

        self.queue.append(L1Manager.L1TxData(l1_block_number, l1_block_data, logs))
        self.logger.debug(
            f"Queued L1 data of block {l1_block_number}, for L2 tx {feeder_gateway_tx['transaction_hash']}"
        )

    # Mock RPC responses - return raw L1 data from queue or defaults for empty queue.

    def get_logs(self, _filter: Any) -> str:
        item = self._current()
        self.logger.debug(f"get_logs called, returning logs for block {item.block_number}" if item else "get_logs called, but queue is empty")
        return json.dumps(item.logs) if item else self._rpc_response([])

    def get_block_by_number(self, _block_number: str) -> str:
        item = self._current()
        self.logger.debug(f"get_block_by_number called, returning block data for block {item.block_number}" if item else "get_block_by_number called, but queue is empty")
        return json.dumps(item.block_data) if item else self._rpc_response(None)

    def get_block_number(self) -> str:
        item = self._current()
        self.logger.debug(f"get_block_number called, returning block number {item.block_number}" if item else "get_block_number called, but queue is empty")
        result = hex(item.block_number) if item else None
        return self._rpc_response(result)
