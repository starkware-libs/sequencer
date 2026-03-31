import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../../.."))

import unittest
from typing import Optional
from unittest.mock import Mock, patch

from test_utils import L1TestUtils

from echonet.constants import DEFAULT_L1_BLOCK_NUMBER
from echonet.helpers import format_hex
from echonet.l1_logic.l1_blocks import L1Blocks
from echonet.l1_logic.l1_client import L1Client
from echonet.l1_logic.l1_manager import L1Manager


class TestL1Manager(unittest.TestCase):
    def setUp(self):
        self.mock_client = Mock(spec=L1Client)
        # get_last_proved_block callback not used in these tests (only needed for get_call).
        # get_last_echonet_block_callback returns a high value so all pending logs are immediately ready.
        self.manager = L1Manager(
            l1_client=self.mock_client,
            get_last_proved_block_callback=lambda: (0, 0),
            get_last_echonet_block_callback=lambda: 999_999_999,
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

    def _make_manager_with_echonet_block(self, echonet_block: Optional[int]) -> L1Manager:
        """Create an L1Manager that reports the given echonet block height."""
        return L1Manager(
            l1_client=self.mock_client,
            get_last_proved_block_callback=lambda: (0, 0),
            get_last_echonet_block_callback=lambda: echonet_block,
        )

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
        self.manager.set_new_tx(
            L1TestUtils.FEEDER_TX, L1TestUtils.L2_BLOCK_TIMESTAMP, source_block_number=2
        )

        # get_block_number returns mock L1 head synced to real block + finality.
        block_number = self.manager.get_block_number()
        self.assertEqual(
            block_number["result"],
            hex(L1TestUtils.BLOCK_NUMBER + L1Manager.L1_SCRAPER_FINALITY_CONFIG_VALUE),
        )

        # get_block_by_number overrides hash and parentHash to 0x0 to avoid false reorgs.
        block = self.manager.get_block_by_number(L1TestUtils.BLOCK_NUMBER_HEX)
        self.assertEqual(block["result"]["number"], L1TestUtils.BLOCK_NUMBER_HEX)
        self.assertEqual(block["result"]["hash"], format_hex(0))
        self.assertEqual(block["result"]["parentHash"], format_hex(0))

        # Logs are injected via _pending_logs (range-independent).
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

        # First get_logs call: all 3 pending logs injected (range-independent).
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

        # Second get_logs call: all pending logs were consumed; no new logs regardless of range.
        result = self.manager.get_logs(self.get_logs_input(b10, b30))
        self.assertEqual(result["result"], [])

    def test_cleanup_old_blocks(self):
        b10 = DEFAULT_L1_BLOCK_NUMBER + 10
        b20 = DEFAULT_L1_BLOCK_NUMBER + 20
        b30 = DEFAULT_L1_BLOCK_NUMBER + 30
        for block_num in [b10, b20, b30]:
            self._mock_handle_feeder_tx_and_store_l1_block(block_num)

        self.assertIn(b10, self.manager._l1_tx_data_by_block, "Block b10 should be stored")
        self.assertIn(b20, self.manager._l1_tx_data_by_block, "Block b20 should be stored")
        self.assertIn(b30, self.manager._l1_tx_data_by_block, "Block b30 should be stored")

        # b30's stored data is returned directly.
        result = self.manager.get_block_by_number(hex(b30))
        self.assertEqual(result["result"]["number"], hex(b30))

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

    def test_behind_range_uses_high_mock_l1_head_when_no_real_blocks_stored(self):
        """A real block arriving below the events scraper window does not reduce the mock head."""
        # Advance mock head by setting gas price 20 times.
        for _ in range(20):
            self.manager.set_gas_price_target(1000, 2000)
        high_mock_head = DEFAULT_L1_BLOCK_NUMBER + 20
        self.assertEqual(self.manager.get_block_number()["result"], hex(high_mock_head))

        # events_scraper_safe_block = max(0, high_mock_head - 10) = DEFAULT + 10.
        # A real block at DEFAULT + 5 is below the safe window.
        low_block = DEFAULT_L1_BLOCK_NUMBER + 5
        self._mock_handle_feeder_tx_and_store_l1_block(low_block)

        # Mock head stays at the high value; the late block does not pull it down.
        self.assertEqual(self.manager.get_block_number()["result"], hex(high_mock_head))

    def test_out_of_order_block_delivery(self):
        """A block arriving behind the events scraper position is stored but does not advance mock head."""
        high_block = DEFAULT_L1_BLOCK_NUMBER + 100
        low_block = DEFAULT_L1_BLOCK_NUMBER + 5

        # Store a high block to advance the mock head.
        self._mock_handle_feeder_tx_and_store_l1_block(high_block)
        high_mock_head = high_block + L1Manager.L1_SCRAPER_FINALITY_CONFIG_VALUE
        self.assertEqual(self.manager.get_block_number()["result"], hex(high_mock_head))

        # Deliver a lower block (behind events scraper safe position).
        self._mock_handle_feeder_tx_and_store_l1_block(low_block)

        # Mock head unchanged.
        self.assertEqual(self.manager.get_block_number()["result"], hex(high_mock_head))

        # The block is still stored and its log was added to _pending_logs then injected.
        self.assertIn(low_block, self.manager._l1_tx_data_by_block)

    def test_echonet_block_gate_withholds_logs_until_threshold(self):
        """Logs from a real L1 block are withheld until echonet reaches source_block_number - 2."""
        source_block_number = 10
        required_echonet_block = source_block_number - 2  # = 8
        l1_block_number = DEFAULT_L1_BLOCK_NUMBER + 1
        l1_block_number_hex = hex(l1_block_number)
        feeder_tx_hash = "0xabc"

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

            # Echonet below threshold: logs are withheld.
            manager_below = self._make_manager_with_echonet_block(required_echonet_block - 1)
            manager_below.set_new_tx(
                {"transaction_hash": feeder_tx_hash}, 0, source_block_number
            )
            result = manager_below.get_logs(self.get_logs_input(0, l1_block_number))
            self.assertEqual(result["result"], [])
            self.assertEqual(len(manager_below._pending_logs), 1)

            # Echonet at threshold: logs are released.
            manager_at = self._make_manager_with_echonet_block(required_echonet_block)
            manager_at.set_new_tx(
                {"transaction_hash": feeder_tx_hash}, 0, source_block_number
            )
            result = manager_at.get_logs(self.get_logs_input(0, l1_block_number))
            self.assertEqual(len(result["result"]), 1)
            self.assertEqual(manager_at._pending_logs, [])

    def test_echonet_block_gate_withholds_pending_logs_until_threshold(self):
        """Pending logs remain in _pending_logs until the echonet block threshold is met."""
        source_block_number = 20
        required_echonet_block = source_block_number - 2  # = 18
        l1_block_number = DEFAULT_L1_BLOCK_NUMBER + 1
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

            echonet_block = required_echonet_block - 1
            manager = self._make_manager_with_echonet_block(echonet_block)
            manager.set_new_tx({"transaction_hash": "0xdef"}, 0, source_block_number)

            # First call: echonet below threshold, logs withheld.
            result = manager.get_logs(self.get_logs_input(0, l1_block_number))
            self.assertEqual(result["result"], [])
            self.assertEqual(len(manager._pending_logs), 1)

            # Advance echonet to threshold and retry.
            manager.get_last_echonet_block_callback = lambda: required_echonet_block

            result = manager.get_logs(self.get_logs_input(0, l1_block_number))
            self.assertEqual(len(result["result"]), 1)
            self.assertEqual(manager._pending_logs, [])


if __name__ == "__main__":
    unittest.main()
