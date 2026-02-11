from __future__ import annotations

import argparse
import os
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Sequence, Tuple

import requests

JsonObject = Dict[str, object]


_DEFAULT_FEEDER_URL = "https://feeder.alpha-mainnet.starknet.io"
_GET_BLOCK_PATH = "/feeder_gateway/get_block"


_LOG_TX_RE = re.compile(r"\btx=(0x[0-9a-fA-F]+)\b")
_LOG_SOURCE_BLOCK_RE = re.compile(r"\bsource_block=(\d+)\b")


def _norm_hex(value: str) -> str:
    v = value.strip().lower()
    if v and not v.startswith("0x"):
        v = "0x" + v
    return v


@dataclass(frozen=True, slots=True)
class LogEntry:
    source_block: int
    tx_hash: str


def parse_log_list(path: Path) -> List[LogEntry]:
    """
    Parse `echonet/log_list.txt`-style logs.

    Expected line format includes:
      ... tx=0x... source_block=123 ...

    Any line missing either field is ignored.
    """
    out: List[LogEntry] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        tx_m = _LOG_TX_RE.search(line)
        bn_m = _LOG_SOURCE_BLOCK_RE.search(line)
        if not tx_m or not bn_m:
            continue
        out.append(LogEntry(source_block=int(bn_m.group(1)), tx_hash=_norm_hex(tx_m.group(1))))
    return out


class FeederGateway:
    def __init__(
        self,
        *,
        base_url: str = _DEFAULT_FEEDER_URL,
        headers: Optional[Dict[str, str]] = None,
        timeout_seconds: float = 20.0,
        session: Optional[requests.Session] = None,
    ) -> None:
        self._base_url = base_url.rstrip("/")
        self._headers = dict(headers) if headers else {}
        self._timeout_seconds = float(timeout_seconds)
        self._session = session or requests.Session()
        self._owns_session = session is None

    def close(self) -> None:
        if self._owns_session:
            self._session.close()

    def get_block(self, block_number: int) -> Dict:
        resp = self._session.get(
            f"{self._base_url}{_GET_BLOCK_PATH}",
            params={"blockNumber": int(block_number)},
            headers=self._headers,
            timeout=self._timeout_seconds,
        )
        resp.raise_for_status()
        return resp.json()


def unique_in_order(entries: Sequence[LogEntry]) -> List[LogEntry]:
    seen: set[Tuple[int, str]] = set()
    out: List[LogEntry] = []
    for e in entries:
        k = (e.source_block, e.tx_hash)
        if k in seen:
            continue
        seen.add(k)
        out.append(e)
    return out


def _tx_hash_from_feeder_tx(tx: JsonObject) -> Optional[str]:
    v = tx.get("transaction_hash")
    return _norm_hex(v) if isinstance(v, str) else None


def _calldata_as_hex_strings(tx: JsonObject) -> List[str]:
    """
    Return calldata items normalized to lowercase 0x-prefixed strings where possible.
    """
    calldata = tx.get("calldata")
    if not isinstance(calldata, list):
        return []
    out: List[str] = []
    for item in calldata:
        if isinstance(item, str):
            out.append(_norm_hex(item))
        elif isinstance(item, int):
            out.append(hex(item).lower())
        else:
            # Unknown / unexpected type; stringify defensively.
            out.append(_norm_hex(str(item)))
    return out


def find_tx_in_block(block: JsonObject, tx_hash: str) -> Optional[JsonObject]:
    txs = block.get("transactions")
    if not isinstance(txs, list):
        return None
    target = _norm_hex(tx_hash)
    for tx in txs:
        if not isinstance(tx, dict):
            continue
        h = _tx_hash_from_feeder_tx(tx)
        if h == target:
            return tx
    return None


def iter_problematic_txs(
    *,
    entries: Sequence[LogEntry],
    feeder: FeederGateway,
    needle_felt: str,
    verbose: bool = False,
) -> Iterable[Tuple[LogEntry, str, Optional[str]]]:
    """
    Yield (entry, tx_type, reason) for txs whose calldata does NOT include `needle_felt`.
    """
    needle = _norm_hex(needle_felt)
    block_cache: Dict[int, JsonObject] = {}

    for e in entries:
        block = block_cache.get(e.source_block)
        if block is None:
            block = feeder.get_block(e.source_block)
            block_cache[e.source_block] = block

        tx = find_tx_in_block(block, e.tx_hash)
        if tx is None:
            yield (e, "UNKNOWN", "tx_not_found_in_block")
            continue

        tx_type = str(tx.get("type", "UNKNOWN"))
        calldata = _calldata_as_hex_strings(tx)
        if not calldata:
            yield (e, tx_type, "missing_or_empty_calldata")
            continue

        if needle not in calldata:
            if verbose:
                yield (e, tx_type, f"needle_not_in_calldata(calldata_len={len(calldata)})")
            else:
                yield (e, tx_type, "needle_not_in_calldata")


def _parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description=(
            "Given echonet log lines containing tx=... source_block=..., fetch each "
            "source block from the Starknet feeder gateway and flag any tx whose "
            "calldata does not contain a required felt."
        )
    )
    p.add_argument(
        "--log-file",
        type=str,
        default=str(Path(__file__).resolve().parent / "log_list.txt"),
        help="Path to echonet/log_list.txt (default: echonet/log_list.txt).",
    )
    p.add_argument(
        "--feeder-url",
        type=str,
        default=_DEFAULT_FEEDER_URL,
        help=f"Feeder gateway base URL (default: {_DEFAULT_FEEDER_URL}).",
    )
    p.add_argument(
        "--x-throttling-bypass",
        type=str,
        default=None,
        help=(
            "Value for the X-Throttling-Bypass header. "
            "If omitted, FEEDER_X_THROTTLING_BYPASS env var is used. "
            "If neither is set, falls back to CONFIG.feeder.headers (may be empty)."
        ),
    )
    p.add_argument(
        "--needle",
        type=str,
        default="0x10398fe631af9ab2311840432d507bf7ef4b959ae967f1507928f5afe888a99",
        help="Hex felt to require inside calldata (default: provided felt).",
    )
    p.add_argument(
        "--verbose",
        action="store_true",
        help="Include a bit more context in reasons.",
    )
    return p.parse_args()


def main() -> None:
    args = _parse_args()

    log_path = Path(args.log_file)
    if not log_path.exists():
        raise SystemExit(f"log file not found: {log_path}")

    entries = unique_in_order(parse_log_list(log_path))
    if not entries:
        raise SystemExit(f"No (source_block, tx) pairs parsed from: {log_path}")

    headers: Optional[Dict[str, str]] = None
    if args.x_throttling_bypass:
        headers = {"X-Throttling-Bypass": args.x_throttling_bypass}
    elif os.environ.get("FEEDER_X_THROTTLING_BYPASS"):
        headers = {"X-Throttling-Bypass": os.environ["FEEDER_X_THROTTLING_BYPASS"]}

    feeder = FeederGateway(base_url=args.feeder_url, headers=headers)
    try:
        any_found = False
        for entry, tx_type, reason in iter_problematic_txs(
            entries=entries,
            feeder=feeder,
            needle_felt=args.needle,
            verbose=bool(args.verbose),
        ):
            any_found = True
            print(
                f"source_block={entry.source_block} tx={entry.tx_hash} type={tx_type} reason={reason}"
            )
        if not any_found:
            print("No problematic txs found (all txs contained the needle in calldata).")
    finally:
        feeder.close()


if __name__ == "__main__":
    main()
