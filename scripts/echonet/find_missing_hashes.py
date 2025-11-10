import re
import sys
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


def main() -> None:
    lookfor_path = Path(sys.argv[1])
    hashes_path = Path(sys.argv[2])

    lookfor_text = read_text_file(lookfor_path)
    hashes_text = read_text_file(hashes_path)

    # Build a set of exact hashes present in hashes.txt for O(1) membership
    hashes_in_file = {h.lower() for h in extract_hashes(hashes_text)}

    # Preserve order from lookfor.txt and print those missing from hashes.txt
    for tx_hash in extract_hashes(lookfor_text):
        if tx_hash.lower() not in hashes_in_file:
            print(tx_hash)


if __name__ == "__main__":
    main()
