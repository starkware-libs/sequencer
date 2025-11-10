import re
import sys
import json
import argparse
from pathlib import Path


def read_text_file(file_path: Path) -> str:
    try:
        return file_path.read_text(encoding="utf-8", errors="ignore")
    except FileNotFoundError:
        print(f"Error: file not found: {file_path}", file=sys.stderr)
        sys.exit(1)


def extract_hashes(text: str) -> list[str]:
    # Match any hex string beginning with 0x (len>=3); allow variable length
    candidates = re.findall(r"0x[0-9a-fA-F]+", text)
    # Deduplicate while preserving order
    seen = set()
    ordered_unique = []
    for item in candidates:
        key = item.lower()
        if key not in seen:
            seen.add(key)
            ordered_unique.append(item)
    return ordered_unique


def _read_jsonl_hashes_ordered(file_path: Path) -> list[str]:
    """
    Read a JSONL file where each line is a JSON object like {"0xHASH": "..."}.
    Return an ordered, de-duplicated list of hash keys found.
    """
    ordered: list[str] = []
    seen: set[str] = set()
    try:
        for line in file_path.read_text(encoding="utf-8", errors="ignore").splitlines():
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except Exception:
                continue
            if isinstance(obj, dict):
                for key in obj.keys():
                    if isinstance(key, str) and key.lower().startswith("0x"):
                        k = key.lower()
                        if k not in seen:
                            seen.add(k)
                            ordered.append(key)
    except FileNotFoundError:
        print(f"Error: file not found: {file_path}", file=sys.stderr)
        sys.exit(1)
    return ordered


def compare_revert_files(app_file: Path, send_file: Path) -> None:
    """
    Print which transaction hashes appear in app.py's errors file but not in send_txs.py's,
    and vice versa. Each line is just the 0x-hash; sections are labeled.
    """
    app_hashes_ordered = _read_jsonl_hashes_ordered(app_file)
    send_hashes_ordered = _read_jsonl_hashes_ordered(send_file)
    send_set = {h.lower() for h in send_hashes_ordered}
    app_set = {h.lower() for h in app_hashes_ordered}

    only_in_app = [h for h in app_hashes_ordered if h.lower() not in send_set]
    only_in_send = [h for h in send_hashes_ordered if h.lower() not in app_set]

    print("Reverted only in Echonet:")
    for h in only_in_app:
        print(h)
    print("Reverted only in Mainnet:")
    for h in only_in_send:
        print(h)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Find missing hashes or compare revert mappings between files"
    )
    parser.add_argument(
        "--compare-reverts",
        nargs=2,
        metavar=("APP_JSONL", "SEND_JSONL"),
        help="Compare JSONL revert mapping files from app.py and send_txs.py",
    )
    parser.add_argument("lookfor_path", nargs="?")
    parser.add_argument("hashes_path", nargs="?")
    args = parser.parse_args()

    # New mode: compare two JSONL files of {hash: error}
    if args.compare_reverts:
        app_file = Path(args.compare_reverts[0])
        send_file = Path(args.compare_reverts[1])
        compare_revert_files(app_file, send_file)
        return

    # Legacy mode: print hashes present in lookfor_path but missing from hashes_path
    if not (args.lookfor_path and args.hashes_path):
        parser.print_usage(sys.stderr)
        sys.exit(2)

    lookfor_path = Path(args.lookfor_path)
    hashes_path = Path(args.hashes_path)

    lookfor_text = read_text_file(lookfor_path)
    hashes_text = read_text_file(hashes_path)

    hashes_in_file = {h.lower() for h in extract_hashes(hashes_text)}
    for tx_hash in extract_hashes(lookfor_text):
        if tx_hash.lower() not in hashes_in_file:
            print(tx_hash)


if __name__ == "__main__":
    main()
