import os
import sys

import unittest
from unittest.mock import Mock

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from l1_block_finder import find_l1_block_for_tx
from l1_client import L1Client


class TestFindL1BlockForTx(unittest.TestCase):
    FEEDER_TX_SAMPLE = {
        "transaction_hash": "0x83c298ad90f4d1b35c0a324fa162a3ab3d3d3a4dcc046f0965bd045083a472",
        "version": "0x0",
        "contract_address": "0x616757a151c21f9be8775098d591c2807316d992bbc3bb1a5c1821630589256",
        "entry_point_selector": "0x1b64b1b3b690b43b9b514fb81377518f4039cd3e4f4914d8a6bdf01d679fb19",
        "nonce": "0x19b255",
        "calldata": [
            "0xf5b6ee2caeb6769659f6c091d209dfdcaf3f69eb",
            "0x4c46e830bb56ce22735d5d8fc9cb90309317d0f",
            "0xc50a951c4426760ba75c5253985a16196b342168",
            "0x11bf9dbebdd770c31ff13808c96a1cb2de15a240274dc527e7d809bb2bf38df",
            "0x956dfdeac59085edc3",
            "0x0",
        ],
        "type": "L1_HANDLER",
    }

    def mock_log(self, block_number: int, nonce: int) -> Mock:
        mock_log = Mock()
        mock_log.block_number = block_number
        mock_log.topics = [
            "0xdb80dd488acf86d17c747445b0eabb5d57c541d3bd7b6b87af987858e5066b2b",
            "0x000000000000000000000000f5b6ee2caeb6769659f6c091d209dfdcaf3f69eb",
            "0x0616757a151c21f9be8775098d591c2807316d992bbc3bb1a5c1821630589256",
            "0x01b64b1b3b690b43b9b514fb81377518f4039cd3e4f4914d8a6bdf01d679fb19",
        ]
        nonce_hex = f"{nonce:064x}"
        mock_log.data = (
            "0x0000000000000000000000000000000000000000000000000000000000000060"
            f"{nonce_hex}"
            "00000000000000000000000000000000000000000000000000001308aba4ade2"
            "0000000000000000000000000000000000000000000000000000000000000005"
            "00000000000000000000000004c46e830bb56ce22735d5d8fc9cb90309317d0f"
            "000000000000000000000000c50a951c4426760ba75c5253985a16196b342168"
            "011bf9dbebdd770c31ff13808c96a1cb2de15a240274dc527e7d809bb2bf38df"
            "0000000000000000000000000000000000000000000000956dfdeac59085edc3"
            "0000000000000000000000000000000000000000000000000000000000000000"
        )
        mock_log.transaction_hash = (
            "0x66c2ef5ae6708ede5e47daaabfc4b54a53c423160ec27eac06524ea3cd939622"
        )
        mock_log.block_timestamp = 1727673743
        return mock_log

    def test_find_l1_block_for_tx_success(self):
        mock_client = Mock(spec=L1Client)
        mock_client.get_block_number_by_timestamp.side_effect = [100, 200]
        mock_client.get_logs.return_value = [self.mock_log(150, 1684053)]

        result = find_l1_block_for_tx(self.FEEDER_TX_SAMPLE, 1000, mock_client)

        self.assertEqual(result, 150)

    def test_find_l1_block_for_tx_multiple_logs_finds_second(self):
        mock_client = Mock(spec=L1Client)
        mock_client.get_block_number_by_timestamp.side_effect = [100, 200]
        mock_log_non_matching_nonce = self.mock_log(140, 1684054)
        mock_log_matching_nonce = self.mock_log(150, 1684053)
        mock_client.get_logs.return_value = [mock_log_non_matching_nonce, mock_log_matching_nonce]

        result = find_l1_block_for_tx(self.FEEDER_TX_SAMPLE, 1000, mock_client)

        self.assertEqual(result, 150)  # Should return second log's block number

    def test_find_l1_block_for_tx_logs_dont_match(self):
        mock_client = Mock(spec=L1Client)
        mock_client.get_block_number_by_timestamp.side_effect = [100, 200]
        mock_log_non_matching_nonce = self.mock_log(150, 25)  # Different nonce
        mock_client.get_logs.return_value = [mock_log_non_matching_nonce]

        result = find_l1_block_for_tx(self.FEEDER_TX_SAMPLE, 1000, mock_client)

        self.assertIsNone(result)

    def test_find_l1_block_for_tx_no_logs_found(self):
        mock_client = Mock(spec=L1Client)
        mock_client.get_block_number_by_timestamp.side_effect = [100, 200]
        mock_client.get_logs.return_value = []

        result = find_l1_block_for_tx(self.FEEDER_TX_SAMPLE, 1000, mock_client)

        self.assertIsNone(result)


if __name__ == "__main__":
    unittest.main()
