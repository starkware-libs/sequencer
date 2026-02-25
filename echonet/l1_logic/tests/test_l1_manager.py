import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../../.."))

import unittest
from unittest.mock import Mock, patch

from test_utils import L1TestUtils

from echonet.l1_logic.l1_blocks import L1Blocks
from echonet.l1_logic.l1_client import L1Client
from echonet.l1_logic.l1_manager import L1Manager


class TestL1Manager(unittest.TestCase):
    def setUp(self):
        self.mock_client = Mock(spec=L1Client)
        # get_last_proved_block callback not used in these tests (only needed for get_call).
        self.manager = L1Manager(
            l1_client=self.mock_client, get_last_proved_block_callback=lambda: (0, 0)
        )

    def get_logs_input(self, fromBlock: int, toBlock: int) -> dict:
        return {"fromBlock": hex(fromBlock), "toBlock": hex(toBlock)}

    def _mock_handle_feeder_tx_and_store_l1_block(self, l1_block_number: int):
        """Simulates processing a feeder gateway transaction and storing its matched L1 block data."""
        l1_block_number_hex = hex(l1_block_number)
        with patch.object(L1Blocks, "find_l1_log_for_tx") as mock_find_l1_log_for_tx:
            mock_find_l1_log_for_tx.return_value = L1Blocks.MatchedL1Log(
                log={
                    "blockNumber": l1_block_number_hex,
                    "transactionHash": l1_block_number_hex,
                    "logIndex": "0x0",
                    "data": "0x",
                },
                l1_tx_hash=l1_block_number_hex,
                block_number=l1_block_number,
                block_timestamp=0,
            )
            self.mock_client.get_block_by_number.return_value = {
                "jsonrpc": "2.0",
                "id": "1",
                "result": {"number": l1_block_number_hex, "timestamp": "0x123"},
            }
            self.manager.set_new_tx({"transaction_hash": l1_block_number_hex}, 0)

    def test_empty_queue_returns_defaults(self):
        block_number = self.manager.get_block_number()
        self.assertEqual(block_number, {"jsonrpc": "2.0", "id": "1", "result": None})

        block_number_hex = hex(1)
        block = self.manager.get_block_by_number(block_number_hex)
        self.assertEqual(
            block,
            {
                "jsonrpc": "2.0",
                "id": "1",
                "result": L1Manager.default_l1_block(block_number_hex),
            },
        )

        logs = self.manager.get_logs(self.get_logs_input(0, 100))
        self.assertEqual(logs, {"jsonrpc": "2.0", "id": "1", "result": []})

    @patch.object(L1Blocks, "find_l1_log_for_tx")
    def test_single_block(self, mock_find_l1_log_for_tx):
        # Setup.
        mock_find_l1_log_for_tx.return_value = L1Blocks.MatchedL1Log(
            log=L1TestUtils.LOG,
            l1_tx_hash=L1TestUtils.LOG["transactionHash"],
            block_number=L1TestUtils.BLOCK_NUMBER,
            block_timestamp=L1TestUtils.BLOCK_TIMESTAMP,
        )
        self.mock_client.get_block_by_number.return_value = L1TestUtils.BLOCK_RPC_RESPONSE
        self.manager.set_new_tx(L1TestUtils.FEEDER_TX, L1TestUtils.L2_BLOCK_TIMESTAMP)

        # Test.
        block_number = self.manager.get_block_number()
        self.assertEqual(
            block_number["result"],
            hex(L1TestUtils.BLOCK_NUMBER + L1Manager.L1_SCRAPER_FINALITY_CONFIG_VALUE),
        )

        block = self.manager.get_block_by_number(L1TestUtils.BLOCK_NUMBER_HEX)
        self.assertEqual(block, L1TestUtils.BLOCK_RPC_RESPONSE)

        logs = self.manager.get_logs(
            self.get_logs_input(L1TestUtils.BLOCK_NUMBER, L1TestUtils.BLOCK_NUMBER)
        )

        self.assertEqual(logs, L1TestUtils.LOGS_RPC_RESPONSE)

    def test_multiple_blocks(self):
        # Setup: add blocks 10, 20, 30.
        for block_num in [10, 20, 30]:
            self._mock_handle_feeder_tx_and_store_l1_block(block_num)

        # get_block_number returns latest block in manager + finality.
        result = self.manager.get_block_number()
        self.assertEqual(result["result"], hex(30 + L1Manager.L1_SCRAPER_FINALITY_CONFIG_VALUE))

        # get_logs merges all logs in range.
        result = self.manager.get_logs(self.get_logs_input(10, 30))
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
        result = self.manager.get_logs(self.get_logs_input(15, 25))
        expected_logs = {
            "jsonrpc": "2.0",
            "id": "1",
            "result": [{"blockNumber": hex(20), "data": "0x"}],
        }
        self.assertEqual(result, expected_logs)

    def test_cleanup_old_blocks(self):
        # Setup: add blocks 10, 20, 30.
        for block_num in [10, 20, 30]:
            self._mock_handle_feeder_tx_and_store_l1_block(block_num)

        # get_block_by_number should not remove older blocks (cleanup is size-based).
        self.manager.get_block_by_number(hex(20))
        result = self.manager.get_block_by_number(hex(10))
        self.assertEqual(result["result"]["number"], hex(10))
        result = self.manager.get_block_by_number(hex(20))
        self.assertEqual(result["result"]["number"], hex(20))
        result = self.manager.get_block_by_number(hex(30))
        self.assertEqual(result["result"]["number"], hex(30))

        # get_block_number still returns 30 + finality.
        result = self.manager.get_block_number()
        self.assertEqual(result["result"], hex(30 + L1Manager.L1_SCRAPER_FINALITY_CONFIG_VALUE))

    def test_clear_stored_blocks(self):
        # Setup.
        block_num = 10
        self._mock_handle_feeder_tx_and_store_l1_block(block_num)

        # Verify block exists.
        result = self.manager.get_block_number()
        self.assertEqual(
            result["result"], hex(block_num + L1Manager.L1_SCRAPER_FINALITY_CONFIG_VALUE)
        )

        self.manager.clear_stored_blocks()

        # Verify blocks cleared (defaults returned).
        result = self.manager.get_block_number()
        self.assertEqual(result, {"jsonrpc": "2.0", "id": "1", "result": None})
        result = self.manager.get_logs(self.get_logs_input(10, 10))
        self.assertEqual(result, {"jsonrpc": "2.0", "id": "1", "result": []})
        result = self.manager.get_block_by_number(hex(block_num))
        self.assertEqual(
            result,
            {"jsonrpc": "2.0", "id": "1", "result": L1Manager.default_l1_block(hex(block_num))},
        )

    @patch.object(L1Blocks, "find_l1_log_for_tx")
    def test_multiple_txs_same_l1_block_are_merged(self, mock_find_l1_log_for_tx):
        # Two different logs in the same L1 block should both be served via get_logs.
        block_number = 10
        self.mock_client.get_block_by_number.return_value = {
            "jsonrpc": "2.0",
            "id": "1",
            "result": {"number": hex(block_number), "timestamp": "0x123"},
        }
        mock_find_l1_log_for_tx.side_effect = [
            L1Blocks.MatchedL1Log(
                log={
                    "blockNumber": hex(block_number),
                    "transactionHash": "0xaaa",
                    "logIndex": "0x1",
                    "data": "0x",
                },
                l1_tx_hash="0xaaa",
                block_number=block_number,
                block_timestamp=0,
            ),
            L1Blocks.MatchedL1Log(
                log={
                    "blockNumber": hex(block_number),
                    "transactionHash": "0xbbb",
                    "logIndex": "0x2",
                    "data": "0x",
                },
                l1_tx_hash="0xbbb",
                block_number=block_number,
                block_timestamp=0,
            ),
        ]

        self.manager.set_new_tx({"transaction_hash": "0x1"}, 0)
        self.manager.set_new_tx({"transaction_hash": "0x2"}, 0)

        logs = self.manager.get_logs(self.get_logs_input(block_number, block_number))
        self.assertEqual(len(logs["result"]), 2)
        self.assertEqual({l["transactionHash"] for l in logs["result"]}, {"0xaaa", "0xbbb"})


if __name__ == "__main__":
    unittest.main()
