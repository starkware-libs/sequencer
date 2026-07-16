import base64
import json
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../.."))

import unittest

import zstandard

from echonet.os_input_builder import OsInputBuildError, decompress_state_commitment_infos


def _compress_without_content_size(payload: bytes) -> bytes:
    """
    Compress `payload` the way the committer's Rust encoder does: as a
    streaming frame that omits the pledged content size (the reason
    `decompress_state_commitment_infos` needs a streaming reader instead of
    the one-shot `zstandard.decompress()`).
    """
    compressor = zstandard.ZstdCompressor().compressobj(size=zstandard.CONTENTSIZE_UNKNOWN)
    return compressor.compress(payload) + compressor.flush()


class TestDecompressStateCommitmentInfos(unittest.TestCase):
    def test_round_trips_a_well_formed_payload(self):
        state_commitment_infos = {"contracts_trie_commitment_info": {"some": "value"}}
        compressed = _compress_without_content_size(json.dumps(state_commitment_infos).encode())
        encoded = base64.b64encode(compressed).decode()

        result = decompress_state_commitment_infos(encoded)

        self.assertEqual(result, state_commitment_infos)

    def test_rejects_decompression_bomb_exceeding_the_configured_cap(self):
        # A few KB of highly-compressible data zstd-compresses to a tiny
        # fraction of its decompressed size; a real WRITE_BLOB request body
        # could smuggle in a payload that expands to gigabytes.
        bomb_payload = json.dumps({"x": "0" * (2 * 1024 * 1024)}).encode()
        compressed = _compress_without_content_size(bomb_payload)
        encoded = base64.b64encode(compressed).decode()
        self.assertLess(len(compressed), len(bomb_payload) // 100)

        with self.assertRaisesRegex(OsInputBuildError, "decompression bomb"):
            decompress_state_commitment_infos(encoded, max_decompressed_bytes=1024 * 1024)

    def test_accepts_payload_within_the_configured_cap(self):
        payload = json.dumps({"x": "a" * 1000}).encode()
        compressed = _compress_without_content_size(payload)
        encoded = base64.b64encode(compressed).decode()

        result = decompress_state_commitment_infos(encoded, max_decompressed_bytes=1024 * 1024)

        self.assertEqual(result, json.loads(payload))

    def test_raises_os_input_build_error_on_invalid_base64(self):
        with self.assertRaisesRegex(OsInputBuildError, "failed to decode"):
            decompress_state_commitment_infos("not valid base64!!!")


if __name__ == "__main__":
    unittest.main()
