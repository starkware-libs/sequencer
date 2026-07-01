"""
Resolve the CASM (and deprecated v0) classes the OS needs to run a block.

The cende blob only carries *newly declared* contract classes (`compiled_classes`)
for the block being run. The OS, however, executes against any class touched by
the block — including pre-existing on-chain ones. We bridge the gap by fetching
missing classes from the mainnet feeder gateway, caching them on the echonet
PVC so subsequent blocks don't refetch the same class.

Inputs:
- `initial_reads.compiled_class_hashes`: `class_hash → compiled_class_hash` for
  every Cairo 1+ class touched. The OS's `compiled_classes` map is keyed by
  `compiled_class_hash`, so this map is exactly the fetch list.
- `initial_reads.class_hashes`: `address → class_hash` for every accessed
  contract. Any non-zero class_hash here that is NOT in `compiled_class_hashes`
  is a Cairo 0 (deprecated) class — needs `deprecated_compiled_classes` keyed
  by class_hash. (`class_hash = 0x0` represents uninitialized accounts.)
- `blob["compiled_classes"]`: newly declared CASM for this block, already in the
  shape the OS expects — passed through without refetching.

The fetched JSON from `feeder_gateway/get_compiled_class_by_class_hash` matches
`CasmContractClass` (keys: bytecode, entry_points_by_type, hints, prime,
pythonic_hints, compiler_version) — drop-in for the OS input.
"""

from __future__ import annotations

import json
import os
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Any, Dict, Iterable, List, Mapping, Set, Tuple

from echonet.echonet_types import CONFIG, JsonObject
from echonet.logger import get_logger

logger = get_logger("echonet.class_fetcher")

_CASM_CACHE_SUBDIR = "cairo1"
_DEPRECATED_CACHE_SUBDIR = "cairo0"
_FETCH_PARALLELISM = 8
_FETCH_TIMEOUT_SECONDS = 30
_ZERO_CLASS_HASH = "0x0"


def _is_sierra_shape(class_json: JsonObject) -> bool:
    """
    A class_by_hash response can be either a Cairo 0 deprecated class
    (`program` + offset-keyed entry_points) OR — for some pre-existing
    Cairo 1 classes that the sequencer didn't include in
    `initial_reads.compiled_class_hashes` — a Cairo 1 Sierra class (mainnet's
    v0 endpoint is permissive and returns Sierra anyway). The two shapes are
    structurally distinct: only Sierra carries `sierra_program`.
    """
    return "sierra_program" in class_json


def _is_zero_felt(felt_hex: str) -> bool:
    if not felt_hex.startswith("0x"):
        return False
    return all(c == "0" for c in felt_hex[2:]) or felt_hex == "0x"


class ClassFetchError(RuntimeError):
    """Raised when a required class cannot be fetched from the feeder."""


