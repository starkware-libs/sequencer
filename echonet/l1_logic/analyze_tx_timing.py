#!/usr/bin/env python3
"""
Script to analyze timing differences between L1 and L2 transactions.
Fetches data from Alchemy (L1) and Starknet Feeder Gateway (L2).
"""

import os
import sys
import requests
from typing import Optional
from dataclasses import dataclass
from datetime import datetime

# Add paths for imports
sys.path.insert(0, os.path.join(os.path.dirname(__file__)))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "../.."))

from l1_client import L1Client


@dataclass
class TransactionData:
    l1_tx_hash: str
    l2_tx_hash: str
    l1_block_number: int
    l1_timestamp: Optional[int] = None
    l2_block_number: Optional[int] = None
    l2_timestamp: Optional[int] = None
    time_difference_seconds: Optional[int] = None


FEEDER_GATEWAY_BASE_URL = "https://feeder.alpha-mainnet.starknet.io/feeder_gateway"


def get_l2_transaction(tx_hash: str) -> Optional[dict]:
    """Get L2 transaction from feeder gateway."""
    url = f"{FEEDER_GATEWAY_BASE_URL}/get_transaction?transactionHash={tx_hash}"
    try:
        response = requests.get(url, timeout=10)
        response.raise_for_status()
        return response.json()
    except Exception as e:
        print(f"Error fetching L2 transaction {tx_hash}: {e}")
        return None


def get_l2_block(block_number: int) -> Optional[dict]:
    """Get L2 block from feeder gateway."""
    url = f"{FEEDER_GATEWAY_BASE_URL}/get_block?blockNumber={block_number}"
    try:
        response = requests.get(url, timeout=10)
        response.raise_for_status()
        return response.json()
    except Exception as e:
        print(f"Error fetching L2 block {block_number}: {e}")
        return None


def analyze_transaction(
    l1_client: L1Client, l1_tx_hash: str, l2_tx_hash: str, l1_block_number: int
) -> TransactionData:
    """Analyze a single transaction and return timing data."""
    print(f"\nAnalyzing transaction:")
    print(f"  L1 TX: {l1_tx_hash}")
    print(f"  L2 TX: {l2_tx_hash}")
    print(f"  L1 Block: {l1_block_number}")

    tx_data = TransactionData(
        l1_tx_hash=l1_tx_hash,
        l2_tx_hash=l2_tx_hash,
        l1_block_number=l1_block_number,
    )

    # Get L1 block and timestamp
    print(f"  Fetching L1 block {l1_block_number}...")
    l1_block_response = l1_client.get_timestamp_of_block(l1_block_number)
    if l1_block_response is not None:
        l1_timestamp = l1_block_response
        tx_data.l1_timestamp = l1_timestamp
        print(f"  L1 timestamp: {tx_data.l1_timestamp} ({datetime.fromtimestamp(tx_data.l1_timestamp)})")
    else:
        print(f"  ERROR: Could not fetch L1 block {l1_block_number}")
        return tx_data

    # Get L2 transaction
    print(f"  Fetching L2 transaction {l2_tx_hash}...")
    l2_tx = get_l2_transaction(l2_tx_hash)
    if not l2_tx:
        print(f"  ERROR: Could not fetch L2 transaction")
        return tx_data

    l2_block_number = l2_tx.get("block_number")
    if not l2_block_number:
        print(f"  ERROR: L2 transaction has no block_number")
        return tx_data

    tx_data.l2_block_number = l2_block_number
    print(f"  L2 block number: {l2_block_number}")

    # Get L2 block and timestamp
    print(f"  Fetching L2 block {l2_block_number}...")
    l2_block = get_l2_block(l2_block_number)
    if not l2_block:
        print(f"  ERROR: Could not fetch L2 block")
        return tx_data

    l2_timestamp = l2_block.get("timestamp")
    if l2_timestamp is None:
        print(f"  ERROR: L2 block has no timestamp")
        return tx_data

    tx_data.l2_timestamp = l2_timestamp
    print(f"  L2 timestamp: {tx_data.l2_timestamp} ({datetime.fromtimestamp(tx_data.l2_timestamp)})")

    # Calculate time difference
    if tx_data.l1_timestamp and tx_data.l2_timestamp:
        tx_data.time_difference_seconds = tx_data.l2_timestamp - tx_data.l1_timestamp
        print(f"  Time difference: {tx_data.time_difference_seconds} seconds ({tx_data.time_difference_seconds / 60:.2f} minutes)")

    return tx_data


