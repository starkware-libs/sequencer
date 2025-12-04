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

    def test_matches_l1_handler_tx_success(self):
        l1_event = TestUtils.L1_EVENT

        feeder_tx = TestUtils.FEEDER_TX

        self.assertTrue(L1Events.l1_event_matches_feeder_tx(l1_event, feeder_tx))

    def test_matches_l1_handler_tx_mismatches(self):
        l1_event = TestUtils.L1_EVENT

        base_feeder_tx = TestUtils.FEEDER_TX

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
                self.assertFalse(L1Events.l1_event_matches_feeder_tx(l1_event, feeder_tx))


if __name__ == "__main__":
    unittest.main()
