import argparse
import json
import os
import re
import sys
from pathlib import Path
from typing import Dict, Optional, Set

_REPO_ROOT = Path(__file__).resolve().parent.parent
if str(_REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(_REPO_ROOT))

from echonet.echonet_types import CONFIG, JsonObject  # noqa: E402
from echonet.feeder_client import FeederClient  # noqa: E402

_HASH_RE = re.compile(r"^\s*(0x[0-9a-fA-F]+)\b")


def _normalize_tx_hash(value: str) -> str:
    v = value.strip().lower()
    if v and not v.startswith("0x"):
        v = "0x" + v
    return v


def _load_tx_hashes(path: str) -> Set[str]:
    """
    Load tx hashes from a text file that may contain lines like:
      0xabc...: some message
    Only the leading `0x...` hash is kept; everything after is ignored.
    """
    p = Path(path)
    hashes: Set[str] = set()
    for line in p.read_text(encoding="utf-8").splitlines():
        m = _HASH_RE.match(line)
        if not m:
            continue
        hashes.add(_normalize_tx_hash(m.group(1)))
    return hashes


def _load_tx_hashes_with_messages(path: str) -> Dict[str, str]:
    """
    Load tx hashes from a text file and also capture the trailing message (if any).

    Supports lines like:
      0xabc...: Caller is not owner of token 123
      0xdef...

    Returns a map: {normalized_tx_hash: message_str}
    """
    p = Path(path)
    out: Dict[str, str] = {}
    for line in p.read_text(encoding="utf-8").splitlines():
        m = _HASH_RE.match(line)
        if not m:
            continue
        tx_hash = _normalize_tx_hash(m.group(1))
        rest = line[m.end() :].lstrip()
        if rest.startswith(":"):
            rest = rest[1:].lstrip()
        out[tx_hash] = rest
    return out


