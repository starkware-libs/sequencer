import os
import sys

import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from l1_events import (
    DecodedLogMessageToL2,
    L1Event,
    L1HandlerTransaction,
    decode_log,
    l1_event_matches_feeder_tx,
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

    L1_EVENT_SAMPLE = L1Event(
        tx=L1HandlerTransaction(
            contract_address=0x0616757A151C21F9BE8775098D591C2807316D992BBC3BB1A5C1821630589256,
            entry_point_selector=0x1B64B1B3B690B43B9B514FB81377518F4039CD3E4F4914D8A6BDF01D679FB19,
            calldata=[
                0xF5B6EE2CAEB6769659F6C091D209DFDCAF3F69EB,
                0x04C46E830BB56CE22735D5D8FC9CB90309317D0F,
                0x5B8EDF4EEC1A29B14E41DBC63B261F4D674809A3,
                0x0208A182D168512C591D596F0010FA13F4B09EE2D43EA70F346731C5CF175AB7,
                0x3635C9ADC5DEA00000,
                0x0,
            ],
            nonce=0x19AB82,
        ),
        fee=1,
        l1_tx_hash="0xabcd1234",
        block_timestamp=1234567890,
    )

    FEEDER_TX_SAMPLE = {
        "transaction_hash": "0x180902ad386a23000f981f6ffac17312d409c75f4e56e3eb9bf5a446f42e7e4",
        "version": "0x0",
        "contract_address": "0x616757a151c21f9be8775098d591c2807316d992bbc3bb1a5c1821630589256",
        "entry_point_selector": "0x1b64b1b3b690b43b9b514fb81377518f4039cd3e4f4914d8a6bdf01d679fb19",
        "nonce": "0x19ab82",
        "calldata": [
            "0xf5b6ee2caeb6769659f6c091d209dfdcaf3f69eb",
            "0x4c46e830bb56ce22735d5d8fc9cb90309317d0f",
            "0x5b8edf4eec1a29b14e41dbc63b261f4d674809a3",
            "0x208a182d168512c591d596f0010fa13f4b09ee2d43ea70f346731c5cf175ab7",
            "0x3635c9adc5dea00000",
            "0x0",
        ],
        "type": "L1_HANDLER",
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

    def test_matches_l1_handler_tx_success(self):
        l1_event = self.L1_EVENT_SAMPLE

        feeder_tx = self.FEEDER_TX_SAMPLE

        self.assertTrue(l1_event_matches_feeder_tx(l1_event, feeder_tx))

    def test_matches_l1_handler_tx_mismatches(self):
        l1_event = self.L1_EVENT_SAMPLE

        base_feeder_tx = self.FEEDER_TX_SAMPLE

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
                self.assertFalse(l1_event_matches_feeder_tx(l1_event, feeder_tx))


if __name__ == "__main__":
    unittest.main()
