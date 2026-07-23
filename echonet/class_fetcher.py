"""
Resolve the CASM (and deprecated v0) classes the OS needs to run a block.

The cende blob only carries the block's *newly declared* classes, but the OS
executes against every class the block touches. The rest are fetched from the
mainnet feeder gateway and cached on the PVC. The fetch list comes from
`initial_reads`: `compiled_class_hashes` drives the Cairo 1 CASM fetches, and
accessed class hashes absent from it are Cairo 0.
"""

from __future__ import annotations

import json
import os
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Any, Callable, Dict, Iterable, List, Mapping, Set, Tuple

from echonet.echonet_types import CONFIG, JsonObject
from echonet.logger import get_logger

logger = get_logger("echonet.class_fetcher")

_CASM_CACHE_SUBDIR = "cairo1"
_DEPRECATED_CACHE_SUBDIR = "cairo0"
_FETCH_PARALLELISM = 8
_FETCH_TIMEOUT_SECONDS = 30
_ZERO_CLASS_HASH = "0x0"


def _is_sierra_shape(class_json: JsonObject) -> bool:
    """The permissive v0 endpoint may return a Sierra body; only Sierra has `sierra_program`."""
    return "sierra_program" in class_json


def _is_zero_felt(felt_hex: str) -> bool:
    return int(felt_hex, 16) == 0


class ClassFetchError(RuntimeError):
    """Raised when a required class cannot be fetched from the feeder."""


def resolve_classes_for_os(
    blob: JsonObject, *, cache_root: Path
) -> Tuple[Dict[str, JsonObject], Dict[str, JsonObject], int, int]:
    """
    Resolve all classes the OS will execute when replaying this block.

    Returns `(compiled_classes, deprecated_compiled_classes, fetched_count,
    cached_count)`; the blob's newly declared classes are merged in as-is.
    """
    initial_reads = blob.get("initial_reads", {})
    compiled_class_hashes: Mapping[str, str] = initial_reads.get("compiled_class_hashes", {})
    address_to_class_hash: Mapping[str, str] = initial_reads.get("class_hashes", {})

    # A zero compiled_class_hash is the StateReader's sentinel for a Cairo 0
    # class — route those to the v0 endpoint, not the CASM endpoint.
    cairo1_compiled_class_hashes: Dict[str, str] = {
        class_hash: compiled_class_hash
        for class_hash, compiled_class_hash in compiled_class_hashes.items()
        if not _is_zero_felt(compiled_class_hash)
    }
    cairo0_from_sentinels: Set[str] = {
        class_hash
        for class_hash, compiled_class_hash in compiled_class_hashes.items()
        if _is_zero_felt(compiled_class_hash)
    }
    cairo1_class_hashes: Set[str] = set(cairo1_compiled_class_hashes.keys())
    cairo0_class_hashes: Set[str] = cairo0_from_sentinels | {
        class_hash
        for class_hash in address_to_class_hash.values()
        if class_hash != _ZERO_CLASS_HASH and class_hash not in cairo1_class_hashes
    }

    blob_compiled: Dict[str, JsonObject] = {}
    for entry in blob.get("compiled_classes", []):
        compiled_class_hash = entry[0]
        casm = entry[1].get("compiled_class") if isinstance(entry[1], dict) else None
        if casm is None:
            raise ClassFetchError(
                f"blob compiled_classes entry for {compiled_class_hash} missing 'compiled_class'"
            )
        blob_compiled[compiled_class_hash] = casm

    casm_cache_dir = cache_root / _CASM_CACHE_SUBDIR
    deprecated_cache_dir = cache_root / _DEPRECATED_CACHE_SUBDIR
    casm_cache_dir.mkdir(parents=True, exist_ok=True)
    deprecated_cache_dir.mkdir(parents=True, exist_ok=True)

    compiled_classes: Dict[str, JsonObject] = {}
    deprecated_compiled_classes: Dict[str, JsonObject] = {}
    fetched_count = 0
    cached_count = 0

    for class_hash, compiled_class_hash in cairo1_compiled_class_hashes.items():
        if compiled_class_hash in compiled_classes:
            continue
        if compiled_class_hash in blob_compiled:
            compiled_classes[compiled_class_hash] = blob_compiled[compiled_class_hash]
            continue
        cached = _read_cache(casm_cache_dir, compiled_class_hash)
        if cached is not None:
            compiled_classes[compiled_class_hash] = cached
            cached_count += 1

    missing_cairo1 = {
        class_hash: compiled_class_hash
        for class_hash, compiled_class_hash in cairo1_compiled_class_hashes.items()
        if compiled_class_hash not in compiled_classes
    }
    # The feeder is queried by class_hash, but the OS looks classes up by
    # compiled_class_hash — re-key the fetch results accordingly.
    for class_hash, casm in _fetch_parallel(missing_cairo1, _fetch_one_casm).items():
        compiled_class_hash = missing_cairo1[class_hash]
        compiled_classes[compiled_class_hash] = casm
        _write_cache(casm_cache_dir, compiled_class_hash, casm)
        fetched_count += 1

    for compiled_class_hash, casm in blob_compiled.items():
        compiled_classes.setdefault(compiled_class_hash, casm)

    missing_cairo0: List[str] = []
    for class_hash in cairo0_class_hashes:
        cached = _read_cache(deprecated_cache_dir, class_hash)
        if cached is None:
            missing_cairo0.append(class_hash)
            continue
        # A stale cache entry may hold a Sierra body; force a refetch.
        if _is_sierra_shape(cached):
            missing_cairo0.append(class_hash)
            continue
        deprecated_compiled_classes[class_hash] = cached
        cached_count += 1
    if missing_cairo0:
        for class_hash, deprecated_class in _fetch_parallel(
            missing_cairo0, _fetch_one_deprecated
        ).items():
            if _is_sierra_shape(deprecated_class):
                logger.warning(f"Skipping class {class_hash}: v0 endpoint returned a Sierra body.")
                continue
            deprecated_compiled_classes[class_hash] = deprecated_class
            _write_cache(deprecated_cache_dir, class_hash, deprecated_class)
            fetched_count += 1

    return compiled_classes, deprecated_compiled_classes, fetched_count, cached_count


