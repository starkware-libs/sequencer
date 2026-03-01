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

    def test_two_consecutive_l1_blocks_both_retrievable(self):
        """
        Reproduces and tests the fix for the bug from L2 block 7035266.

        Bug scenario:
        - Two L1 handler txs in consecutive L1 blocks (24511510 and 24511511)
        - Both blocks stored in echonet
        - Sequencer calls eth_getBlockByNumber(24511511) - this triggered cleanup
        - Old code: deleted block 24511510 (all blocks < 24511511)
        - Sequencer calls eth_getLogs(from=24511351, to=24511511)
        - Only TX 2 from block 24511511 was returned (TX 1 was deleted)

        Fix:
        - Add CLEANUP_BUFFER = 20 blocks
        - Only delete blocks that are much older (> 20 blocks behind)
        - Block 24511510 is kept because 24511510 >= (24511511 - 20)

        This test:
        - Stores two consecutive blocks
        - Calls get_block_by_number(latest) which triggers cleanup
        - Verifies earlier block is NOT deleted (with fix) or IS deleted (without fix)
        - Verifies both blocks' logs are returned
        """
        # Setup: Use actual block numbers from the bug (24511510 and 24511511)
        block_10 = 24511510
        block_11 = 24511511

        # Create unique log data for each block so we can verify which logs are returned.
        with patch.object(L1Blocks, "find_l1_block_for_tx") as mock_find:
            # Store block ...10 with TX 1
            mock_find.return_value = block_10
            self.mock_client.get_block_by_number.return_value = {
                "jsonrpc": "2.0",
                "id": "1",
                "result": {"number": hex(block_10), "timestamp": "0x123"},
            }
            self.mock_client.get_logs.return_value = {
                "jsonrpc": "2.0",
                "id": "1",
                "result": [
                    {"blockNumber": hex(block_10), "transactionHash": "0xTX1", "data": "0x"}
                ],
            }
            self.manager.set_new_tx({"transaction_hash": "0xTX1"}, 0)

            # Store block ...11 with TX 2
            mock_find.return_value = block_11
            self.mock_client.get_block_by_number.return_value = {
                "jsonrpc": "2.0",
                "id": "1",
                "result": {"number": hex(block_11), "timestamp": "0x124"},
            }
            self.mock_client.get_logs.return_value = {
                "jsonrpc": "2.0",
                "id": "1",
                "result": [
                    {"blockNumber": hex(block_11), "transactionHash": "0xTX2", "data": "0x"}
                ],
            }
            self.manager.set_new_tx({"transaction_hash": "0xTX2"}, 0)

        # Verify both blocks are stored initially
        self.assertIn(block_10, self.manager.blocks, "Block 24511510 should be stored initially")
        self.assertIn(block_11, self.manager.blocks, "Block 24511511 should be stored initially")
        print(f"\n✓ Initial state: Both blocks stored: {list(self.manager.blocks.keys())}")

        # SEQUENCER FLOW: Reproduce the exact bug scenario from the logs
        # At 13:25:17.793, sequencer called eth_getBlockByNumber('0x1760417') [24511511]
        # This is what triggered the cleanup that deleted block 24511510
        print(f"\n→ Sequencer calls eth_getBlockByNumber({hex(block_11)}) [block {block_11}]")
        block_data = self.manager.get_block_by_number(hex(block_11))
        self.assertIsNotNone(block_data, f"Block {block_11} data should be returned")

        # CRITICAL ASSERTION: Block 24511510 should STILL be in storage after the cleanup
        # With the BUG (old code): blocks_to_remove = [24511510] → DELETED ❌
        # With the FIX (new code): blocks_to_remove = [] (24511510 is within buffer) → KEPT ✓
        print(f"→ After cleanup, storage contains: {list(self.manager.blocks.keys())}")
        self.assertIn(
            block_10,
            self.manager.blocks,
            f"CRITICAL BUG: Block {block_10} was deleted by get_block_by_number({block_11})! "
            f"Without the cleanup buffer fix, this block is deleted before eth_getLogs can retrieve it. "
            f"Current storage: {list(self.manager.blocks.keys())}",
        )
        print(f"✓ Block {block_10} still in storage (cleanup buffer working!)")

        # At 13:25:17.798, sequencer called eth_getLogs(from='0x1760377', to='0x1760417')
        # Range [24511351, 24511511] should include block 24511510
        print(f"\n→ Sequencer calls eth_getLogs(from={block_10}, to={block_11})")
        logs_result = self.manager.get_logs(self.get_logs_input(block_10, block_11))

        # ASSERTION: Both blocks' logs must be returned
        returned_logs = logs_result["result"]
        returned_blocks = [int(log["blockNumber"], 16) for log in returned_logs]
        returned_tx_hashes = [log.get("transactionHash") for log in returned_logs]

        print(f"→ Returned blocks: {returned_blocks}")
        print(f"→ Returned tx hashes: {returned_tx_hashes}")

        self.assertEqual(
            len(returned_logs),
            2,
            f"Expected 2 logs (one per block), got {len(returned_logs)}. "
            f"If only 1 log is returned, the bug is present: block {block_10} was deleted before it could be queried.",
        )
        self.assertIn(
            block_10, returned_blocks, f"TX 1 from block {block_10} missing - this was the bug!"
        )
        self.assertIn(block_11, returned_blocks, f"TX 2 from block {block_11} missing")
        self.assertIn("0xTX1", returned_tx_hashes, "TX 1 should be in logs")
        self.assertIn("0xTX2", returned_tx_hashes, "TX 2 should be in logs")

        print(f"✓ Both transactions returned successfully!")


if __name__ == "__main__":
    unittest.main()
