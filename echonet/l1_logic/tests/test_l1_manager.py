import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../../.."))

import unittest
from typing import Optional
from unittest.mock import Mock, patch

from test_utils import L1TestUtils

from echonet.l1_logic.l1_blocks import L1Blocks
from echonet.l1_logic.l1_client import L1Client
from echonet.l1_logic.l1_manager import L1Manager


class TestL1Manager(unittest.TestCase):
    def setUp(self):
        self.mock_client = Mock(spec=L1Client)
        # get_last_proved_block callback not used in these tests (only needed for get_call).
        # get_last_echonet_block_callback returns a high value so all gated txs are immediately exposed.
        self.manager = L1Manager(
            l1_client=self.mock_client,
            get_last_proved_block_callback=lambda: (0, 0),
            get_last_echonet_block_callback=lambda: 999_999_999,
        )

    def _make_manager_with_echonet_block(self, echonet_block: Optional[int]) -> L1Manager:
        """Create an L1Manager that reports the given echonet block height."""
        return L1Manager(
            l1_client=self.mock_client,
            get_last_proved_block_callback=lambda: (0, 0),
            get_last_echonet_block_callback=lambda: echonet_block,
        )

    def get_logs_input(self, fromBlock: int, toBlock: int) -> dict:
        return {"fromBlock": hex(fromBlock), "toBlock": hex(toBlock)}

    def _mock_handle_feeder_tx_and_store_l1_block(
        self, l1_block_number: int, source_block_number: int = 2
    ):
        """Simulates processing a feeder gateway transaction and storing its matched L1 block data."""
        l1_block_number_hex = hex(l1_block_number)
        with patch.object(L1Blocks, "find_l1_block_for_tx") as mock_find_l1_block_for_tx:
            mock_find_l1_block_for_tx.return_value = l1_block_number
            self.mock_client.get_block_by_number.return_value = {
                "jsonrpc": "2.0",
                "id": "1",
                "result": {"number": l1_block_number_hex, "timestamp": "0x123"},
            }
            self.mock_client.get_logs.return_value = {
                "jsonrpc": "2.0",
                "id": "1",
                "result": [{"blockNumber": l1_block_number_hex, "data": "0x"}],
            }
            self.manager.set_new_tx(
                {"transaction_hash": l1_block_number_hex}, 0, source_block_number
            )

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

    @patch.object(L1Blocks, "find_l1_block_for_tx")
    def test_single_block(self, mock_find_l1_block_for_tx):
        # Setup.
        mock_find_l1_block_for_tx.return_value = L1TestUtils.BLOCK_NUMBER
        self.mock_client.get_block_by_number.return_value = L1TestUtils.BLOCK_RPC_RESPONSE
        self.mock_client.get_logs.return_value = L1TestUtils.LOGS_RPC_RESPONSE
        self.manager.set_new_tx(
            L1TestUtils.FEEDER_TX, L1TestUtils.L2_BLOCK_TIMESTAMP, source_block_number=2
        )

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

        # get_block_number returns latest exposed block + finality.
        result = self.manager.get_block_number()
        self.assertEqual(result["result"], hex(30 + L1Manager.L1_SCRAPER_FINALITY_CONFIG_VALUE))

        # get_logs returns logs for blocks within the queried range.
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

        # Same range returns the same logs (range-based, not consumed).
        result = self.manager.get_logs(self.get_logs_input(10, 30))
        self.assertEqual(result, expected_logs)

        # Range outside stored blocks returns empty.
        result = self.manager.get_logs(self.get_logs_input(31, 100))
        self.assertEqual(result["result"], [])

    def test_cleanup_old_blocks(self):
        # Setup: add blocks 10, 20, 30.
        for block_num in [10, 20, 30]:
            self._mock_handle_feeder_tx_and_store_l1_block(block_num)

        # get_block_by_number with cleanup buffer (2 * finality = 20)
        # When requesting block 30, it keeps blocks >= (30 - 20) = 10
        self.manager.get_block_by_number(hex(30))

        # Block 10 should still exist (within buffer of 30)
        self.assertIn(10, self.manager.blocks, "Block 10 should be kept (within cleanup buffer)")
        self.assertIn(20, self.manager.blocks, "Block 20 should be kept")
        self.assertIn(30, self.manager.blocks, "Block 30 should be kept")

        # Now request block 50 - this should clean up block 10 (50 - 20 = 30, so blocks < 30 are removed)
        self.manager.get_block_by_number(hex(50))
        result = self.manager.get_block_by_number(hex(10))
        # Block 10 was cleaned up, should return default block.
        self.assertEqual(result["result"], L1Manager.default_l1_block(hex(10)))
        result = self.manager.get_block_by_number(hex(20))
        # Block 20 was also cleaned up
        self.assertEqual(result["result"], L1Manager.default_l1_block(hex(20)))
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

    def test_get_block_number_gated_until_threshold(self):
        """get_block_number returns None while gated, then a stable value once exposed."""
        source_block_number = 100
        required_echonet_block = source_block_number - 2  # = 98
        l1_block_number = 10

        self.mock_client.get_block_by_number.return_value = {
            "jsonrpc": "2.0",
            "id": "1",
            "result": {"number": hex(l1_block_number), "timestamp": "0x1"},
        }
        self.mock_client.get_logs.return_value = {
            "jsonrpc": "2.0",
            "id": "1",
            "result": [{"blockNumber": hex(l1_block_number), "data": "0x"}],
        }

        with patch.object(L1Blocks, "find_l1_block_for_tx") as mock_find:
            mock_find.return_value = l1_block_number
            natural = hex(l1_block_number + L1Manager.L1_SCRAPER_FINALITY_CONFIG_VALUE)

            # Echonet below threshold: block not yet exposed, get_block_number returns None.
            manager_below = self._make_manager_with_echonet_block(required_echonet_block - 1)
            manager_below.set_new_tx({"transaction_hash": "0xabc"}, 0, source_block_number)
            self.assertIsNone(manager_below.get_block_number()["result"])
            self.assertIsNone(manager_below.get_block_number()["result"])
            self.assertIsNone(manager_below.get_block_number()["result"])

            # Echonet at threshold: block exposed, get_block_number returns stable value.
            manager_at = self._make_manager_with_echonet_block(required_echonet_block)
            manager_at.set_new_tx({"transaction_hash": "0xdef"}, 0, source_block_number)
            self.assertEqual(manager_at.get_block_number()["result"], natural)
            self.assertEqual(manager_at.get_block_number()["result"], natural)
            self.assertEqual(manager_at.get_block_number()["result"], natural)

    def test_echonet_block_gate_withholds_logs_until_threshold(self):
        """Logs are withheld until echonet reaches source_block_number - 2, then released."""
        source_block_number = 10
        required_echonet_block = source_block_number - 2  # = 8
        l1_block_number = 1
        l1_block_number_hex = hex(l1_block_number)

        self.mock_client.get_block_by_number.return_value = {
            "jsonrpc": "2.0",
            "id": "1",
            "result": {"number": l1_block_number_hex, "timestamp": "0x1"},
        }
        self.mock_client.get_logs.return_value = {
            "jsonrpc": "2.0",
            "id": "1",
            "result": [{"blockNumber": l1_block_number_hex, "data": "0x"}],
        }

        with patch.object(L1Blocks, "find_l1_block_for_tx") as mock_find:
            mock_find.return_value = l1_block_number

            # Echonet below threshold: block is gated, logs are withheld.
            manager = L1Manager(
                l1_client=self.mock_client,
                get_last_proved_block_callback=lambda: (0, 0),
                get_last_echonet_block_callback=lambda: required_echonet_block - 1,
            )
            manager.set_new_tx({"transaction_hash": "0xabc"}, 0, source_block_number)
            result = manager.get_logs(self.get_logs_input(0, l1_block_number))
            self.assertEqual(result["result"], [])
            self.assertEqual(len(manager._gated_txs), 1)

            # Advance echonet to threshold: block is exposed, logs are returned.
            manager.get_last_echonet_block_callback = lambda: required_echonet_block
            result = manager.get_logs(self.get_logs_input(0, l1_block_number))
            self.assertEqual(len(result["result"]), 1)
            self.assertEqual(manager._gated_txs, [])


if __name__ == "__main__":
    unittest.main()
