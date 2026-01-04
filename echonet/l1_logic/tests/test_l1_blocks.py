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
    @patch.object(L1Blocks, "_find_block_near_timestamp")
    @patch.object(L1Blocks, "_get_latest_block_info")
    def test_find_l1_block_for_tx_success(
        self, mock_get_latest_block_info, mock_find_block_near_timestamp
    ):
        mock_client = Mock(spec=L1Client)
        mock_get_latest_block_info.return_value = L1Blocks.BlockInfo(
            L1TestUtils.BLOCK_NUMBER, L1TestUtils.BLOCK_TIMESTAMP
        )
        mock_find_block_near_timestamp.return_value = L1TestUtils.BLOCK_NUMBER
        mock_client.get_logs.return_value = L1TestUtils.LOGS_RPC_RESPONSE

        result = L1Blocks.find_l1_block_for_tx(
            L1TestUtils.FEEDER_TX, L1TestUtils.L2_BLOCK_TIMESTAMP, mock_client
        )

        self.assertEqual(result, L1TestUtils.BLOCK_NUMBER)

    @patch.object(L1Blocks, "_find_block_near_timestamp")
    @patch.object(L1Blocks, "_get_latest_block_info")
    def test_find_l1_block_for_tx_multiple_logs_finds_second(
        self, mock_get_latest_block_info, mock_find_block_near_timestamp
    ):
        mock_client = Mock(spec=L1Client)
        mock_get_latest_block_info.return_value = L1Blocks.BlockInfo(
            L1TestUtils.BLOCK_NUMBER, L1TestUtils.BLOCK_TIMESTAMP
        )
        mock_find_block_near_timestamp.return_value = L1TestUtils.BLOCK_NUMBER
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

    @patch.object(L1Blocks, "_find_block_near_timestamp")
    @patch.object(L1Blocks, "_get_latest_block_info")
    def test_find_l1_block_for_tx_logs_dont_match(
        self, mock_get_latest_block_info, mock_find_block_near_timestamp
    ):
        mock_client = Mock(spec=L1Client)
        mock_get_latest_block_info.return_value = L1Blocks.BlockInfo(
            L1TestUtils.BLOCK_NUMBER, L1TestUtils.BLOCK_TIMESTAMP
        )
        mock_find_block_near_timestamp.return_value = L1TestUtils.BLOCK_NUMBER
        mock_client.get_logs.return_value = L1TestUtils.logs_rpc_response_with_logs(
            [L1TestUtils.log_with_nonce(L1TestUtils.NONCE - 1)]
        )

        result = L1Blocks.find_l1_block_for_tx(
            L1TestUtils.FEEDER_TX, L1TestUtils.L2_BLOCK_TIMESTAMP, mock_client
        )

        self.assertIsNone(result)

    @patch.object(L1Blocks, "_find_block_near_timestamp")
    @patch.object(L1Blocks, "_get_latest_block_info")
    def test_find_l1_block_for_tx_no_logs_found(
        self, mock_get_latest_block_info, mock_find_block_near_timestamp
    ):
        mock_client = Mock(spec=L1Client)
        mock_get_latest_block_info.return_value = L1Blocks.BlockInfo(
            L1TestUtils.BLOCK_NUMBER, L1TestUtils.BLOCK_TIMESTAMP
        )
        mock_find_block_near_timestamp.return_value = L1TestUtils.BLOCK_NUMBER
        mock_client.get_logs.return_value = L1TestUtils.logs_rpc_response_with_logs([])

        result = L1Blocks.find_l1_block_for_tx(
            L1TestUtils.FEEDER_TX, L1TestUtils.L2_BLOCK_TIMESTAMP, mock_client
        )

        self.assertIsNone(result)

    def test_find_block_near_timestamp_finds_on_first_iteration_when_estimation_is_accurate(self):
        mock_client = Mock(spec=L1Client)
        target_timestamp = 1000000
        expected_block_number = 1000

        # Set reference block with number and timestamp, one minute ahead, 5 blocks higher.
        reference_block = L1Blocks.BlockInfo(
            number=expected_block_number + 5, timestamp=target_timestamp + 60
        )

        # First estimate: block 1005 - (60 // 12) = block 1000
        # Mock returns timestamp within tolerance (diff = 2s < 20s)
        mock_client.get_timestamp_of_block.return_value = target_timestamp - 2

        result = L1Blocks._find_block_near_timestamp(mock_client, target_timestamp, reference_block)

        self.assertEqual(result, expected_block_number)
        mock_client.get_timestamp_of_block.assert_called_once_with(expected_block_number)

    def test_find_block_near_timestamp_finds_on_fourth_iteration(self):
        mock_client = Mock(spec=L1Client)
        target_timestamp = 1000000
        expected_block_number = 986

        # Set reference block with number and timestamp, 2 minutes ahead.
        reference_block = L1Blocks.BlockInfo(number=1000, timestamp=target_timestamp + 120)

        mock_client.get_timestamp_of_block.side_effect = [
            target_timestamp
            + 60,  # Iteration 1: Check block 990. 60s too new -> adjust 5 blocks backward -> 985.
            target_timestamp
            - 36,  # Iteration 2: Check block 985. 36s too old -> adjust 3 blocks forward -> 988.
            target_timestamp
            + 24,  # Iteration 3: Check block 988. 24s too new -> adjust 2 blocks backward -> 986.
            target_timestamp
            + 5,  # Iteration 4: Check block 986. 5s too new -> within 20s tolerance -> Return 986.
        ]

        result = L1Blocks._find_block_near_timestamp(mock_client, target_timestamp, reference_block)

        self.assertEqual(result, expected_block_number)
        self.assertEqual(mock_client.get_timestamp_of_block.call_count, 4)

    def test_find_block_near_timestamp_max_iterations_returns_none(self):
        mock_client = Mock(spec=L1Client)
        target_timestamp = 1000000
        reference_block = L1Blocks.BlockInfo(number=1000, timestamp=target_timestamp + 120)

        # Always return timestamp outside tolerance (25s > 20s) for all 10 iterations.
        mock_client.get_timestamp_of_block.return_value = target_timestamp + 25

        result = L1Blocks._find_block_near_timestamp(mock_client, target_timestamp, reference_block)

        self.assertIsNone(result)
        self.assertEqual(
            mock_client.get_timestamp_of_block.call_count, L1Blocks._MAX_BLOCK_SEARCH_ITERATIONS
        )

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
