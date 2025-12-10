from dataclasses import dataclass
from typing import Any, Dict, Optional

from collections import deque
from l1_blocks import L1Blocks
from l1_client import L1Client
from logger import get_logger


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
        # Use the common echonet logger namespace so messages are visible
        # alongside other components like transaction_sender.
        self.logger = get_logger("l1_manager")
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

    def _rpc_response(self, result: Any) -> Dict[str, Any]:
        """
        Build a JSON-RPC 2.0 response object.

        Returning a dict (rather than a pre-serialized string) allows the
        Flask layer to serialize exactly once, avoiding double-encoding like:
        "\"{...jsonrpc response...}\"".
        """
        return {"jsonrpc": "2.0", "id": "1", "result": result}

    def set_new_tx(self, feeder_gateway_tx: dict, l2_block_timestamp: int) -> None:
        """
        Gets a feeder gateway transaction and its block timestamp,
        fetches the relevant L1 data, and queues it.
        """
        self.logger.info(
            f"Got new tx: {feeder_gateway_tx}, l2_block_timestamp: {l2_block_timestamp}"
        )
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

    def get_logs(self, _filter: Any) -> Dict[str, Any]:
        item = self._current()
        self.logger.debug(
            f"get_logs called, returning logs for block {item.block_number}"
            if item
            else "get_logs called, but queue is empty"
        )
        # When we have queued data, return the raw JSON-RPC response object
        # obtained from L1Client; otherwise, synthesize an empty JSON-RPC result.
        return item.logs if item else self._rpc_response([])

    def get_block_by_number(self, _block_number: str) -> Dict[str, Any]:
        item = self._current()
        self.logger.debug(
            f"get_block_by_number called, returning block data for block {item.block_number}"
            if item
            else "get_block_by_number called, but queue is empty"
        )
        # As with logs, queued block_data is already a JSON-RPC response object
        # from L1Client. When the queue is empty, synthesize a minimal-but-valid
        # block object instead of a JSON-RPC null result so that consumers
        # expecting a `BlockResponse`-like structure (see alloy `BlockResponse`
        # and `RpcRecv` traits) still receive a deserializable value.
        if item and item.block_number == int(_block_number, 16):
            return item.block_data

        default_block = {
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "hash": "0xf62064a4320f5efa7d6df0752f5c69f820532250999d3db03f31003f4034f3d2",
                "parentHash": "0xd8c2b897792b19d0580cb91e8091293ba617087bcd18798b554f866afb086170",
                "sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
                "miner": "0x396343362be2a4da1ce0c1c210945346fb82aa49",
                "stateRoot": "0xd451e50c9ca35cd06d2b7e18daf33a7ad35996f1f3a183b83fcad4416bf99399",
                "transactionsRoot": "0xc4f6adc8e52568aec7a94500394f89336bc10075253177351c201bfdd3e11eeb",
                "receiptsRoot": "0x4f65bbc6b66dd2b71461476cccf6f29ffe871a2ed611c30edceb65fe2e749230",
                "logsBloom": "0x6d21116040240822b20229a18f09bc151747138d64535034058c0af21d10e16204dedf19f25c1a081e777bb36a271bc70af68602be923f033395464de03401b6e4277b4dc8d19bddad8a71289cc877ff5055797917c60b280c778d84eb2b8d2fd7258b049b7e43b0ea34c1332c20f90d060f8933463e15f6b3c4e69e7a3bd26d586dab3811800c2c6219c0ec8bd95fa73c59c257ed70796fc82caeda09b07dabcf72b5fba8352858e75059c8b884a6c4318090dd27d458090dd7434e368585e306ee9f8a240b5b5100fb33f7976a3888c14c7ad2975a6f54640c48eb21b4e81ac3b5eca3cfee40838fe6eeb3a201caf58d66490541f92cc49e21b190675d12e0",
                "difficulty": "0x0",
                "number": _block_number,
                "gasLimit": "0x3938700",
                "gasUsed": "0x2abf50b",
                "timestamp": "0x6936c59f",
                "extraData": "0xe29ca82051756173617220287175617361722e77696e2920e29ca8",
                "mixHash": "0x730565cb80632ba9ad96df859af247dafeaf1d9d35464b002efcc2fe4e9308c2",
                "nonce": "0x0000000000000000",
                "baseFeePerGas": "0x12ee8ddf",
                "withdrawalsRoot": "0xf34ff23c6174cf4ec8eb1aed02d7a2825deb5f4f510c5918ae2f1649a4fb5115",
                "blobGasUsed": "0x60000",
                "excessBlobGas": "0x50aa1ae",
                "parentBeaconBlockRoot": "0x0485cf532e13b73485f2f7e33a551c6d20e0cbf5b6e9a16d72fa32f8c7dea9bf",
                "requestsHash": "0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                "size": "0x10ee6",
                "uncles": [],
                "transactions": [],
                "withdrawals": [],
            },
        }
        return default_block

    def get_block_number(self) -> Dict[str, Any]:
        item = self._current()
        self.logger.debug(
            f"get_block_number called, returning block number {item.block_number}"
            if item
            else "get_block_number called, but queue is empty"
        )
        if item:
            result = hex(item.block_number + 10)
            return self._rpc_response(result)

        raise RuntimeError("L1Manager.get_block_number called but queue is empty")
