import os
import sys

import copy
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from l1_client import L1Client
from l1_events import L1Events


class TestL1Events(unittest.TestCase):
    SAMPLE_LOG = L1Client.Log(
        address="0xc662c410c0ecf747543f5ba90660f6abebd9c8c4",
        topics=[
            "0xdb80dd488acf86d17c747445b0eabb5d57c541d3bd7b6b87af987858e5066b2b",
            "0x000000000000000000000000023a2aac5d0fa69e3243994672822ba43e34e5c9",
            "0x07c76a71952ce3acd1f953fd2a3fda8564408b821ff367041c89f44526076633",
            "0x02d757788a8d8d6f21d1cd40bce38a8222d70654214e96ff95d8086e684fbee5",
        ],
        data="0x0000000000000000000000000000000000000000000000000000000000000060"
        "0000000000000000000000000000000000000000000000000000000000195c23"
        "0000000000000000000000000000000000000000000000000000000000000001"
        "0000000000000000000000000000000000000000000000000000000000000003"
        "001e220c4ac08b2f247d45721e08af1b2d8d65b640cea780534c8f20dc6ea981"
        "000000000000000000000000000000000000000000001c468e3281804cca0000"
        "0000000000000000000000000000000000000000000000000000000000000000",
        block_number=20861344,
        block_hash="0xe090b2c6fbffb35b6e07d5943938384daa59c8c9fefe487d9952ef9894f2483e",
        transaction_hash="0x66c2ef5ae6708ede5e47daaabfc4b54a53c423160ec27eac06524ea3cd939622",
        transaction_index=146,
        log_index=749,
        removed=False,
        block_timestamp=1727673743,
    )

    def test_decode_log_success(self):
        decoded_log_result = L1Events.decode_log(self.SAMPLE_LOG)

        self.assertIsInstance(decoded_log_result, L1Events.DecodedLogMessageToL2)
        self.assertEqual(
            decoded_log_result.from_address, "0x23a2aac5d0fa69e3243994672822ba43e34e5c9"
        )
        self.assertEqual(
            decoded_log_result.to_address,
            "0x7c76a71952ce3acd1f953fd2a3fda8564408b821ff367041c89f44526076633",
        )
        self.assertEqual(
            decoded_log_result.selector,
            0x02D757788A8D8D6F21D1CD40BCE38A8222D70654214E96FF95D8086E684FBEE5,
        )
        self.assertIsInstance(decoded_log_result.payload, list)
        self.assertEqual(len(decoded_log_result.payload), 3)
        self.assertEqual(
            decoded_log_result.payload[0],
            0x001E220C4AC08B2F247D45721E08AF1B2D8D65B640CEA780534C8F20DC6EA981,
        )
        self.assertEqual(
            decoded_log_result.payload[1],
            0x000000000000000000000000000000000000000000001C468E3281804CCA0000,
        )
        self.assertEqual(decoded_log_result.payload[2], 0)
        self.assertEqual(decoded_log_result.nonce, 0x195C23)
        self.assertEqual(decoded_log_result.fee, 1)
        self.assertEqual(
            decoded_log_result.l1_tx_hash,
            "0x66c2ef5ae6708ede5e47daaabfc4b54a53c423160ec27eac06524ea3cd939622",
        )
        self.assertEqual(decoded_log_result.block_timestamp, 0x66FA358F)

    def test_decode_log_invalid_topics_raises_error(self):
        with self.assertRaisesRegex(
            ValueError, "Log has no topics or insufficient topics for LogMessageToL2 event"
        ):
            log = L1Client.Log(
                address="0x0",
                topics=[],
                data="0x",
                block_number=0,
                block_hash="0x0",
                transaction_hash="0x0",
                transaction_index=0,
                log_index=0,
                removed=False,
                block_timestamp=0,
            )
            L1Events.decode_log(log)

    def test_decode_log_wrong_signature_raises_error(self):
        log = copy.deepcopy(self.SAMPLE_LOG)
        log.topics[0] = "0x0000000000000000000000000000000000000000000000000000000000000001"
        with self.assertRaisesRegex(ValueError, "Unhandled event signature"):
            L1Events.decode_log(log)


if __name__ == "__main__":
    unittest.main()
