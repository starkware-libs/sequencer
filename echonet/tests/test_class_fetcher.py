import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../.."))

from echonet.class_fetcher import (
    ClassFetchError,
    _read_cache,
    _write_cache,
    resolve_classes_for_os,
)

# `class_hash`/`compiled_class_hash` values come from the replayed block's `initial_reads`
# and are used unsanitized as `_read_cache`/`_write_cache` filename components. A key of
# `../..` segments escapes `cairo0_dir` (two levels below `outside_dir`'s parent) straight
# into `outside_dir`, reaching files well outside the intended cache tree.
_TRAVERSAL_KEY = "../../outside/leaked"


class TestClassFetcherPathTraversal(unittest.TestCase):
    def setUp(self):
        self._tmp_dir = tempfile.TemporaryDirectory()
        tmp_root = Path(self._tmp_dir.name)
        self.cache_root = tmp_root / "cache"
        self.cairo0_dir = self.cache_root / "cairo0"
        self.cairo0_dir.mkdir(parents=True)
        self.outside_dir = tmp_root / "outside"
        self.outside_dir.mkdir(parents=True)

    def tearDown(self):
        self._tmp_dir.cleanup()

    def test_read_cache_rejects_path_traversal_key(self):
        # Plant a real secret at the traversal target: unpatched, `_read_cache` returns
        # its content instead of raising, leaking a file from outside the cache directory.
        planted_secret = self.outside_dir / "leaked.json"
        planted_secret.write_text('{"secret": "leaked"}')

        with self.assertRaises(ClassFetchError):
            _read_cache(self.cairo0_dir, _TRAVERSAL_KEY)

    def test_write_cache_rejects_path_traversal_key(self):
        with self.assertRaises(ClassFetchError):
            _write_cache(self.cairo0_dir, _TRAVERSAL_KEY, {"pwned": True})

        self.assertEqual(
            list(self.outside_dir.iterdir()),
            [],
            "cache write escaped the cache directory via path traversal",
        )

    def test_cache_roundtrip_accepts_hex_felt_keys(self):
        # The guard must not be stricter than the real `0x`-hex felt shape: legitimate
        # class hashes (full-length and short) must still write and read back unchanged.
        value = {"program": "not_sierra"}
        for class_hash in (
            "0x1b64b1b3b690b43b9b514fb81377518f4039cd3e4f4914d8a6bdf01d679fb19",
            "0x123",
            "0x1",
        ):
            _write_cache(self.cairo0_dir, class_hash, value)
            self.assertEqual(_read_cache(self.cairo0_dir, class_hash), value)

    def test_resolve_classes_for_os_rejects_path_traversal_class_hash(self):
        # A malicious/corrupt replayed block smuggles a traversal payload as a
        # `class_hashes` (address -> class_hash) value; this reaches the cairo0
        # cache path unsanitized.
        blob = {
            "initial_reads": {
                "compiled_class_hashes": {},
                "class_hashes": {"0xdeadbeef": _TRAVERSAL_KEY},
            },
            "compiled_classes": [],
        }

        with patch(
            "echonet.class_fetcher._fetch_one_deprecated",
            return_value={"program": "not_sierra"},
        ) as mock_fetch:
            with self.assertRaises(ClassFetchError):
                resolve_classes_for_os(blob, cache_root=self.cache_root)
            mock_fetch.assert_not_called()

        self.assertEqual(
            list(self.outside_dir.iterdir()),
            [],
            "resolve_classes_for_os escaped the cache directory via path traversal",
        )


if __name__ == "__main__":
    unittest.main()
