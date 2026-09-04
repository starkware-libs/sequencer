#!/usr/bin/env python3
"""
Builds the batcher's `blocked_storage_keys` config value from a list of account addresses.

For each address, the output contains the ERC20 balance storage entry of that account, i.e. the
storage key of `ERC20_balances[address]`. A balance is a u256 stored in two consecutive slots, and
reading or writing it always accesses the low word, so blocking the low word alone blocks every
balance access. Pass `--include-high-word` to also emit the second slot.

Usage (from the repo root, inside the venv):
    python scripts/blocked_balance_storage_keys.py 0x1234... 0xabcd...
    python scripts/blocked_balance_storage_keys.py 0x1234...,0xabcd...
"""

import argparse
import sys
from typing import List

from Crypto.Hash import keccak
from starkware.crypto.signature.fast_pedersen_hash import pedersen_hash

# Matches `starknet_api::core::L2_ADDRESS_UPPER_BOUND`: 2**251 - MAX_STORAGE_ITEM_SIZE.
STORAGE_KEY_UPPER_BOUND = 2**251 - 256
# Matches `starknet_api::core::PATRICIA_KEY_UPPER_BOUND_FELT`.
ADDRESS_UPPER_BOUND = 2**251
STARKNET_KECCAK_MASK = 2**250 - 1
DEFAULT_BALANCES_STORAGE_VAR_NAME = "ERC20_balances"


def starknet_keccak(data: bytes) -> int:
    keccak_digest = keccak.new(digest_bits=256, data=data).digest()
    return int.from_bytes(keccak_digest, byteorder="big") & STARKNET_KECCAK_MASK


def get_storage_var_address(storage_var_name: str, args: List[int]) -> int:
    """
    Mirrors `starknet_api::abi::abi_utils::get_storage_var_address`.
    """
    storage_key_hash = starknet_keccak(storage_var_name.encode("ascii"))
    for arg in args:
        storage_key_hash = pedersen_hash(storage_key_hash, arg)
    return storage_key_hash % STORAGE_KEY_UPPER_BOUND


def balance_storage_keys(address: int, storage_var_name: str, include_high_word: bool) -> List[int]:
    low_word_key = get_storage_var_address(storage_var_name, [address])
    if not include_high_word:
        return [low_word_key]
    # The high word sits in the next storage slot (`StorageKey::next_storage_key`).
    return [low_word_key, low_word_key + 1]


def parse_address(address_hex: str) -> int:
    try:
        address = int(address_hex, 16)
    except ValueError:
        raise argparse.ArgumentTypeError(f"{address_hex!r} is not a hexadecimal number.")
    if not 0 <= address < ADDRESS_UPPER_BOUND:
        raise argparse.ArgumentTypeError(f"{address_hex!r} is out of the contract address range.")
    return address


def split_comma_separated_addresses(args: List[str]) -> List[str]:
    split_args: List[str] = []
    for arg in args:
        if arg.startswith("-"):
            split_args.append(arg)
        else:
            split_args.extend(part for part in arg.split(",") if part.strip())
    return split_args


def parse_args(args: List[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Prints the batcher's blocked_storage_keys config value that blocks access to the "
            "ERC20 balance entries of the given account addresses."
        ),
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument(
        "addresses",
        type=parse_address,
        nargs="+",
        metavar="ADDRESS",
        help="Account addresses in hexadecimal, e.g. 0x1234. Space- or comma-separated.",
    )
    parser.add_argument(
        "--storage-var-name",
        type=str,
        default=DEFAULT_BALANCES_STORAGE_VAR_NAME,
        help="The ERC20 contract's balances storage variable name.",
    )
    parser.add_argument(
        "--include-high-word",
        action="store_true",
        help="Also emit the second slot of each balance, holding the u256 high word.",
    )
    return parser.parse_args(split_comma_separated_addresses(args))


def main(args: List[str]) -> None:
    parsed_args = parse_args(args)
    storage_keys: List[int] = []
    for address in parsed_args.addresses:
        storage_keys.extend(
            balance_storage_keys(
                address, parsed_args.storage_var_name, parsed_args.include_high_word
            )
        )
    print(",".join(hex(storage_key) for storage_key in storage_keys))


if __name__ == "__main__":
    main(sys.argv[1:])
