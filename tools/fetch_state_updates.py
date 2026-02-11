#!/usr/bin/env python3
"""
Fetch multiple Starknet feeder_gateway state updates and aggregate them into a list.

By default, prints a JSON array to stdout (so you can redirect it to a file).
Progress/errors are printed to stderr.
"""

from __future__ import annotations

import argparse
import json
import sys
import time
import urllib.error
import urllib.request
from typing import Any

URLS: list[str] = [
    "https://feeder.alpha-mainnet.starknet.io/feeder_gateway/get_state_update?blockNumber=6099312",
    "https://feeder.alpha-mainnet.starknet.io/feeder_gateway/get_state_update?blockNumber=6099784",
    "https://feeder.alpha-mainnet.starknet.io/feeder_gateway/get_state_update?blockNumber=6100038",
    "https://feeder.alpha-mainnet.starknet.io/feeder_gateway/get_state_update?blockNumber=6100869",
    "https://feeder.alpha-mainnet.starknet.io/feeder_gateway/get_state_update?blockNumber=6100927",
    "https://feeder.alpha-mainnet.starknet.io/feeder_gateway/get_state_update?blockNumber=6100937",
    "https://feeder.alpha-mainnet.starknet.io/feeder_gateway/get_state_update?blockNumber=6101430",
    "https://feeder.alpha-mainnet.starknet.io/feeder_gateway/get_state_update?blockNumber=6101581",
    "https://feeder.alpha-mainnet.starknet.io/feeder_gateway/get_state_update?blockNumber=6101691",
    "https://feeder.alpha-mainnet.starknet.io/feeder_gateway/get_state_update?blockNumber=6102487",
]


def _fetch_json(url: str, *, timeout_s: float, retries: int, backoff_s: float) -> Any:
    """
    Fetch URL and parse JSON, with simple retries/backoff.

    Raises on final failure.
    """
    last_err: BaseException | None = None
    headers = {
        "Accept": "application/json",
        "User-Agent": "sequencer-tools/fetch_state_updates.py",
    }

    for attempt in range(1, retries + 2):  # retries=0 -> one attempt
        try:
            req = urllib.request.Request(url, headers=headers, method="GET")
            with urllib.request.urlopen(req, timeout=timeout_s) as resp:
                # `resp.read()` returns bytes.
                payload = resp.read().decode("utf-8")
            return json.loads(payload)
        except (
            urllib.error.HTTPError,
            urllib.error.URLError,
            TimeoutError,
            json.JSONDecodeError,
        ) as e:
            last_err = e
            if attempt >= retries + 1:
                break
            sleep_s = backoff_s * (2 ** (attempt - 1))
            print(
                f"[warn] fetch failed (attempt {attempt}/{retries + 1}), "
                f"sleeping {sleep_s:.2f}s: {url}\n  error: {e}",
                file=sys.stderr,
            )
            time.sleep(sleep_s)

    assert last_err is not None
    raise last_err


def fetch_all_state_updates(
    urls: list[str],
    *,
    timeout_s: float = 30.0,
    retries: int = 2,
    backoff_s: float = 0.5,
) -> list[Any]:
    """
    Returns a list of parsed JSON objects in the same order as `urls`.
    """
    results: list[Any] = []
    for i, url in enumerate(urls, start=1):
        print(f"[info] fetching {i}/{len(urls)}: {url}", file=sys.stderr)
        obj = _fetch_json(url, timeout_s=timeout_s, retries=retries, backoff_s=backoff_s)
        results.append(obj)
    return results


def _parse_args(argv: list[str]) -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Fetch Starknet state updates and aggregate as JSON array."
    )
    p.add_argument(
        "--output",
        "-o",
        default="-",
        help="Output path. Use '-' (default) for stdout.",
    )
    p.add_argument(
        "--pretty",
        action="store_true",
        help="Pretty-print JSON output (indent=2).",
    )
    p.add_argument(
        "--timeout",
        type=float,
        default=30.0,
        help="Per-request timeout in seconds.",
    )
    p.add_argument(
        "--retries",
        type=int,
        default=2,
        help="Number of retries per URL (default: 2). Total attempts = retries + 1.",
    )
    p.add_argument(
        "--backoff",
        type=float,
        default=0.5,
        help="Initial backoff in seconds (exponential, per retry).",
    )
    return p.parse_args(argv)


def main(argv: list[str]) -> int:
    args = _parse_args(argv)

    state_updates = fetch_all_state_updates(
        URLS,
        timeout_s=float(args.timeout),
        retries=int(args.retries),
        backoff_s=float(args.backoff),
    )

    dump_kwargs = {"ensure_ascii": False}
    if args.pretty:
        dump_kwargs["indent"] = 2
        dump_kwargs["sort_keys"] = True

    out_str = json.dumps(state_updates, **dump_kwargs)
    if args.output == "-":
        sys.stdout.write(out_str)
        sys.stdout.write("\n")
    else:
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(out_str)
            f.write("\n")
        print(f"[info] wrote {len(state_updates)} JSON objects to {args.output}", file=sys.stderr)

    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
