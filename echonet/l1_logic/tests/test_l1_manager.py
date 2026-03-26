import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../../.."))

import unittest
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
        # get_last_echonet_block_callback returns a high value so existing tests are unaffected
        # by the echonet-block gate; gating behaviour is tested separately below.
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
        """Simulates processing a feeder gateway transaction and storing its matched L1 block data.

        source_block_number defaults to 2 so required_echonet_block=0, always satisfied by the
        default callback (returns 999_999_999).
        """
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

        # get_block_by_number returns block data with hash/parentHash overridden to 0x0.
        block = self.manager.get_block_by_number(L1TestUtils.BLOCK_NUMBER_HEX)
        expected_result = {
            **L1TestUtils.BLOCK,
            "hash": format_hex(0),
            "parentHash": format_hex(0),
        }
        self.assertEqual(block["result"], expected_result)

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

        # All logs are delivered via pending injection on the first get_logs call, regardless
        # of the requested range. This is because all L1_HANDLER logs are queued in _pending_logs
        # so they survive even if the events scraper advances past the real L1 block number before
        # the echonet gate opens.
        result = self.manager.get_logs(self.get_logs_input(b10, b30))
        injected_block_numbers = [log["blockNumber"] for log in result["result"]]
        self.assertIn(hex(b10), injected_block_numbers)
        self.assertIn(hex(b20), injected_block_numbers)
        self.assertIn(hex(b30), injected_block_numbers)

        # Pending logs are consumed — a second call returns empty (no range-based fallback).
        result2 = self.manager.get_logs(self.get_logs_input(b10, b30))
        self.assertEqual(result2["result"], [])

    def test_cleanup_old_blocks(self):
        b10 = DEFAULT_L1_BLOCK_NUMBER + 10
        b20 = DEFAULT_L1_BLOCK_NUMBER + 20
        b30 = DEFAULT_L1_BLOCK_NUMBER + 30
        for block_num in [b10, b20, b30]:
            self._mock_handle_feeder_tx_and_store_l1_block(block_num)

        self.assertIn(b10, self.manager._l1_tx_data_by_block)
        self.assertIn(b20, self.manager._l1_tx_data_by_block)
        self.assertIn(b30, self.manager._l1_tx_data_by_block)

        # get_block_by_number returns correct data for all stored blocks.
        for block_num in [b10, b20, b30]:
            result = self.manager.get_block_by_number(hex(block_num))
            self.assertEqual(result["result"]["number"], hex(block_num))

        # Mock L1 head synced to b30 + finality = DEFAULT + 40.
        result = self.manager.get_block_number()
        self.assertEqual(result["result"], hex(DEFAULT_L1_BLOCK_NUMBER + 40))

    def test_clear_stored_blocks(self):
        # Setup.
        block_num = 10
        self._mock_handle_feeder_tx_and_store_l1_block(block_num)

        # Verify block exists; mock L1 head unchanged (block_num 10 < DEFAULT).
        result = self.manager.get_block_number()
        self.assertEqual(result["result"], hex(DEFAULT_L1_BLOCK_NUMBER))

        self.manager.clear_stored_blocks()

        # After clearing, mock L1 head is reset to default.
        result = self.manager.get_block_number()
        self.assertEqual(result["result"], hex(DEFAULT_L1_BLOCK_NUMBER))
        result = self.manager.get_logs(self.get_logs_input(10, 10))
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
        """A real block below mock-head-minus-finality is queued as pending even if _max_real_block=0."""
        # Simulate mock L1 head being advanced (e.g., by gas price increments) without any real
        # blocks being stored. This happens in production when the head was synced up by a prior
        # run and blocks were subsequently cleaned up.
        high_mock_l1_head = DEFAULT_L1_BLOCK_NUMBER + 1000  # 22001000
        self.manager._mock_l1_head_number = high_mock_l1_head
        self.assertEqual(self.manager._max_real_block, 0)

        # A real block arrives well below mock_l1_head - finality (= 22000990).
        low_block = DEFAULT_L1_BLOCK_NUMBER + 500  # 22000500
        self._mock_handle_feeder_tx_and_store_l1_block(low_block)

        self.assertIn(low_block, self.manager._l1_tx_data_by_block)
        self.assertEqual(len(self.manager._pending_logs), 1)

        # get_logs injects the pending log regardless of the queried range.
        result = self.manager.get_logs(
            self.get_logs_input(high_mock_l1_head, high_mock_l1_head + 50)
        )
        log_block_numbers = [log["blockNumber"] for log in result["result"]]
        self.assertIn(hex(low_block), log_block_numbers)

        # Pending log is consumed — second call returns no injected logs.
        result2 = self.manager.get_logs(
            self.get_logs_input(high_mock_l1_head, high_mock_l1_head + 50)
        )
        log_block_numbers2 = [log["blockNumber"] for log in result2["result"]]
        self.assertNotIn(hex(low_block), log_block_numbers2)

        # get_block_by_number still returns the correct block data.
        block = self.manager.get_block_by_number(hex(low_block))
        self.assertEqual(block["result"]["number"], hex(low_block))

    def test_out_of_order_block_delivery(self):
        """All L1_HANDLER logs are routed through _pending_logs and injected into the next get_logs call."""
        high_block = DEFAULT_L1_BLOCK_NUMBER + 1000  # 22001000
        low_block = DEFAULT_L1_BLOCK_NUMBER + 500  # 22000500 — arrives second, below high_block

        # Store the high block first.
        self._mock_handle_feeder_tx_and_store_l1_block(high_block)
        self.assertIn(high_block, self.manager._l1_tx_data_by_block)
        self.assertEqual(self.manager._max_real_block, high_block)

        # Store the low block second (arrives out of order, below high_block).
        self._mock_handle_feeder_tx_and_store_l1_block(low_block)
        self.assertIn(low_block, self.manager._l1_tx_data_by_block)
        self.assertEqual(len(self.manager._pending_logs), 2)

        # get_logs injects both pending logs regardless of the requested range.
        result = self.manager.get_logs(self.get_logs_input(high_block, high_block + 50))
        log_block_numbers = [log["blockNumber"] for log in result["result"]]
        self.assertIn(hex(low_block), log_block_numbers, "low_block log should be injected")
        self.assertIn(hex(high_block), log_block_numbers, "high_block log should be injected")

        # Pending logs are consumed — a second call returns empty.
        result2 = self.manager.get_logs(self.get_logs_input(high_block, high_block + 50))
        self.assertEqual(result2["result"], [], "pending already consumed")

        # get_block_by_number returns correct data for both blocks.
        for block_num in [high_block, low_block]:
            block = self.manager.get_block_by_number(hex(block_num))
            self.assertEqual(block["result"]["number"], hex(block_num))

    def _make_manager_with_echonet_block(self, echonet_block: int) -> L1Manager:
        """Return an L1Manager whose echonet-block callback returns the given fixed value."""
        return L1Manager(
            l1_client=self.mock_client,
            get_last_proved_block_callback=lambda: (0, 0),
            get_last_echonet_block_callback=lambda: echonet_block,
        )

    def test_echonet_block_gate_withholds_logs_until_threshold(self):
        """Logs are not delivered until echonet has reached source_block_number - 2."""
        source_block_number = 100
        required_echonet_block = source_block_number - 2  # 98

        l1_block_number = DEFAULT_L1_BLOCK_NUMBER + 10

        # Manager whose echonet block is below the threshold.
        manager_before = self._make_manager_with_echonet_block(required_echonet_block - 1)
        with patch.object(L1Blocks, "find_l1_block_for_tx", return_value=l1_block_number):
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
            manager_before.set_new_tx(
                {"transaction_hash": hex(l1_block_number)}, 0, source_block_number
            )

        # Logs must not be returned while echonet is below the threshold.
        result = manager_before.get_logs(self.get_logs_input(l1_block_number, l1_block_number))
        self.assertEqual(result["result"], [], "logs must be withheld before echonet threshold")

        # Manager whose echonet block has reached the threshold.
        manager_after = self._make_manager_with_echonet_block(required_echonet_block)
        with patch.object(L1Blocks, "find_l1_block_for_tx", return_value=l1_block_number):
            manager_after.set_new_tx(
                {"transaction_hash": hex(l1_block_number)}, 0, source_block_number
            )

        result = manager_after.get_logs(self.get_logs_input(l1_block_number, l1_block_number))
        self.assertEqual(
            len(result["result"]), 1, "logs must be exposed once echonet reaches threshold"
        )

    def test_echonet_block_gate_withholds_pending_logs_until_threshold(self):
        """Behind-range pending logs are also withheld until the echonet threshold is met."""
        source_block_number = 100
        required_echonet_block = source_block_number - 2  # 98

        # Use a block number well below the mock L1 head so it is stored while head is still high.
        l1_block_number = 10

        def _store(manager: L1Manager) -> None:
            with patch.object(L1Blocks, "find_l1_block_for_tx", return_value=l1_block_number):
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
                manager.set_new_tx(
                    {"transaction_hash": hex(l1_block_number)}, 0, source_block_number
                )

        # Below threshold: pending log is stored but not injected.
        manager_before = self._make_manager_with_echonet_block(required_echonet_block - 1)
        _store(manager_before)
        self.assertEqual(len(manager_before._pending_logs), 1)
        result = manager_before.get_logs(self.get_logs_input(1000, 2000))
        self.assertEqual(
            result["result"], [], "pending log must be withheld before echonet threshold"
        )
        self.assertEqual(len(manager_before._pending_logs), 1, "pending log must remain queued")

        # At threshold: pending log is injected and consumed.
        manager_after = self._make_manager_with_echonet_block(required_echonet_block)
        _store(manager_after)
        result = manager_after.get_logs(self.get_logs_input(1000, 2000))
        self.assertEqual(len(result["result"]), 1, "pending log must be injected at threshold")
        self.assertEqual(len(manager_after._pending_logs), 0, "pending log must be consumed")


if __name__ == "__main__":
    unittest.main()
