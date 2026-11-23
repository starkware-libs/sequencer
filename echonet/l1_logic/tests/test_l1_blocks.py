import os
import sys
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../../.."))

from test_utils import L1TestUtils

from echonet.l1_logic.l1_blocks import L1Blocks
from echonet.l1_logic.l1_client import L1Client


class TestL1Blocks(unittest.TestCase):
    @patch.object(L1Blocks, "_find_block_range")
    @patch.object(L1Blocks, "_get_latest_block_info")
    def test_find_l1_block_for_tx_success(self, mock_get_latest_block_info, mock_find_block_range):
        mock_client = Mock(spec=L1Client)
        mock_get_latest_block_info.return_value = L1Blocks.BlockInfo(
            L1TestUtils.BLOCK_NUMBER, L1TestUtils.BLOCK_TIMESTAMP
        )
        mock_find_block_range.return_value = (L1TestUtils.BLOCK_NUMBER, L1TestUtils.BLOCK_NUMBER)
        mock_client.get_logs.return_value = L1TestUtils.LOGS_RPC_RESPONSE

        result = L1Blocks.find_l1_block_for_tx(
            L1TestUtils.FEEDER_TX, L1TestUtils.L2_BLOCK_TIMESTAMP, mock_client
        )

        self.assertEqual(result, L1TestUtils.BLOCK_NUMBER)

    @patch.object(L1Blocks, "_find_block_range")
    @patch.object(L1Blocks, "_get_latest_block_info")
    def test_find_l1_block_for_tx_multiple_logs_finds_second(
        self, mock_get_latest_block_info, mock_find_block_range
    ):
        mock_client = Mock(spec=L1Client)
        mock_get_latest_block_info.return_value = L1Blocks.BlockInfo(
            L1TestUtils.BLOCK_NUMBER, L1TestUtils.BLOCK_TIMESTAMP
        )
        mock_find_block_range.return_value = (L1TestUtils.BLOCK_NUMBER, L1TestUtils.BLOCK_NUMBER)
        mock_client.get_logs.return_value = L1TestUtils.logs_rpc_response_with_logs(
            [
                L1TestUtils.log_with_nonce(L1TestUtils.NONCE - 1),
                L1TestUtils.LOG,
            ]
        )

        result = L1Blocks.find_l1_block_for_tx(
            L1TestUtils.FEEDER_TX, L1TestUtils.L2_BLOCK_TIMESTAMP, mock_client
        )

        self.assertEqual(
            result, L1TestUtils.BLOCK_NUMBER
        )  # Should return second log's block number

    @patch.object(L1Blocks, "_find_block_range")
    @patch.object(L1Blocks, "_get_latest_block_info")
    def test_find_l1_block_for_tx_logs_dont_match(
        self, mock_get_latest_block_info, mock_find_block_range
    ):
        mock_client = Mock(spec=L1Client)
        mock_get_latest_block_info.return_value = L1Blocks.BlockInfo(
            L1TestUtils.BLOCK_NUMBER, L1TestUtils.BLOCK_TIMESTAMP
        )
        mock_find_block_range.return_value = (L1TestUtils.BLOCK_NUMBER, L1TestUtils.BLOCK_NUMBER)
        mock_client.get_logs.return_value = L1TestUtils.logs_rpc_response_with_logs(
            [L1TestUtils.log_with_nonce(L1TestUtils.NONCE - 1)]
        )

        result = L1Blocks.find_l1_block_for_tx(
            L1TestUtils.FEEDER_TX, L1TestUtils.L2_BLOCK_TIMESTAMP, mock_client
        )

        self.assertIsNone(result)

    @patch.object(L1Blocks, "_find_block_range")
    @patch.object(L1Blocks, "_get_latest_block_info")
    def test_find_l1_block_for_tx_no_logs_found(
        self, mock_get_latest_block_info, mock_find_block_range
    ):
        mock_client = Mock(spec=L1Client)
        mock_get_latest_block_info.return_value = L1Blocks.BlockInfo(
            L1TestUtils.BLOCK_NUMBER, L1TestUtils.BLOCK_TIMESTAMP
        )
        mock_find_block_range.return_value = (L1TestUtils.BLOCK_NUMBER, L1TestUtils.BLOCK_NUMBER)
        mock_client.get_logs.return_value = L1TestUtils.logs_rpc_response_with_logs([])

        result = L1Blocks.find_l1_block_for_tx(
            L1TestUtils.FEEDER_TX, L1TestUtils.L2_BLOCK_TIMESTAMP, mock_client
        )

        self.assertIsNone(result)

    def test_find_block_range_covers_the_target_across_missed_slots(self):
        mock_client = Mock(spec=L1Client)
        target_timestamp = 1000000
        # Mainnet L1 blocks 26300475-26300477 on 2026-11-03: two missed slots put block 1000 24s
        # before the target and block 1001 12s after it.
        block_timestamps = {
            1000: target_timestamp - 24,
            **{
                block_number: target_timestamp + 12 * (block_number - 1000)
                for block_number in range(1001, 1013)
            },
        }
        mock_client.get_timestamp_of_block.side_effect = block_timestamps.__getitem__
        latest_block = L1Blocks.BlockInfo(number=1012, timestamp=block_timestamps[1012])

        result = L1Blocks._find_block_range(
            mock_client, target_timestamp, target_timestamp + 60, latest_block
        )

        self.assertEqual(result, (1000, 1007))
        mock_client.get_timestamp_of_block.assert_called_once_with(1000)

    def test_find_block_range_bounds_every_window_despite_missed_slots(self):
        mock_client = Mock(spec=L1Client)
        # Every tenth slot is missed, plus a 2-minute outage.
        block_timestamps = dict(
            enumerate(
                1000000 + 12 * slot
                for slot in range(1000)
                if slot % 10 != 7 and not 700 <= slot < 710
            )
        )
        mock_client.get_timestamp_of_block.side_effect = block_timestamps.__getitem__
        latest_block_number = len(block_timestamps) - 1
        latest_block = L1Blocks.BlockInfo(
            number=latest_block_number, timestamp=block_timestamps[latest_block_number]
        )

        for search_start_timestamp in range(1000000 + 12 * 300, latest_block.timestamp, 5):
            search_end_timestamp = search_start_timestamp + 330
            start_block, end_block = L1Blocks._find_block_range(
                mock_client, search_start_timestamp, search_end_timestamp, latest_block
            )

            self.assertLessEqual(block_timestamps[start_block], search_start_timestamp)
            if end_block != latest_block_number:
                self.assertGreaterEqual(block_timestamps[end_block], search_end_timestamp)

    def test_find_block_range_returns_none_when_block_lookup_fails(self):
        mock_client = Mock(spec=L1Client)
        mock_client.get_timestamp_of_block.return_value = None
        latest_block = L1Blocks.BlockInfo(number=1000, timestamp=1000000)

        result = L1Blocks._find_block_range(mock_client, 999000, 999330, latest_block)

        self.assertIsNone(result)

    def test_matches_l1_handler_tx_success(self):
        l1_event = L1TestUtils.L1_EVENT

        feeder_tx = L1TestUtils.FEEDER_TX

        self.assertTrue(L1Blocks.l1_event_matches_feeder_tx(l1_event, feeder_tx))

    def test_matches_l1_handler_tx_mismatches(self):
        l1_event = L1TestUtils.L1_EVENT

        base_feeder_tx = L1TestUtils.FEEDER_TX

        mismatch_cases = [
            ("type", {"type": "INVOKE"}),
            ("contract", {"contract_address": "0x1"}),
            ("selector", {"entry_point_selector": "0x1"}),
            ("nonce", {"nonce": "0x1"}),
            ("calldata", {"calldata": ["0xabc"]}),
        ]

        for field_name, overrides in mismatch_cases:
            with self.subTest(field=field_name):
                # Builds a tx that is valid except for one mismatching field
                feeder_tx = {**base_feeder_tx, **overrides}
                self.assertFalse(L1Blocks.l1_event_matches_feeder_tx(l1_event, feeder_tx))


if __name__ == "__main__":
    unittest.main()
