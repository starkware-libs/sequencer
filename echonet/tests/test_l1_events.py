import os
import sys

import copy
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from l1_events import L1Events
from test_utils import TestUtils


class TestL1Events(unittest.TestCase):
    def test_decode_log_success(self):
        result = L1Events.decode_log(TestUtils.RAW_JSON_LOG)

        self.assertIsInstance(result, L1Events.L1Event)
        self.assertEqual(result, TestUtils.L1_EVENT)

    def test_decode_log_invalid_topics_raises_error(self):
        with self.assertRaisesRegex(
            ValueError, "Log has insufficient topics for LogMessageToL2 event"
        ):
            log = copy.deepcopy(TestUtils.RAW_JSON_LOG)
            log["topics"] = ["0x1", "0x2"]
            L1Events.decode_log(log)

    def test_decode_log_wrong_signature_raises_error(self):
        log = copy.deepcopy(TestUtils.RAW_JSON_LOG)
        log["topics"][0] = "0x0000000000000000000000000000000000000000000000000000000000000001"
        with self.assertRaisesRegex(ValueError, "Unhandled event signature"):
            L1Events.decode_log(log)


if __name__ == "__main__":
    unittest.main()
