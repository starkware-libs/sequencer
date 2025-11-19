import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import requests
import unittest
from l1_client import L1Client
from unittest.mock import Mock, patch


class TestL1Client(unittest.TestCase):
    BLOCK_NUMBER_SAMPLE = 20_861_344  # 0x13e51a0

    RPC_LOG_RESULT_SAMPLE = {
        "address": "0xc662c410c0ecf747543f5ba90660f6abebd9c8c4",
        "topics": [
            "0xdb80dd488acf86d17c747445b0eabb5d57c541d3bd7b6b87af987858e5066b2b",
            "0x000000000000000000000000023a2aac5d0fa69e3243994672822ba43e34e5c9",
            "0x07c76a71952ce3acd1f953fd2a3fda8564408b821ff367041c89f44526076633",
            "0x02d757788a8d8d6f21d1cd40bce38a8222d70654214e96ff95d8086e684fbee5",
        ],
        "data": (
            "0x0000000000000000000000000000000000000000000000000000000000000060"
            "0000000000000000000000000000000000000000000000000000000000195c23"
            "0000000000000000000000000000000000000000000000000000000000000001"
            "0000000000000000000000000000000000000000000000000000000000000003"
            "001e220c4ac08b2f247d45721e08af1b2d8d65b640cea780534c8f20dc6ea981"
            "000000000000000000000000000000000000000000001c468e3281804cca0000"
            "0000000000000000000000000000000000000000000000000000000000000000"
        ),
        "blockNumber": hex(BLOCK_NUMBER_SAMPLE),
        "blockHash": "0xe090b2c6fbffb35b6e07d5943938384daa59c8c9fefe487d9952ef9894f2483e",
        "transactionHash": "0x66c2ef5ae6708ede5e47daaabfc4b54a53c423160ec27eac06524ea3cd939622",
        "transactionIndex": hex(146),
        "logIndex": hex(749),
        "removed": False,
        "blockTimestamp": hex(1_727_673_743),
    }

    EXPECTED_LOG_SAMPLE = L1Client.Log(
        address=RPC_LOG_RESULT_SAMPLE["address"],
        topics=RPC_LOG_RESULT_SAMPLE["topics"],
        data=RPC_LOG_RESULT_SAMPLE["data"],
        block_number=BLOCK_NUMBER_SAMPLE,
        block_hash=RPC_LOG_RESULT_SAMPLE["blockHash"],
        transaction_hash=RPC_LOG_RESULT_SAMPLE["transactionHash"],
        transaction_index=int(RPC_LOG_RESULT_SAMPLE["transactionIndex"], 16),
        log_index=int(RPC_LOG_RESULT_SAMPLE["logIndex"], 16),
        removed=RPC_LOG_RESULT_SAMPLE["removed"],
        block_timestamp=int(RPC_LOG_RESULT_SAMPLE["blockTimestamp"], 16),
    )

    @patch("l1_client.requests.post")
    def test_get_logs_retries_after_exception_and_succeeds_on_second_attempt(self, mock_post):
        request_exception = requests.RequestException("some error")

        successful_response = Mock()
        successful_response.raise_for_status.return_value = None
        successful_response.json.return_value = {"result": [self.RPC_LOG_RESULT_SAMPLE]}

        mock_post.side_effect = [request_exception, successful_response]

        logs = L1Client.get_logs(
            from_block=self.BLOCK_NUMBER_SAMPLE,
            to_block=self.BLOCK_NUMBER_SAMPLE,
            alchemy_api_key="alchemy_api_key",
        )

        self.assertEqual(mock_post.call_count, 2)
        self.assertEqual(logs, [self.EXPECTED_LOG_SAMPLE])

    def test_get_logs_raises_on_invalid_block_range(self):
        with self.assertRaisesRegex(
            ValueError,
            "from_block must be less than or equal to to_block",
        ):
            L1Client.get_logs(
                from_block=11,
                to_block=10,
                alchemy_api_key="alchemy_api_key",
            )

    @patch("l1_client.requests.post")
    def test_get_logs_parses_several_results(self, mock_post):
        response_ok = Mock()
        response_ok.raise_for_status.return_value = None
        response_ok.json.return_value = {
            "result": [
                {
                    "address": "0x1",
                    "topics": [],
                    "data": "0x01",
                    "blockNumber": "0x1",
                    "blockHash": "0xaa",
                    "transactionHash": "0xa1",
                    "transactionIndex": "0x0",
                    "logIndex": "0x0",
                    "removed": False,
                    "blockTimestamp": "0x10",
                },
                {
                    "address": "0x2",
                    "topics": ["0xdead"],
                    "data": "0x02",
                    "blockNumber": "0x2",
                    "blockHash": "0xbb",
                    "transactionHash": "0xb2",
                    "transactionIndex": "0x1",
                    "logIndex": "0x1",
                    "removed": False,
                    "blockTimestamp": "0x20",
                },
            ]
        }

        mock_post.return_value = response_ok

        logs = L1Client.get_logs(
            from_block=1,
            to_block=2,
            alchemy_api_key="alchemy_api_key",
        )

        self.assertEqual(mock_post.call_count, 1)

        expected_logs = [
            L1Client.Log(
                address="0x1",
                topics=[],
                data="0x01",
                block_number=1,
                block_hash="0xaa",
                transaction_hash="0xa1",
                transaction_index=0,
                log_index=0,
                removed=False,
                block_timestamp=0x10,
            ),
            L1Client.Log(
                address="0x2",
                topics=["0xdead"],
                data="0x02",
                block_number=2,
                block_hash="0xbb",
                transaction_hash="0xb2",
                transaction_index=1,
                log_index=1,
                removed=False,
                block_timestamp=0x20,
            ),
        ]

        self.assertEqual(logs, expected_logs)

    @patch("l1_client.requests.post")
    def test_get_logs_when_rpc_result_is_empty(self, mock_post):
        response_ok = Mock()
        response_ok.raise_for_status.return_value = None
        response_ok.json.return_value = {"result": []}

        mock_post.return_value = response_ok

        logs = L1Client.get_logs(
            from_block=1,
            to_block=1,
            alchemy_api_key="alchemy_api_key",
        )

        self.assertEqual(mock_post.call_count, 1)
        self.assertEqual(logs, [])


if __name__ == "__main__":
    unittest.main()
