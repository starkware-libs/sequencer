import copy
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import unittest
from unittest.mock import Mock, patch

import requests
from l1_client import L1Client
from test_utils import L1TestUtils


class TestL1Client(unittest.TestCase):
    @patch("l1_client.requests.post")
    def test_get_logs_retries_after_exception_and_succeeds_on_second_attempt(self, mock_post):
        request_exception = requests.RequestException("some error")

        successful_response = Mock()
        successful_response.raise_for_status.return_value = None
        successful_response.json.return_value = L1TestUtils.LOGS_RPC_RESPONSE
        mock_post.side_effect = [request_exception, successful_response]

        client = L1Client(api_key="api_key")
        logs = client.get_logs(
            from_block=L1TestUtils.BLOCK_NUMBER,
            to_block=L1TestUtils.BLOCK_NUMBER,
        )

        self.assertEqual(mock_post.call_count, 2)
        self.assertEqual(logs, L1TestUtils.LOGS_RPC_RESPONSE)

    def test_get_logs_raises_on_invalid_block_range(self):
        client = L1Client(api_key="api_key")
        with self.assertRaisesRegex(
            ValueError,
            "from_block must be less than or equal to to_block",
        ):
            client.get_logs(
                from_block=11,
                to_block=10,
            )

    @patch("l1_client.requests.post")
    def test_get_logs_when_rpc_result_is_empty(self, mock_post):
        empty_response = L1TestUtils.logs_rpc_response_with_logs([])
        response_ok = Mock()
        response_ok.raise_for_status.return_value = None
        response_ok.json.return_value = empty_response

        mock_post.return_value = response_ok

        client = L1Client(api_key="api_key")
        logs = client.get_logs(from_block=1, to_block=1)

        self.assertEqual(mock_post.call_count, 1)
        self.assertEqual(logs, empty_response)

    @patch("l1_client.requests.post")
    def test_get_block_by_number_retries_after_failure_and_succeeds(self, mock_post):
        request_exception = requests.RequestException("some error")

        successful_response = Mock()
        successful_response.raise_for_status.return_value = None
        successful_response.json.return_value = L1TestUtils.BLOCK_RPC_RESPONSE

        mock_post.side_effect = [request_exception, successful_response]

        client = L1Client(api_key="api_key")
        result = client.get_block_by_number(L1TestUtils.BLOCK_NUMBER_HEX)

        self.assertEqual(mock_post.call_count, 2)
        self.assertEqual(result, L1TestUtils.BLOCK_RPC_RESPONSE)

    @patch("l1_client.requests.post")
    def test_get_block_by_number_returns_none_when_rpc_result_is_empty(self, mock_post):
        empty_response = L1TestUtils.block_rpc_response_with_block(None)
        response_ok = Mock()
        response_ok.raise_for_status.return_value = None
        response_ok.json.return_value = empty_response

        mock_post.return_value = response_ok

        client = L1Client(api_key="api_key")
        result = client.get_block_by_number(block_number=L1TestUtils.BLOCK_NUMBER_HEX)

        self.assertEqual(mock_post.call_count, 1)
        self.assertEqual(result, empty_response)

    @patch.object(L1Client, "get_block_by_number")
    def test_get_timestamp_of_block_returns_int_timestamp(self, mock_get_block_by_number):
        mock_get_block_by_number.return_value = L1TestUtils.BLOCK_RPC_RESPONSE

        client = L1Client(api_key="api_key")
        result = client.get_timestamp_of_block(L1TestUtils.BLOCK_NUMBER_HEX)

        self.assertEqual(result, L1TestUtils.BLOCK_TIMESTAMP)
        mock_get_block_by_number.assert_called_once_with(L1TestUtils.BLOCK_NUMBER_HEX)

    @patch.object(L1Client, "get_block_by_number")
    def test_get_timestamp_of_block_returns_none_when_block_not_found(
        self, mock_get_block_by_number
    ):
        mock_get_block_by_number.return_value = None

        client = L1Client(api_key="api_key")
        result = client.get_timestamp_of_block(block_number=L1TestUtils.BLOCK_NUMBER_HEX)

        self.assertIsNone(result)

    @patch.object(L1Client, "get_block_by_number")
    def test_get_timestamp_of_block_returns_none_when_result_is_none(
        self, mock_get_block_by_number
    ):
        mock_get_block_by_number.return_value = L1TestUtils.block_rpc_response_with_block(None)

        client = L1Client(api_key="api_key")
        result = client.get_timestamp_of_block(block_number=L1TestUtils.BLOCK_NUMBER_HEX)

        self.assertIsNone(result)

    def test_decode_log_success(self):
        result = L1Client.decode_log_response(L1TestUtils.LOG)

        self.assertIsInstance(result, L1Client.L1Event)
        self.assertEqual(result, L1TestUtils.L1_EVENT)

    def test_decode_log_invalid_topics_raises_error(self):
        log = copy.deepcopy(L1TestUtils.LOG)
        log["topics"] = ["0x1", "0x2"]
        with self.assertRaisesRegex(
            ValueError, "Log has insufficient topics for LogMessageToL2 event"
        ):
            L1Client.decode_log_response(log)

    def test_decode_log_wrong_signature_raises_error(self):
        log = copy.deepcopy(L1TestUtils.LOG)
        log["topics"][0] = "0x0000000000000000000000000000000000000000000000000000000000000001"
        with self.assertRaisesRegex(ValueError, "Unhandled event signature"):
            L1Client.decode_log_response(log)


if __name__ == "__main__":
    unittest.main()
