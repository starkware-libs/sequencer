import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../../.."))

import unittest
from l1_client import L1Client
from l1_manager import L1Manager
from test_utils import L1TestUtils
from unittest.mock import Mock, patch

from echonet.helpers import format_hex


class TestL1Manager(unittest.TestCase):
    def setUp(self):
        self.mock_client = Mock(spec=L1Client)
        # get_last_proved_block callback not used in these tests (only needed for get_call).
        self.manager = L1Manager(
            l1_client=self.mock_client, get_last_proved_block_callback=lambda: (0, 0)
        )

    def test_empty_queue_returns_defaults(self):
        block_number = self.manager.get_block_number()
        self.assertEqual(block_number, {"jsonrpc": "2.0", "id": "1", "result": None})

        block_number_hex = hex(1)
        block = self.manager.get_block_by_number(block_number_hex)
        expected_default_block = {
            "number": block_number_hex,
            "hash": format_hex(0),
            "timestamp": "0x0",
        }
        self.assertEqual(block, {"jsonrpc": "2.0", "id": "1", "result": expected_default_block})

        logs = self.manager.get_logs(0, 100)
        self.assertEqual(logs, {"jsonrpc": "2.0", "id": "1", "result": []})

    @patch("l1_manager.L1Blocks.find_l1_block_for_tx")
    def test_single_block(self, mock_find_l1_block_for_tx):
        # Setup.
        mock_find_l1_block_for_tx.return_value = L1TestUtils.BLOCK_NUMBER
        self.mock_client.get_block_by_number.return_value = L1TestUtils.BLOCK_RPC_RESPONSE
        self.mock_client.get_logs.return_value = L1TestUtils.LOGS_RPC_RESPONSE
        self.manager.set_new_tx(L1TestUtils.FEEDER_TX, L1TestUtils.L2_BLOCK_TIMESTAMP)

        # Test.
        block_number = self.manager.get_block_number()
        self.assertEqual(block_number, L1TestUtils.BLOCK_NUMBER_RPC_RESPONSE)

        block = self.manager.get_block_by_number(L1TestUtils.BLOCK_NUMBER_HEX)
        self.assertEqual(block, L1TestUtils.BLOCK_RPC_RESPONSE)

        logs = self.manager.get_logs(L1TestUtils.BLOCK_NUMBER, L1TestUtils.BLOCK_NUMBER)
        self.assertEqual(logs, L1TestUtils.LOGS_RPC_RESPONSE)

    @patch("l1_manager.L1Blocks.find_l1_block_for_tx")
    def test_multiple_blocks(self, mock_find_l1_block_for_tx):
        # Setup: add blocks 10, 20, 30.
        for block_num in [10, 20, 30]:
            mock_find_l1_block_for_tx.return_value = block_num
            self.mock_client.get_block_by_number.return_value = {
                "jsonrpc": "2.0",
                "id": "1",
                "result": {"number": hex(block_num), "timestamp": "0x123"},
            }
            self.mock_client.get_logs.return_value = {
                "jsonrpc": "2.0",
                "id": "1",
                "result": [{"blockNumber": hex(block_num), "data": "0x"}],
            }
            self.manager.set_new_tx({"transaction_hash": f"0x{block_num}"}, 0)

        # get_block_number returns latest block in manager.
        result = self.manager.get_block_number()
        self.assertEqual(result["result"], hex(30))

        # get_logs merges all logs in range.
        result = self.manager.get_logs(10, 30)
        expected_logs = {
            "jsonrpc": "2.0",
            "id": "1",
            "result": [
                {"blockNumber": hex(10), "data": "0x"},
                {"blockNumber": hex(20), "data": "0x"},
                {"blockNumber": hex(30), "data": "0x"},
            ],
        }
        self.assertEqual(result, expected_logs)

        # get_logs with partial range (only 20 exists in 15-25).
        result = self.manager.get_logs(15, 25)
        expected_logs = {
            "jsonrpc": "2.0",
            "id": "1",
            "result": [{"blockNumber": hex(20), "data": "0x"}],
        }
        self.assertEqual(result, expected_logs)

    @patch("l1_manager.L1Blocks.find_l1_block_for_tx")
    def test_cleanup_old_blocks(self, mock_find_l1_block_for_tx):
        # Setup: add blocks 10, 20, 30.
        for block_num in [10, 20, 30]:
            mock_find_l1_block_for_tx.return_value = block_num
            self.mock_client.get_block_by_number.return_value = {
                "jsonrpc": "2.0",
                "id": "1",
                "result": {"number": hex(block_num), "timestamp": "0x123"},
            }
            self.mock_client.get_logs.return_value = {
                "jsonrpc": "2.0",
                "id": "1",
                "result": [{"blockNumber": hex(block_num), "data": "0x"}],
            }
            self.manager.set_new_tx({"transaction_hash": f"0x{block_num}"}, 0)

        # get_block_by_number removed older blocks (< 20).
        self.manager.get_block_by_number(hex(20))
        result = self.manager.get_block_by_number(hex(10))
        # Block 10 was cleaned up, should return default block
        expected_default_block = {
            "number": hex(10),
            "hash": format_hex(0),
            "timestamp": "0x0",
        }
        self.assertEqual(result["result"], expected_default_block)
        result = self.manager.get_block_by_number(hex(20))
        self.assertEqual(result["result"]["number"], hex(20))
        result = self.manager.get_block_by_number(hex(30))
        self.assertEqual(result["result"]["number"], hex(30))

        # get_block_number still returns 30.
        result = self.manager.get_block_number()
        self.assertEqual(result["result"], hex(30))


if __name__ == "__main__":
    unittest.main()