def _read_cache(cache_dir: Path, key: str) -> JsonObject | None:
    path = cache_dir / f"{key}.json"
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return None
    except (OSError, ValueError):
        return None


def _write_cache(cache_dir: Path, key: str, value: JsonObject) -> None:
    path = cache_dir / f"{key}.json"
    tmp_path = cache_dir / f".{key}.json.{os.getpid()}.tmp"
    try:
        tmp_path.write_text(json.dumps(value), encoding="utf-8")
        tmp_path.replace(path)
    except OSError:
        try:
            tmp_path.unlink()
        except OSError:
            pass


def _fetch_parallel(
    class_hashes: Iterable[str], fetch_one: Callable[[str], JsonObject]
) -> Dict[str, JsonObject]:
    """Fetch every class concurrently with `fetch_one`; returns `class_hash → response`."""
    class_hashes_list = list(class_hashes)
    if not class_hashes_list:
        return {}
    result: Dict[str, JsonObject] = {}
    errors: List[str] = []
    with ThreadPoolExecutor(max_workers=_FETCH_PARALLELISM) as pool:
        future_to_class = {
            pool.submit(fetch_one, class_hash): class_hash for class_hash in class_hashes_list
        }
        for future in as_completed(future_to_class):
            class_hash = future_to_class[future]
            try:
                result[class_hash] = future.result()
            except Exception as exc:
                errors.append(f"{class_hash}: {exc}")
    if errors:
        raise ClassFetchError(f"feeder fetch failures: {errors}")
    return result


def _fetch_one_casm(class_hash: str) -> JsonObject:
    path = CONFIG.feeder.endpoints.get_compiled_class_by_class_hash
    return _http_get_json(path, {"classHash": class_hash})


def _fetch_one_deprecated(class_hash: str) -> JsonObject:
    path = CONFIG.feeder.endpoints.get_class_by_hash
    return _http_get_json(path, {"classHash": class_hash})


def _http_get_json(path: str, params: Mapping[str, Any]) -> JsonObject:
    base = CONFIG.feeder.base_url.rstrip("/")
    url = f"{base}{path}?{urllib.parse.urlencode(params)}"
    req = urllib.request.Request(url)
    for header, value in CONFIG.feeder.headers.items():
        req.add_header(header, value)
    with urllib.request.urlopen(req, timeout=_FETCH_TIMEOUT_SECONDS) as resp:
        return json.loads(resp.read())
