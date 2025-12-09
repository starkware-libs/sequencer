import json
import os
import sys

import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from l1_client import L1Client
from l1_manager import L1Manager
from test_utils import L1TestUtils


class TestL1Manager(unittest.TestCase):
    def setUp(self):
        self.mock_client = Mock(spec=L1Client)
        self.manager = L1Manager(self.mock_client)

    def test_empty_queue_returns_defaults(self):
        block_number = json.loads(self.manager.get_block_number())
        self.assertEqual(block_number, {"jsonrpc": "2.0", "id": "1", "result": None})

        block = json.loads(self.manager.get_block_by_number("0x1"))
        self.assertEqual(block, {"jsonrpc": "2.0", "id": "1", "result": None})

        logs = json.loads(self.manager.get_logs({}))
        self.assertEqual(logs, {"jsonrpc": "2.0", "id": "1", "result": []})

    @patch("l1_manager.L1Blocks")
    def test_returns_queued_data(self, mock_l1_blocks):
        self.mock_client.get_logs.return_value = L1TestUtils.LOGS_RPC_RESPONSE
        self.mock_client.get_block_by_number.return_value = L1TestUtils.BLOCK_RPC_RESPONSE
        mock_l1_blocks.find_l1_block_for_tx.return_value = L1TestUtils.BLOCK_NUMBER

        self.manager.set_new_tx(L1TestUtils.FEEDER_TX, L1TestUtils.L2_BLOCK_TIMESTAMP)

        block_number = json.loads(self.manager.get_block_number())
        self.assertEqual(block_number, L1TestUtils.BLOCK_NUMBER_RPC_RESPONSE)

        block = json.loads(self.manager.get_block_by_number("0x1"))
        self.assertEqual(block, L1TestUtils.BLOCK_RPC_RESPONSE)

        logs = json.loads(self.manager.get_logs({}))
        self.assertEqual(logs, L1TestUtils.LOGS_RPC_RESPONSE)

    @patch("l1_manager.L1Blocks")
    def test_two_txs_fifo_advances_after_4_calls(self, mock_l1_blocks):
        # Setup L1 data for two transactions.
        first_block_number = L1TestUtils.BLOCK_NUMBER
        second_block_number = first_block_number + 100
        second_block_timestamp = L1TestUtils.L2_BLOCK_TIMESTAMP + 1
        second_block_rpc = L1TestUtils.block_rpc_response_with_block(
            {"number": hex(second_block_number), "timestamp": hex(second_block_timestamp)}
        )
        second_logs_rpc = L1TestUtils.logs_rpc_response_with_logs(
            [L1TestUtils.log_with_nonce(L1TestUtils.NONCE + 1)]
        )
        feeder_tx_2 = L1TestUtils.FEEDER_TX.copy()
        feeder_tx_2["nonce"] = hex(L1TestUtils.NONCE + 1)
        l2_block_timestamp_2 = L1TestUtils.L2_BLOCK_TIMESTAMP + 1

        # Setup mocks.
        mock_l1_blocks.find_l1_block_for_tx.side_effect = [first_block_number, second_block_number]
        self.mock_client.get_logs.return_value = [L1TestUtils.LOGS_RPC_RESPONSE, second_logs_rpc]
        self.mock_client.get_block_by_number.return_value = [
            L1TestUtils.BLOCK_RPC_RESPONSE,
            second_block_rpc,
        ]

        # Queue two transactions.
        self.manager.set_new_tx(L1TestUtils.FEEDER_TX, L1TestUtils.L2_BLOCK_TIMESTAMP)

        self.manager.set_new_tx(feeder_tx_2, l2_block_timestamp_2)

        # First tx: get_block_number() returns first block number, then complete flow (2–4) for first tx
        response = json.loads(self.manager.get_block_number())
        self.assertEqual(response, L1TestUtils.BLOCK_NUMBER_RPC_RESPONSE)
        self.manager.get_block_by_number("0x1")
        self.manager.get_logs({})
        self.manager.get_block_by_number("0x1")

        # Second tx: get_block_number() returns second block number, then complete flow (2–4) for second tx
        response = json.loads(self.manager.get_block_number())
        self.assertEqual(response["result"], hex(second_block_number))
        self.manager.get_block_by_number("0x1")
        self.manager.get_logs({})
        self.manager.get_block_by_number("0x1")

        # Queue empty: get_block_number() returns default
        response = json.loads(self.manager.get_block_number())
        self.assertIsNone(response["result"])


if __name__ == "__main__":
    unittest.main()