def resolve_classes_for_os(
    blob: JsonObject, *, cache_root: Path
) -> Tuple[Dict[str, JsonObject], Dict[str, JsonObject], int, int]:
    """
    Resolve all classes the OS will execute when replaying this block.

    Returns:
      compiled_classes:           `compiled_class_hash` → CASM (Cairo 1+)
      deprecated_compiled_classes: `class_hash` → ContractClass v0 (Cairo 0)
      fetched_count:              count of classes fetched from the feeder
      cached_count:               count of classes served from the disk cache

    Newly declared classes (from `blob["compiled_classes"]`) are merged in as-is
    without refetching.
    """
    initial_reads = blob.get("initial_reads") or {}
    compiled_class_hashes: Mapping[str, str] = initial_reads.get("compiled_class_hashes") or {}
    address_to_class_hash: Mapping[str, str] = initial_reads.get("class_hashes") or {}

    # The sequencer's `get_os_initial_reads` (cached_state.rs) force-reads
    # `get_compiled_class_hash` for every accessed class — which per the
    # `StateReader` contract returns `CompiledClassHash::default()` (= felt 0)
    # for Cairo 0 classes. So a class_hash present in `compiled_class_hashes`
    # with value 0 is the sentinel for "this is a Cairo 0 class", not a Cairo 1
    # one. Route those to the v0 endpoint, not the CASM endpoint.
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
    for entry in blob.get("compiled_classes") or []:
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

    missing_cairo1 = [
        (class_hash, compiled_class_hash)
        for class_hash, compiled_class_hash in cairo1_compiled_class_hashes.items()
        if compiled_class_hash not in compiled_classes
    ]
    if missing_cairo1:
        for compiled_class_hash, casm in _fetch_casm_parallel(missing_cairo1).items():
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
        # Stale cache from the pre-fix behavior may hold a Sierra (Cairo 1)
        # body under the deprecated cache. Force a refetch so the new
        # classification path routes it correctly.
        if _is_sierra_shape(cached):
            missing_cairo0.append(class_hash)
            continue
        deprecated_compiled_classes[class_hash] = cached
        cached_count += 1
    if missing_cairo0:
        # The v0 endpoint is permissive — for a class that's actually Cairo 1
        # but absent from `initial_reads.compiled_class_hashes`, it still
        # returns the Sierra body. We can't reuse that body: the FGW's
        # cairo-lang Python serialization of CASM differs from the Rust
        # `CasmContractClass` schema (e.g. `pythonic_hints` is dict vs
        # sequence-of-pairs), so injecting the raw FGW response would just
        # move the deserialization failure. Skip the class — the OS only
        # actually needs its code if it's invoked by some tx in this block;
        # if so, we'll see a cleaner downstream error rather than the
        # opaque "missing field offset" at JSON load.
        for class_hash, deprecated_class in _fetch_deprecated_parallel(missing_cairo0).items():
            if _is_sierra_shape(deprecated_class):
                logger.warning(
                    f"Skipping class {class_hash}: address-only reference, "
                    "v0 endpoint returned Sierra (Cairo 1) but schema mismatch "
                    "blocks using it; OS run will fail only if the class is "
                    "actually invoked in this block."
                )
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


def _fetch_casm_parallel(
    pairs: Iterable[Tuple[str, str]],
) -> Dict[str, JsonObject]:
    """
    `pairs` is `(class_hash, compiled_class_hash)`. Returns `compiled_class_hash → CASM`.
    Class_hash is what the feeder needs as the request param; compiled_class_hash is
    the key the OS will look up by.
    """
    pair_list = list(pairs)
    if not pair_list:
        return {}
    result: Dict[str, JsonObject] = {}
    errors: List[str] = []
    with ThreadPoolExecutor(max_workers=_FETCH_PARALLELISM) as pool:
        future_to_pair = {
            pool.submit(_fetch_one_casm, class_hash): (class_hash, compiled_class_hash)
            for class_hash, compiled_class_hash in pair_list
        }
        for fut in as_completed(future_to_pair):
            class_hash, compiled_class_hash = future_to_pair[fut]
            try:
                result[compiled_class_hash] = fut.result()
            except Exception as exc:
                errors.append(f"{class_hash}: {exc}")
    if errors:
        raise ClassFetchError(f"feeder fetch failures: {errors}")
    return result


def _fetch_deprecated_parallel(class_hashes: Iterable[str]) -> Dict[str, JsonObject]:
    class_hashes_list = list(class_hashes)
    if not class_hashes_list:
        return {}
    result: Dict[str, JsonObject] = {}
    errors: List[str] = []
    with ThreadPoolExecutor(max_workers=_FETCH_PARALLELISM) as pool:
        future_to_class = {
            pool.submit(_fetch_one_deprecated, class_hash): class_hash
            for class_hash in class_hashes_list
        }
        for fut in as_completed(future_to_class):
            class_hash = future_to_class[fut]
            try:
                result[class_hash] = fut.result()
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