def generate_report(transactions: list[TransactionData]) -> str:
    """Generate a markdown report with all transaction data and statistics."""
    report = ["# L1/L2 Transaction Timing Analysis\n"]

    # Table header
    report.append("## Transaction Details\n")
    report.append("| L1 TX | L1 Block | L1 Timestamp | L2 TX | L2 Block | L2 Timestamp | Time Diff (s) | Time Diff (min) |")
    report.append("|-------|----------|--------------|-------|----------|--------------|---------------|-----------------|")

    valid_transactions = []
    for tx in transactions:
        if tx.l1_timestamp and tx.l2_timestamp:
            valid_transactions.append(tx)
            l1_time = datetime.fromtimestamp(tx.l1_timestamp).strftime("%Y-%m-%d %H:%M:%S")
            l2_time = datetime.fromtimestamp(tx.l2_timestamp).strftime("%Y-%m-%d %H:%M:%S")
            time_diff_min = tx.time_difference_seconds / 60 if tx.time_difference_seconds else 0

            report.append(
                f"| {tx.l1_tx_hash[:16]}... | {tx.l1_block_number} | {l1_time} | "
                f"{tx.l2_tx_hash[:16]}... | {tx.l2_block_number} | {l2_time} | "
                f"{tx.time_difference_seconds} | {time_diff_min:.2f} |"
            )

    # Statistics
    if valid_transactions:
        report.append("\n## Statistics\n")
        time_diffs = [tx.time_difference_seconds for tx in valid_transactions if tx.time_difference_seconds is not None]
        
        if time_diffs:
            avg_diff = sum(time_diffs) / len(time_diffs)
            min_diff = min(time_diffs)
            max_diff = max(time_diffs)
            
            report.append(f"- **Total transactions analyzed:** {len(valid_transactions)}")
            report.append(f"- **Average time difference:** {avg_diff:.2f} seconds ({avg_diff / 60:.2f} minutes)")
            report.append(f"- **Minimum time difference:** {min_diff} seconds ({min_diff / 60:.2f} minutes)")
            report.append(f"- **Maximum time difference:** {max_diff} seconds ({max_diff / 60:.2f} minutes)")

    return "\n".join(report)