def scan_blocks_for_tx_hashes_sync(
    *,
    feeder_url: str = CONFIG.feeder.base_url,
    start_block: int,
    end_block: Optional[int] = None,
    headers: Optional[Dict[str, str]] = None,
    stop_tx_hashes: Optional[Set[str]] = None,
    stop_tx_hash_messages: Optional[Dict[str, str]] = None,
    dump_block_json: bool = False,
    dump_matching_txs: bool = False,
) -> Optional[JsonObject]:
    """
    Synchronously scan blocks starting at `start_block` from the feeder gateway.

    Prints output only when a match is found (no per-block progress logging).
    Continues scanning after a match is found.

    Returns the last block object that matched (or None if no match was found).

    `headers` can be used to explicitly pass the X-Throttling-Bypass value from
    the terminal instead of relying solely on environment variables.
    """
    feeder = FeederClient(base_url=feeder_url, headers=headers)
    block_number = int(start_block)
    stop_set = {_normalize_tx_hash(h) for h in (stop_tx_hashes or set())}
    msg_map = {k: v for k, v in (stop_tx_hash_messages or {}).items()}
    last_match: Optional[JsonObject] = None

    while True:
        if end_block is not None and block_number > end_block:
            return last_match

        block: JsonObject = feeder.get_block(block_number)
        txs = block.get("transactions", []) or []
        matching_txs: list[JsonObject] = []
        matching_hashes: list[str] = []
        for tx in txs:
            tx_hash = tx.get("transaction_hash")
            if stop_set and isinstance(tx_hash, str) and _normalize_tx_hash(tx_hash) in stop_set:
                matching_txs.append(tx)
                matching_hashes.append(_normalize_tx_hash(tx_hash))

        if matching_txs:
            block_number_out = block.get("block_number", block_number)
            hashes = ", ".join(_normalize_tx_hash(tx["transaction_hash"]) for tx in matching_txs)
            print(
                f"Found matching transaction hash(es) in block {block_number_out}: {hashes}",
                flush=True,
            )
            flagged_hashes = []
            for h in matching_hashes:
                msg = (msg_map.get(h, "") or "").lower()
                if ("erc721" in msg) or ("caller is not owner of token" in msg):
                    flagged_hashes.append(h)
            if flagged_hashes:
                flagged_hashes_str = ", ".join(flagged_hashes)
                print(
                    f"matches revert filter (ERC721 / not owner of token): {flagged_hashes_str}",
                    flush=True,
                )
            if dump_block_json:
                print(json.dumps(block, indent=2, sort_keys=False), flush=True)
            if dump_matching_txs:
                for tx in matching_txs:
                    print(json.dumps(tx, indent=2, sort_keys=False), flush=True)
            last_match = block

        block_number += 1


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Scan Starknet feeder blocks starting from a given block and stop when a "
            "transaction hash matches one from a provided file."
        )
    )
    parser.add_argument(
        "start_block",
        type=int,
        nargs="?",
        default=None,
        help="Block number to start searching from (inclusive).",
    )
    parser.add_argument(
        "--start-block",
        "--start_block",
        dest="start_block_opt",
        type=int,
        default=None,
        help="Block number to start searching from (inclusive). (Alias for positional start_block.)",
    )
    parser.add_argument(
        "--end-block",
        type=int,
        default=None,
        help="Optional last block number to search (inclusive). If omitted, search is unbounded.",
    )
    parser.add_argument(
        "--feeder-url",
        type=str,
        default=CONFIG.feeder.base_url,
        help=f"Feeder gateway base URL (default: {CONFIG.feeder.base_url}).",
    )
    parser.add_argument(
        "--x-throttling-bypass",
        type=str,
        default=None,
        help=(
            "Value for the X-Throttling-Bypass header. "
            "If omitted, the FEEDER_X_THROTTLING_BYPASS environment variable "
            "from the shell (if set) will be used via CONFIG.feeder.headers."
        ),
    )
    parser.add_argument(
        "--dump-json",
        action="store_true",
        help="Print the full JSON block if found (otherwise only prints the block number).",
    )
    parser.add_argument(
        "--dump-matching-txs",
        action="store_true",
        help="Print the full JSON for the matching transaction(s) in the found block.",
    )
    parser.add_argument(
        "--tx-hash-file",
        type=str,
        required=True,
        help=(
            "Path to a text file containing transaction hashes to stop on. "
            "Lines may be in the form `0xHASH: message`; only the leading hash is used."
        ),
    )
    return parser.parse_args()


def main() -> None:
    args = _parse_args()

    headers: Optional[Dict[str, str]] = None
    if args.x_throttling_bypass:
        headers = {"X-Throttling-Bypass": args.x_throttling_bypass}
    elif os.environ.get("FEEDER_X_THROTTLING_BYPASS"):
        headers = {"X-Throttling-Bypass": os.environ["FEEDER_X_THROTTLING_BYPASS"]}

    start_block = args.start_block_opt if args.start_block_opt is not None else args.start_block
    if start_block is None:
        raise SystemExit(
            "start_block is required (provide positional start_block or --start_block)."
        )

    stop_tx_hash_messages = _load_tx_hashes_with_messages(args.tx_hash_file)
    stop_tx_hashes = set(stop_tx_hash_messages.keys())
    if not stop_tx_hashes:
        raise SystemExit(f"No tx hashes found in file: {args.tx_hash_file}")

    last_match = scan_blocks_for_tx_hashes_sync(
        feeder_url=args.feeder_url,
        start_block=start_block,
        end_block=args.end_block,
        headers=headers,
        stop_tx_hashes=stop_tx_hashes,
        stop_tx_hash_messages=stop_tx_hash_messages,
        dump_block_json=args.dump_json,
        dump_matching_txs=args.dump_matching_txs,
    )

    if args.end_block is not None and last_match is None:
        print(
            f"No matching transaction hash found between blocks "
            f"{start_block} and {args.end_block} (inclusive)."
        )


if __name__ == "__main__":
    main()
