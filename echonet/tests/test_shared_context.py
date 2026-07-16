import base64
import json
import os
import sys
import tempfile

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../.."))

# `echonet.shared_context` builds an L1Client from `CONFIG` at import time,
# which lazily loads echonet's keys/secrets config from disk. Point those at a
# throwaway fixture with placeholder (non-secret) values, set before the
# import below, so loading the module under test doesn't require the real
# PVC-mounted config this only runs against in a deployed echonet pod.
_config_fixture_dir = tempfile.mkdtemp(prefix="echonet_test_config_")
_config_fixture_a_path = os.path.join(_config_fixture_dir, "test_fixture_config_a.json")
_config_fixture_b_path = os.path.join(_config_fixture_dir, "test_fixture_config_b.json")
with open(_config_fixture_a_path, "w") as _fixture_file:
    json.dump({"start_block": 1}, _fixture_file)
with open(_config_fixture_b_path, "w") as _fixture_file:
    json.dump({"l1_events_provider_api_key": "test-fixture-placeholder"}, _fixture_file)
os.environ.setdefault("ECHONET_KEYS_PATH", _config_fixture_a_path)
os.environ.setdefault("ECHONET_SECRETS_PATH", _config_fixture_b_path)

import unittest
from unittest.mock import patch

import zstandard

from echonet.os_input_builder import decompress_state_commitment_infos
from echonet.shared_context import SharedContext


def _compress_without_content_size(payload: bytes) -> bytes:
    """Mirror the committer's streaming (content-size-omitted) zstd frame."""
    compressor = zstandard.ZstdCompressor().compressobj(size=zstandard.CONTENTSIZE_UNKNOWN)
    return compressor.compress(payload) + compressor.flush()


def _commit_entry(block_number: int) -> dict:
    state_commitment_infos = {"contracts_trie_commitment_info": {"block_number": block_number}}
    compressed = _compress_without_content_size(json.dumps(state_commitment_infos).encode())
    return {
        "block_number": block_number,
        "state_commitment_infos": base64.b64encode(compressed).decode(),
    }


class TestRecordCommitsFromBlob(unittest.TestCase):
    def test_stores_every_entry_within_the_cap(self):
        shared = SharedContext()
        entries = [_commit_entry(block_number) for block_number in range(1, 6)]

        shared.record_commits_from_blob({"recent_state_commitment_infos": entries})

        for block_number in range(1, 6):
            self.assertIsNotNone(shared.get_state_commitment_infos(block_number))

    def test_caps_the_number_of_entries_decompressed_per_call(self):
        """
        `recent_state_commitment_infos`'s length comes straight from an
        external WRITE_BLOB request body; an oversized vector must not force
        unbounded decompression work in a single call. The `_COMMITS_RETENTION`
        trim at the end of `record_commits_from_blob` is a separate,
        pre-existing bound on final storage size that would mask an
        unenforced cap here, so assert directly on how many entries get
        decompressed rather than on what ends up stored.
        """
        shared = SharedContext()
        num_entries = SharedContext._MAX_COMMIT_ENTRIES_PER_BLOB + 10
        entries = [_commit_entry(block_number) for block_number in range(1, num_entries + 1)]

        with patch(
            "echonet.shared_context.decompress_state_commitment_infos",
            wraps=decompress_state_commitment_infos,
        ) as mock_decompress, self.assertLogs("echonet.shared_context", level="WARNING"):
            shared.record_commits_from_blob({"recent_state_commitment_infos": entries})

        self.assertEqual(mock_decompress.call_count, SharedContext._MAX_COMMIT_ENTRIES_PER_BLOB)


if __name__ == "__main__":
    unittest.main()
