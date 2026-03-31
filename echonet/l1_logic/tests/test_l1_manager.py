import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../../.."))

import unittest
from unittest.mock import Mock, patch

from test_utils import L1TestUtils

from echonet.constants import DEFAULT_L1_BLOCK_NUMBER
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
            self.manager.set_new_tx({"transaction_hash": l1_block_number_hex}, 0)

    def test_empty_queue_returns_defaults(self):
        # get_block_number returns the default mock L1 head (eth_blockNumber) when no blocks are stored.
        block_number = self.manager.get_block_number()
        self.assertEqual(
            block_number, {"jsonrpc": "2.0", "id": "1", "result": hex(DEFAULT_L1_BLOCK_NUMBER)}
        )

        block_number_hex = hex(1)
        block = self.manager.get_block_by_number(block_number_hex)
        self.assertEqual(
            block,
            {
                "jsonrpc": "2.0",
                "id": "1",
                "result": self.manager.default_l1_block(block_number_hex),
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
        self.manager.set_new_tx(L1TestUtils.FEEDER_TX, L1TestUtils.L2_BLOCK_TIMESTAMP)

        # get_block_number returns mock L1 head synced to real block + finality.
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
        b10 = DEFAULT_L1_BLOCK_NUMBER + 10
        b20 = DEFAULT_L1_BLOCK_NUMBER + 20
        b30 = DEFAULT_L1_BLOCK_NUMBER + 30
        for block_num in [b10, b20, b30]:
            self._mock_handle_feeder_tx_and_store_l1_block(block_num)

        # Mock L1 head syncs to b30 + finality (10) = DEFAULT + 40.
        result = self.manager.get_block_number()
        self.assertEqual(result["result"], hex(DEFAULT_L1_BLOCK_NUMBER + 40))

        # get_logs merges all logs in range.
        result = self.manager.get_logs(self.get_logs_input(b10, b30))
        expected_logs = {
            "jsonrpc": "2.0",
            "id": "1",
            "result": [
                {"blockNumber": hex(b10), "data": "0x"},
                {"blockNumber": hex(b20), "data": "0x"},
                {"blockNumber": hex(b30), "data": "0x"},
            ],
        }
        self.assertEqual(result, expected_logs)

        # get_logs with partial range (only b20 exists in [b15, b25]).
        b15 = DEFAULT_L1_BLOCK_NUMBER + 15
        b25 = DEFAULT_L1_BLOCK_NUMBER + 25
        result = self.manager.get_logs(self.get_logs_input(b15, b25))
        expected_logs = {
            "jsonrpc": "2.0",
            "id": "1",
            "result": [{"blockNumber": hex(b20), "data": "0x"}],
        }
        self.assertEqual(result, expected_logs)

    def test_cleanup_old_blocks(self):
        b10 = DEFAULT_L1_BLOCK_NUMBER + 10
        b20 = DEFAULT_L1_BLOCK_NUMBER + 20
        b30 = DEFAULT_L1_BLOCK_NUMBER + 30
        for block_num in [b10, b20, b30]:
            self._mock_handle_feeder_tx_and_store_l1_block(block_num)

        self.assertIn(b10, self.manager.blocks, "Block b10 should be stored")
        self.assertIn(b20, self.manager.blocks, "Block b20 should be stored")
        self.assertIn(b30, self.manager.blocks, "Block b30 should be stored")

        # Query DEFAULT+50: cleanup threshold = DEFAULT+30. b10 and b20 are strictly below
        # the threshold and are evicted; b30 is exactly at the threshold and survives.
        b50 = DEFAULT_L1_BLOCK_NUMBER + 50
        self.manager.get_block_by_number(hex(b50))
        self.assertNotIn(b10, self.manager.blocks, "Block b10 should be evicted")
        self.assertNotIn(b20, self.manager.blocks, "Block b20 should be evicted")
        self.assertIn(b30, self.manager.blocks, "Block b30 should survive cleanup")

        # b30's stored data is still returned (not a default block fallback).
        result = self.manager.get_block_by_number(hex(b30))
        self.assertEqual(result["result"]["number"], hex(b30))

        # Mock L1 head synced to b30 + finality = DEFAULT + 40.
        result = self.manager.get_block_number()
        self.assertEqual(result["result"], hex(DEFAULT_L1_BLOCK_NUMBER + 40))

    def test_clear_stored_blocks(self):
        # Setup with a block above DEFAULT so the mock head syncs.
        block_num = DEFAULT_L1_BLOCK_NUMBER + 10
        self._mock_handle_feeder_tx_and_store_l1_block(block_num)

        # Verify block exists; mock L1 head synced to block_num + finality.
        result = self.manager.get_block_number()
        self.assertEqual(
            result["result"], hex(block_num + L1Manager.L1_SCRAPER_FINALITY_CONFIG_VALUE)
        )

        self.manager.clear_stored_blocks()

        # After clearing, mock L1 head is reset to DEFAULT_L1_BLOCK_NUMBER.
        result = self.manager.get_block_number()
        self.assertEqual(result["result"], hex(DEFAULT_L1_BLOCK_NUMBER))
        result = self.manager.get_logs(self.get_logs_input(block_num, block_num))
        self.assertEqual(result, {"jsonrpc": "2.0", "id": "1", "result": []})
        result = self.manager.get_block_by_number(hex(block_num))
        self.assertEqual(
            result,
            {"jsonrpc": "2.0", "id": "1", "result": self.manager.default_l1_block(hex(block_num))},
        )

    def test_mock_l1_head_syncs_with_real_block(self):
        """Mock L1 head tracks real L1 block number so the scraper does not get stuck."""
        # Start with default mock L1 head.
        self.assertEqual(
            self.manager.get_block_number()["result"],
            hex(DEFAULT_L1_BLOCK_NUMBER),
        )

        # Store a real block above DEFAULT.
        real_block_num = DEFAULT_L1_BLOCK_NUMBER + 1000
        self._mock_handle_feeder_tx_and_store_l1_block(real_block_num)

        # Mock L1 head synced to real_block_num + finality.
        self.assertEqual(
            self.manager.get_block_number()["result"],
            hex(real_block_num + L1Manager.L1_SCRAPER_FINALITY_CONFIG_VALUE),
        )

        # After clearing stored blocks, mock L1 head is reset to default.
        # This ensures the events scraper re-initializes from a low block number after resync,
        # so it doesn't skip real blocks that arrive after resync at positions below the old mock head.
        self.manager.clear_stored_blocks()
        self.assertEqual(
            self.manager.get_block_number()["result"],
            hex(DEFAULT_L1_BLOCK_NUMBER),
        )

        # set_gas_price_target increments from the reset default position.
        self.manager.set_gas_price_target(1000, 2000, l2_timestamp=12345)

        self.assertEqual(
            self.manager.get_block_number()["result"],
            hex(DEFAULT_L1_BLOCK_NUMBER + 1),
        )


if __name__ == "__main__":
    unittest.main()
