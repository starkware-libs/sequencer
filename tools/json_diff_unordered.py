#!/usr/bin/env python3
"""
JSON diff checker that treats ALL lists as unordered (multiset) collections.

Usage:
  python3 tools/json_diff_unordered.py path/to/a.json path/to/b.json

Exit codes:
  0 - equal under unordered-list semantics
  1 - different
  2 - usage / read / parse error
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable, List

Json = Any


def _stable_dumps(value: Json) -> str:
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"))


def normalize_unordered_lists(value: Json) -> Json:
    """
    Normalize JSON so that:
    - dict keys are sorted (via stable dumps usage)
    - ALL lists are treated as unordered: we normalize items and then sort the list by their stable JSON encoding.

    This is suitable for equality testing when list order should not matter.
    """
    if value is None or isinstance(value, (bool, int, float, str)):
        return value
    if isinstance(value, dict):
        return {k: normalize_unordered_lists(v) for k, v in value.items()}
    if isinstance(value, list):
        normalized_items = [normalize_unordered_lists(v) for v in value]
        normalized_items.sort(key=_stable_dumps)
        return normalized_items
    # If you have non-JSON-native types, fail loudly.
    raise TypeError(f"Unsupported JSON type: {type(value).__name__}")


@dataclass(frozen=True, slots=True)
class Diff:
    path: str
    message: str


def _diff_values(a: Json, b: Json, path: str) -> List[Diff]:
    if type(a) is not type(b):
        return [Diff(path, f"type mismatch: {type(a).__name__} != {type(b).__name__}")]

    if a is None or isinstance(a, (bool, int, float, str)):
        if a != b:
            return [Diff(path, f"value mismatch: {_stable_dumps(a)} != {_stable_dumps(b)}")]
        return []

    if isinstance(a, dict):
        diffs: List[Diff] = []
        a_keys = set(a.keys())
        b_keys = set(b.keys())
        for k in sorted(a_keys - b_keys):
            diffs.append(Diff(f"{path}.{k}" if path else k, "missing from right"))
        for k in sorted(b_keys - a_keys):
            diffs.append(Diff(f"{path}.{k}" if path else k, "missing from left"))
        for k in sorted(a_keys & b_keys):
            diffs.extend(_diff_values(a[k], b[k], f"{path}.{k}" if path else k))
        return diffs

    if isinstance(a, list):
        # Lists are unordered multisets after normalization, but diffing them as sets loses multiplicity.
        # So we diff as Counters on a stable representation of each element.
        a_norm = [normalize_unordered_lists(x) for x in a]
        b_norm = [normalize_unordered_lists(x) for x in b]
        a_counts = Counter(_stable_dumps(x) for x in a_norm)
        b_counts = Counter(_stable_dumps(x) for x in b_norm)

        only_left = a_counts - b_counts
        only_right = b_counts - a_counts
        diffs: List[Diff] = []
        if only_left:
            sample = ", ".join([f"{k}×{v}" for k, v in list(only_left.items())[:15]])
            diffs.append(Diff(path, f"list has extra elements on left (showing up to 5): {sample}"))
        if only_right:
            sample = ", ".join([f"{k}×{v}" for k, v in list(only_right.items())[:15]])
            diffs.append(
                Diff(path, f"list has extra elements on right (showing up to 5): {sample}")
            )
        return diffs

    raise TypeError(f"Unsupported JSON type: {type(a).__name__}")


def load_json(path: Path) -> Json:
    raw = path.read_text(encoding="utf-8")
    return json.loads(raw)


def main(argv: Iterable[str]) -> int:
    p = argparse.ArgumentParser(
        description="Diff two JSON files, ignoring list order (unordered multisets)."
    )
    p.add_argument("left", type=Path)
    p.add_argument("right", type=Path)
    args = p.parse_args(list(argv))

    try:
        left = load_json(args.left)
        right = load_json(args.right)
    except Exception as e:
        print(f"error: failed to load/parse JSON: {e}", file=sys.stderr)
        return 2

    left_norm = normalize_unordered_lists(left)
    right_norm = normalize_unordered_lists(right)

    if left_norm == right_norm:
        print("equal (treating all lists as unordered)")
        return 0

    diffs = _diff_values(left, right, path="")
    print(f"DIFF: {len(diffs)} difference(s) (treating all lists as unordered)\n")
    for d in diffs[:200]:
        print(f"- {d.path}: {d.message}")
    if len(diffs) > 200:
        print(f"\n... truncated ({len(diffs) - 200} more)")
    return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
