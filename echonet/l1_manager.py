"""
Provides mock responses for L1 RPC calls.
"""

import json
import logging
from collections import deque
from dataclasses import dataclass
from typing import Any

from l1_blocks import L1Blocks
from l1_client import L1Client


class L1Manager:
    @dataclass(frozen=True)
    class L1TxData:
        block_number: int
        block_data: dict
        logs: list[dict]

    def __init__(self, l1_client: L1Client):
        self.logger = logging.getLogger("L1Manager")
        self.l1_client = l1_client
        self.queue: deque[L1Manager.L1TxData] = deque()

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

        l1_block_data = self.l1_client.get_block_by_number(l1_block_number)
        assert l1_block_data is not None, f"Block {l1_block_number} must exist"

        logs = self.l1_client.get_logs(l1_block_number, l1_block_number)
        assert logs, f"Logs must exist for block {l1_block_number}"

        self.queue.append(L1Manager.L1TxData(l1_block_number, l1_block_data, logs))
        self.logger.debug(
            f"Queued L1 data of block {l1_block_number}, for L2 tx {feeder_gateway_tx['transaction_hash']}"
        )

    def get_logs(self, filter: Any) -> str:
        """
        Returns raw logs response from Alchemy.
        """
        return json.dumps(
            {
                "jsonrpc": "2.0",
                "id": "1",
                "result": [
                    {
                        "address": "0xc662c410c0ecf747543f5ba90660f6abebd9c8c4",
                        "topics": [
                            "0xdb80dd488acf86d17c747445b0eabb5d57c541d3bd7b6b87af987858e5066b2b",
                            "0x000000000000000000000000f5b6ee2caeb6769659f6c091d209dfdcaf3f69eb",
                            "0x0616757a151c21f9be8775098d591c2807316d992bbc3bb1a5c1821630589256",
                            "0x01b64b1b3b690b43b9b514fb81377518f4039cd3e4f4914d8a6bdf01d679fb19",
                        ],
                        "data": "0x0000000000000000000000000000000000000000000000000000000000000060"
                        "000000000000000000000000000000000000000000000000000000000019b255"
                        "00000000000000000000000000000000000000000000000000001308aba4ade2"
                        "0000000000000000000000000000000000000000000000000000000000000005"
                        "00000000000000000000000004c46e830bb56ce22735d5d8fc9cb90309317d0f"
                        "000000000000000000000000c50a951c4426760ba75c5253985a16196b342168"
                        "011bf9dbebdd770c31ff13808c96a1cb2de15a240274dc527e7d809bb2bf38df"
                        "0000000000000000000000000000000000000000000000956dfdeac59085edc3"
                        "0000000000000000000000000000000000000000000000000000000000000000",
                        "blockHash": "0xb33512d13e1a2ff4f3aa6e799a4a2455249be5198760a3f41300a8362d802bf8",
                        "blockNumber": "0x16cda82",
                        "blockTimestamp": "0x692c23df",
                        "transactionHash": "0x726df509fdd23a944f923a6fc18e80cbe7300a54aa34f8e6bd77e9961ca6ce52",
                        "transactionIndex": "0x4f",
                        "logIndex": "0x7b",
                        "removed": False,
                    }
                ],
            }
        )

    def get_block_by_number(self, block_number: str) -> str:
        """
        Returns raw block response from Alchemy.
        """
        return json.dumps(
            {
                "jsonrpc": "2.0",
                "id": "1",
                "result": {
                    "number": "0x68b3",
                    "hash": "0xd5f1812548be429cbdc6376b29611fc49e06f1359758c4ceaaa3b393e2239f9c",
                    "mixHash": "0x24900fb3da77674a861c428429dce0762707ecb6052325bbd9b3c64e74b5af9d",
                    "parentHash": "0x1f68ac259155e2f38211ddad0f0a15394d55417b185a93923e2abe71bb7a4d6d",
                    "nonce": "0x378da40ff335b070",
                    "sha3Uncles": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
                    "logsBloom": "0x00000000000000100000004080000000000500000000000000020000100000000800001000000004000001000000000000000800040010000020100000000400000010000000000000000040000000000000040000000000000000000000000000000400002400000000000000000000000000000004000004000000000000840000000800000080010004000000001000000800000000000000000000000000000000000800000000000040000000020000000000000000000800000400000000000000000000000600000400000000002000000000000000000000004000000000000000100000000000000000000000000000000000040000900010000000",
                    "transactionsRoot": "0x4d0c8e91e16bdff538c03211c5c73632ed054d00a7e210c0eb25146c20048126",
                    "stateRoot": "0x91309efa7e42c1f137f31fe9edbe88ae087e6620d0d59031324da3e2f4f93233",
                    "receiptsRoot": "0x68461ab700003503a305083630a8fb8d14927238f0bc8b6b3d246c0c64f21f4a",
                    "miner": "0xb42b6c4a95406c78ff892d270ad20b22642e102d",
                    "difficulty": "0x66e619a",
                    "totalDifficulty": "0x1e875d746ae",
                    "extraData": "0xd583010502846765746885676f312e37856c696e7578",
                    "size": "0x334",
                    "gasLimit": "0x47e7c4",
                    "gasUsed": "0x37993",
                    "timestamp": "0x5835c54d",
                    "uncles": [],
                    "transactions": [
                        "0xa0807e117a8dd124ab949f460f08c36c72b710188f01609595223b325e58e0fc",
                        "0xeae6d797af50cb62a596ec3939114d63967c374fa57de9bc0f4e2b576ed6639d",
                    ],
                    "baseFeePerGas": "0x7",
                    "withdrawalsRoot": "0x7a4ecf19774d15cf9c15adf0dd8e8a250c128b26c9e2ab2a08d6c9c8ffbd104f",
                    "withdrawals": [
                        {
                            "index": "0x0",
                            "validatorIndex": "0x9d8c0",
                            "address": "0xb9d7934878b5fb9610b3fe8a5e441e8fad7e293f",
                            "amount": "0x11a33e3760",
                        }
                    ],
                    "blobGasUsed": "0x0",
                    "excessBlobGas": "0x0",
                    "parentBeaconBlockRoot": "0x95c4dbd5b19f6fe3cbc3183be85ff4e85ebe75c5b4fc911f1c91e5b7a554a685",
                },
            }
        )

    def get_block_number(self) -> str:
        """
        Returns raw block number.
        """
        return json.dumps(
            {
                "jsonrpc": "2.0",
                "id": "1",
                "result": "0x2377",
            }
        )
