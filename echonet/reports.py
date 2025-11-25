import argparse
import json
import sys

import requests


def print_snapshot(endpoint: str) -> None:
    try:
        resp = requests.get(endpoint, timeout=5)
        resp.raise_for_status()
        data = resp.json()
    except Exception as e:
        print(f"Failed to fetch snapshot from {endpoint}: {e}", file=sys.stderr)
        sys.exit(1)
    gw = data.get("gateway_errors")
    print("=== Gateway errors (hash -> response) ===")
    for k, v in gw.items():
        print(f"{k}: {v}")
    print("=== Sent tx hashes ===")
    for k, v in (data.get("sent_tx_hashes")).items():
        print(f"{k} @ {v}")
    print(f"Committed count: {data.get('committed_count')}")
    print(f"Sender running: {data.get('running')}")
    print(f"Sent empty: {data.get('sent_empty')}")


def compare_reverts(endpoint: str) -> None:
    try:
        resp = requests.get(endpoint, timeout=5)
        resp.raise_for_status()
        data = resp.json()
    except Exception as e:
        print(f"Failed to fetch snapshot from {endpoint}: {e}", file=sys.stderr)
        sys.exit(1)
    revs_mainnet = data.get("revert_errors_mainnet")
    revs_echonet = data.get("revert_errors_echonet")
    # Matched reverts are removed upon echonet addition, so remaining entries are inherently “only” sets
    mainnet_only = list(revs_mainnet.items())
    echonet_only = list(revs_echonet.items())
    print("=== Reverted only on Mainnet ===")
    for h, msg in mainnet_only:
        print(f"{h}: {msg}")
    print("=== Reverted only on Echonet ===")
    for h, msg in echonet_only:
        print(f"{h}: {msg}")


def show_block(endpoint: str, block_number: int, kind: str) -> None:
    url = f"{endpoint.rsplit('/echonet/report', 1)[0]}/echonet/block_dump?blockNumber={block_number}&kind={kind}"
    try:
        resp = requests.get(url, timeout=5)
        if resp.status_code == 404:
            print(f"Block {block_number} not found")
            sys.exit(1)
        resp.raise_for_status()
        obj = resp.json()
    except Exception as e:
        print(
            f"Failed to fetch block {block_number} ({kind}) from {url}: {e}",
            file=sys.stderr,
        )
        sys.exit(1)
    print(f"=== Block {block_number} [{kind}] ===")
    print(json.dumps(obj, ensure_ascii=False, indent=2))


def main() -> None:
    parser = argparse.ArgumentParser(description="Echonet reports")
    parser.add_argument(
        "--snapshot",
        action="store_true",
        help="Fetch and print in-memory tx snapshot from the running app",
    )
    parser.add_argument(
        "--compare-reverts",
        action="store_true",
        help="Compare revert errors from in-memory state (mainnet vs echonet)",
    )
    parser.add_argument(
        "--endpoint",
        default="http://127.0.0.1/echonet/report",
        help="Report endpoint to query (default: http://127.0.0.1/echonet/report)",
    )
    parser.add_argument(
        "--all", action="store_true", help="Run both snapshot and revert comparison"
    )
    parser.add_argument("--show-block", type=int, help="Block number to fetch from in-memory store")
    parser.add_argument(
        "--kind",
        choices=["blob", "block", "state_update"],
        default="blob",
        help="Which payload to fetch for --show-block",
    )
    args = parser.parse_args()

    # If no explicit action flags provided, run --all by default
    if not (args.snapshot or args.compare_reverts or args.all or args.show_block):
        args.all = True

    if args.snapshot or args.all:
        print_snapshot(args.endpoint)
        if not args.all:
            return

    if args.compare_reverts or args.all:
        compare_reverts(args.endpoint)
        if not args.all:
            return

    if args.show_block is not None:
        show_block(args.endpoint, args.show_block, args.kind)
        return


if __name__ == "__main__":
    main()
