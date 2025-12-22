import os
import sys
import unittest
from unittest.mock import Mock

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../../.."))

from l1_blocks import L1Blocks
from l1_client import L1Client
from test_utils import L1TestUtils


class TestL1Blocks(unittest.TestCase):
    def test_find_l1_block_for_tx_success(self):
        mock_client = Mock(spec=L1Client)
        mock_client.get_block_number_by_timestamp.side_effect = L1TestUtils.BLOCK_RANGE
        mock_client.get_logs.return_value = L1TestUtils.LOGS_RPC_RESPONSE

        result = L1Blocks.find_l1_block_for_tx(L1TestUtils.FEEDER_TX, 1000, mock_client)

        self.assertEqual(result, L1TestUtils.BLOCK_NUMBER)

    def test_find_l1_block_for_tx_multiple_logs_finds_second(self):
        mock_client = Mock(spec=L1Client)
        mock_client.get_block_number_by_timestamp.side_effect = L1TestUtils.BLOCK_RANGE
        mock_client.get_logs.return_value = L1TestUtils.logs_rpc_response_with_logs(
            [
                L1TestUtils.log_with_nonce(L1TestUtils.NONCE - 1),
                L1TestUtils.LOG,
            ]
        )

        result = L1Blocks.find_l1_block_for_tx(L1TestUtils.FEEDER_TX, 1000, mock_client)

        self.assertEqual(
            result, L1TestUtils.BLOCK_NUMBER
        )  # Should return second log's block number

    def test_find_l1_block_for_tx_logs_dont_match(self):
        mock_client = Mock(spec=L1Client)
        mock_client.get_block_number_by_timestamp.side_effect = L1TestUtils.BLOCK_RANGE
        mock_client.get_logs.return_value = L1TestUtils.logs_rpc_response_with_logs(
            [L1TestUtils.log_with_nonce(L1TestUtils.NONCE - 1)]
        )

        result = L1Blocks.find_l1_block_for_tx(L1TestUtils.FEEDER_TX, 1000, mock_client)

        self.assertIsNone(result)

    def test_find_l1_block_for_tx_no_logs_found(self):
        mock_client = Mock(spec=L1Client)
        mock_client.get_block_number_by_timestamp.side_effect = L1TestUtils.BLOCK_RANGE
        mock_client.get_logs.return_value = L1TestUtils.logs_rpc_response_with_logs([])

        result = L1Blocks.find_l1_block_for_tx(L1TestUtils.FEEDER_TX, 1000, mock_client)

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
