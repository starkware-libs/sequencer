import os
import sys

import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from l1_events import (
    DecodedLogMessageToL2,
    L1Event,
    L1HandlerTransaction,
    decode_log,
    parse_event,
)


class TestL1Events(unittest.TestCase):
    SAMPLE_LOG = {
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
        "blockNumber": "0x13e51a0",
        "blockHash": "0xe090b2c6fbffb35b6e07d5943938384daa59c8c9fefe487d9952ef9894f2483e",
        "transactionHash": "0x66c2ef5ae6708ede5e47daaabfc4b54a53c423160ec27eac06524ea3cd939622",
        "transactionIndex": "0x92",
        "logIndex": "0x2ed",
        "removed": False,
        "blockTimestamp": "0x66fa358f",
    }

    def test_decode_log_success(self):
        result = decode_log(self.SAMPLE_LOG)

        self.assertIsInstance(result, DecodedLogMessageToL2)
        self.assertEqual(result.from_address, "0x023a2aac5d0fa69e3243994672822ba43e34e5c9")
        self.assertEqual(
            result.to_address, 0x07C76A71952CE3ACD1F953FD2A3FDA8564408B821FF367041C89F44526076633
        )
        self.assertEqual(
            result.selector, 0x02D757788A8D8D6F21D1CD40BCE38A8222D70654214E96FF95D8086E684FBEE5
        )
        self.assertIsInstance(result.payload, list)
        self.assertEqual(len(result.payload), 3)
        self.assertEqual(
            result.payload[0], 0x001E220C4AC08B2F247D45721E08AF1B2D8D65B640CEA780534C8F20DC6EA981
        )
        self.assertEqual(
            result.payload[1], 0x000000000000000000000000000000000000000000001C468E3281804CCA0000
        )
        self.assertEqual(result.payload[2], 0)
        self.assertEqual(result.nonce, 0x195C23)
        self.assertEqual(result.fee, 1)
        self.assertEqual(
            result.l1_tx_hash, "0x66c2ef5ae6708ede5e47daaabfc4b54a53c423160ec27eac06524ea3cd939622"
        )
        self.assertEqual(result.block_timestamp, 0x66FA358F)

    def test_decode_log_invalid_topics_raises_error(self):
        with self.assertRaisesRegex(ValueError, "Log has no topics"):
            decode_log({"topics": [], "data": "0x"})

        with self.assertRaisesRegex(ValueError, "Log has no topics"):
            decode_log({"data": "0x"})

    def test_decode_log_wrong_signature_raises_error(self):
        log = {
            "topics": ["0x0000000000000000000000000000000000000000000000000000000000000001"],
            "data": "0x00",
        }
        with self.assertRaisesRegex(ValueError, "Unhandled event signature"):
            decode_log(log)

    def test_parse_event_success(self):
        result = parse_event(self.SAMPLE_LOG)

        self.assertIsInstance(result, L1Event)
        self.assertIsInstance(result.tx, L1HandlerTransaction)
        self.assertEqual(
            result.tx.contract_address,
            0x07C76A71952CE3ACD1F953FD2A3FDA8564408B821FF367041C89F44526076633,
        )
        self.assertEqual(
            result.tx.entry_point_selector,
            0x02D757788A8D8D6F21D1CD40BCE38A8222D70654214E96FF95D8086E684FBEE5,
        )
        self.assertEqual(
            result.tx.calldata[0],
            0x001E220C4AC08B2F247D45721E08AF1B2D8D65B640CEA780534C8F20DC6EA981,
        )
        self.assertEqual(result.tx.nonce, 0x195C23)
        self.assertEqual(result.fee, 1)
        self.assertEqual(
            result.l1_tx_hash, "0x66c2ef5ae6708ede5e47daaabfc4b54a53c423160ec27eac06524ea3cd939622"
        )
        self.assertEqual(result.block_timestamp, 1727673743)


if __name__ == "__main__":
    unittest.main()
