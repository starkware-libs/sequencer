import os
import sys

import unittest
from unittest.mock import Mock

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from l1_blocks import L1Blocks
from l1_client import L1Client
from test_utils import L1TestUtils


class TestL1Blocks(unittest.TestCase):
    def mock_log(self, block_number: int, nonce: int) -> dict:
        nonce_hex = f"{nonce:064x}"
        return {
            "blockNumber": hex(block_number),
            "topics": [
                "0xdb80dd488acf86d17c747445b0eabb5d57c541d3bd7b6b87af987858e5066b2b",
                "0x000000000000000000000000f5b6ee2caeb6769659f6c091d209dfdcaf3f69eb",
                "0x0616757a151c21f9be8775098d591c2807316d992bbc3bb1a5c1821630589256",
                "0x01b64b1b3b690b43b9b514fb81377518f4039cd3e4f4914d8a6bdf01d679fb19",
            ],
            "data": (
                "0x0000000000000000000000000000000000000000000000000000000000000060"
                f"{nonce_hex}"
                "00000000000000000000000000000000000000000000000000001308aba4ade2"
                "0000000000000000000000000000000000000000000000000000000000000005"
                "00000000000000000000000004c46e830bb56ce22735d5d8fc9cb90309317d0f"
                "000000000000000000000000c50a951c4426760ba75c5253985a16196b342168"
                "011bf9dbebdd770c31ff13808c96a1cb2de15a240274dc527e7d809bb2bf38df"
                "0000000000000000000000000000000000000000000000956dfdeac59085edc3"
                "0000000000000000000000000000000000000000000000000000000000000000"
            ),
            "transactionHash": (
                "0x66c2ef5ae6708ede5e47daaabfc4b54a53c423160ec27eac06524ea3cd939622"
            ),
            "blockTimestamp": hex(1727673743),
        }

    def test_find_l1_block_for_tx_success(self):
        mock_client = Mock(spec=L1Client)
        mock_client.get_block_number_by_timestamp.side_effect = [100, 200]
        mock_client.get_logs.return_value = [self.mock_log(150, 1684053)]

        result = L1Blocks.find_l1_block_for_tx(L1TestUtils.FEEDER_TX, 1000, mock_client)

        self.assertEqual(result, 150)

    def test_find_l1_block_for_tx_multiple_logs_finds_second(self):
        mock_client = Mock(spec=L1Client)
        mock_client.get_block_number_by_timestamp.side_effect = [100, 200]
        mock_log_non_matching_nonce = self.mock_log(140, 1684054)
        mock_log_matching_nonce = self.mock_log(150, 1684053)
        mock_client.get_logs.return_value = [mock_log_non_matching_nonce, mock_log_matching_nonce]

        result = L1Blocks.find_l1_block_for_tx(L1TestUtils.FEEDER_TX, 1000, mock_client)

        self.assertEqual(result, 150)  # Should return second log's block number

    def test_find_l1_block_for_tx_logs_dont_match(self):
        mock_client = Mock(spec=L1Client)
        mock_client.get_block_number_by_timestamp.side_effect = [100, 200]
        mock_log_non_matching_nonce = self.mock_log(150, 25)  # Different nonce
        mock_client.get_logs.return_value = [mock_log_non_matching_nonce]

        result = L1Blocks.find_l1_block_for_tx(L1TestUtils.FEEDER_TX, 1000, mock_client)

        self.assertIsNone(result)

    def test_find_l1_block_for_tx_no_logs_found(self):
        mock_client = Mock(spec=L1Client)
        mock_client.get_block_number_by_timestamp.side_effect = [100, 200]
        mock_client.get_logs.return_value = []

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