def main():
    # All transactions to analyze
    transactions_data = [
        {
            "l1_tx": "0x342723aaa6b8fb4defc6ccf11dbce717a9413526dd3a2454979a7d5430e57279",
            "l2_tx": "0x72ec17e1b956c79cc81da0e7cc188937dc71c8f271c4fc0946bb4b1260c34b2",
            "l1_block": 23968650,
        },
        {
            "l1_tx": "0x72ae47dcff4a2f62d0f1328a2e232c99a4dce21b8471742ce87584cf0e9d0778",
            "l2_tx": "0x47a71910b822395c3ac07744c3c7521c82c31c0732afd3fdab94401fbdf346f",
            "l1_block": 24013093,
        },
        {
            "l1_tx": "0xda86667b4b19c0f212d4d4fa6533bb0265c399b4e9e0f8cd7d502d78f30f0cfd",
            "l2_tx": "0x4ebcff94576ae2565b0eb7bdf626a37b38e3a9d932fe10c79ed8e1f99caf530",
            "l1_block": 24013123,
        },
        {
            "l1_tx": "0xabd01dd3eb3d90313307f83c1d8ee83d7ee316370599afcfe8a2927588b094aa",
            "l2_tx": "0x6c21653cbcaf75af03e77ac0902c78823017c30558e530b44607838f0935117",
            "l1_block": 24013152,
        },
        {
            "l1_tx": "0x17f7b5f5bac18783919586ea1e43f7d95abde8185bc34870ff651535815e4ffe",
            "l2_tx": "0x6b184b162a385b869a9196031b0b6558bae31c8c26bdf2919f0102e79a1e2bd",
            "l1_block": 24013256,
        },
        {
            "l1_tx": "0x3de2bd653ee1f422c7ca591eed4c6bdcaf57189de8ec0c0099e7378d97da86fc",
            "l2_tx": "0x3c2b30e932e70c35b3c398440d46ad9208ff5182d1c6984df429df20cd587da",
            "l1_block": 24013612,
        },
        {
            "l1_tx": "0xe91f54b046bd3a67fda03c0a3c7117d9bc3438e3f2054cc5c66defbe3223b93e",
            "l2_tx": "0x8cb8d0d4cb0b4fde323ad37137997779535faa0bb042522feb27d7f04709bd",
            "l1_block": 24013640,
        },
        {
            "l1_tx": "0x01f8ee74f2fab3c442b78c7f1e915367a2bf307b47eff0115a9fe45dad163b57",
            "l2_tx": "0x3219d8b6fdbdd4ad4276f53d08e5a35f6aa755ef90249f9b6827f1b5463717a",
            "l1_block": 24013672,
        },
        {
            "l1_tx": "0x77351d43655acd11ba4e8cd4c652df4dd590d956a982e50bfe6c1bc779dda605",
            "l2_tx": "0x5a0cac845060acec715e6c51f344852b72f9ad615d02d9cb39252d7cc9a6b65",
            "l1_block": 24014106,
        },
        {
            "l1_tx": "0x7caf836f150f3903280f43bac77ba0b3bd7cc25c3244c4791287961c22f6dac4",
            "l2_tx": "0x4ffc0f06728487b97446e57361bd60b9e407d9d6e6aaa78be2b24efdccf2a2f",
            "l1_block": 24014234,
        },
        {
            "l1_tx": "0xc008f333bb90801d33cc0f76d09324377dbc474a8dba4a3e03a984033c7313d5",
            "l2_tx": "0x2e5a3c882c9d24e9144ffd3ae4b80702db1d4ef50df2f67e3b19de6c6da871f",
            "l1_block": 24014362,
        },
        {
            "l1_tx": "0x23c1f818e61af15b8edff79edd89bdd304a6e1e00eba32ffa02f0d30910f0f82",
            "l2_tx": "0x1eadceb508dc171c49da07248770d204553ea69974b6b187811f56dcd51df9",
            "l1_block": 24014375,
        },
        {
            "l1_tx": "0xc08ed494778185b93bd862c136af219091b70a8378986ee0b5169b58516542e3",
            "l2_tx": "0x48938c476a5bd4e71d50271ca9cdd98a38f842f3c44b6d787cb591e396dd234",
            "l1_block": 24014389,
        },
        {
            "l1_tx": "0x35d4fe8db3d388f1d6b898bb3b2c9aa5ec749fac5596e439acac144ae988163c",
            "l2_tx": "0x9da4dd8891741596d2fa51b0bf62c29112650cfce761321d417843e494e6bf",
            "l1_block": 24014408,
        },
        {
            "l1_tx": "0x80326718da9f0d895b1531f9bd05ee4c09934545e02347b0af8c0ce2c5d2d048",
            "l2_tx": "0x2f51c6ed29ec0e489385cd1d0d5b4731c4ddeeb9a64c4941a668b0dc9c13c53",
            "l1_block": 24014538,
        },
    ]

    # Initialize L1 client (you'll need to set your API key)
    api_key = "your_alchemy_api_key_here"
    if not api_key:
        print("ERROR: Please set ALCHEMY_API_KEY environment variable")
        sys.exit(1)

    l1_client = L1Client(api_key=api_key)

    # Analyze transactions
    print(f"Analyzing {len(transactions_data)} transactions...")
    transactions = []
    for idx, data in enumerate(transactions_data, 1):
        print(f"\n[{idx}/{len(transactions_data)}] Processing transaction...")
        tx_data = analyze_transaction(
            l1_client,
            data["l1_tx"],
            data["l2_tx"],
            data["l1_block"],
        )
        transactions.append(tx_data)

    # Generate and save report
    report = generate_report(transactions)
    print("\n" + "=" * 80)
    print("REPORT:")
    print("=" * 80)
    print(report)

    # Save to file
    output_file = "tx_timing_analysis.md"
    with open(output_file, "w") as f:
        f.write(report)
    print(f"\nReport saved to {output_file}")


if __name__ == "__main__":
    main()

