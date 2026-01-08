#!/usr/bin/env python3
"""
Script to declare account_with_dummy_validate contract and deploy a new account on mainnet.

Usage:
    python scripts/deploy_account_mainnet.py --rpc-url <RPC_URL>

Environment variables:
    NODE_URL: RPC URL (alternative to --rpc-url)
    ACCOUNT_PRIVATE_KEY: Private key of existing account (defaults to provided key)
"""

import argparse
import json
import os
import sys
from pathlib import Path

try:
    from starknet_py.net.account.account import Account
    from starknet_py.net.client_models import Call
    from starknet_py.net.full_node_client import FullNodeClient
    from starknet_py.net.models import StarknetChainId
    from starknet_py.net.signer.stark_curve_signer import KeyPair, StarkCurveSigner
    from starknet_py.hash.address import compute_address
    from starknet_py.hash.class_hash import compute_class_hash
    from starknet_py.net.models import AddressRepresentation, parse_address
    from starknet_py.cairo.felt import encode_shortstring
    from starknet_py.net.client_errors import ClientError
except ImportError:
    print("Error: starknet-py is not installed. Install it with: pip install starknet-py")
    sys.exit(1)


# Constants from contracts.rs
ACCOUNT_WITHOUT_VALIDATIONS_COMPILED_CLASS_HASH_V2 = 0x2fa9a4b44d4c9c0b5522b50fd0ec55fb78f1db356837e33f6ddda1cfe6e1b71
STRK_TOKEN_ADDRESS = 0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d

# Default account details
DEFAULT_ACCOUNT_PRIVATE_KEY = 0x074ccac3fac4584254427a0d23dbd6eec6a0f5d107921b49f435580e7b552749
DEFAULT_ACCOUNT_ADDRESS = 0x07f2f71bebfd9021684fcbcb954a37450febef5f3649ac6228e0c76c4f8819c4

# Amount to fund (5 STRK = 5 * 10^18, but STRK uses 18 decimals)
FUND_AMOUNT = 5 * 10**18


def load_sierra_json(sierra_path: Path) -> dict:
    """Load and return the Sierra JSON file."""
    with open(sierra_path, 'r') as f:
        return json.load(f)


def calculate_class_hash_from_sierra(sierra_json: dict) -> int:
    """Calculate class hash from Sierra JSON using starknet-py."""
    # Convert Sierra JSON to the format expected by compute_class_hash
    # The function expects a ContractClass object, but we can use the dict directly
    # by converting it to the proper format
    contract_class = {
        "abi": sierra_json.get("abi", []),
        "sierra_program": sierra_json["sierra_program"],
        "contract_class_version": sierra_json.get("contract_class_version", "0.1.0"),
        "entry_points_by_type": sierra_json["entry_points_by_type"],
    }
    
    # Compute class hash
    class_hash = compute_class_hash(contract_class)
    return class_hash


def get_project_root() -> Path:
    """Get the project root directory."""
    script_dir = Path(__file__).parent
    return script_dir.parent


async def main():
    parser = argparse.ArgumentParser(description="Declare and deploy account contract on mainnet")
    parser.add_argument(
        "--rpc-url",
        type=str,
        default=os.getenv("NODE_URL"),
        help="Pathfinder RPC URL (or set NODE_URL env var)",
    )
    parser.add_argument(
        "--account-key",
        type=str,
        default=os.getenv("ACCOUNT_PRIVATE_KEY", hex(DEFAULT_ACCOUNT_PRIVATE_KEY)),
        help="Private key of existing account (hex string)",
    )
    parser.add_argument(
        "--account-address",
        type=str,
        default=hex(DEFAULT_ACCOUNT_ADDRESS),
        help="Address of existing account (hex string)",
    )
    parser.add_argument(
        "--salt",
        type=str,
        default=None,
        help="Salt for new account address (hex string, random if not provided)",
    )
    
    args = parser.parse_args()
    
    if not args.rpc_url:
        print("Error: RPC URL is required. Use --rpc-url or set NODE_URL environment variable.")
        sys.exit(1)
    
    # Load Sierra JSON
    project_root = get_project_root()
    sierra_path = project_root / "crates" / "blockifier_test_utils" / "resources" / "feature_contracts" / "cairo1" / "sierra" / "account_with_dummy_validate.sierra.json"
    
    if not sierra_path.exists():
        print(f"Error: Sierra file not found at {sierra_path}")
        sys.exit(1)
    
    print(f"Loading Sierra contract from {sierra_path}")
    sierra_json = load_sierra_json(sierra_path)
    
    # Initialize client
    print(f"Connecting to RPC: {args.rpc_url}")
    client = FullNodeClient(node_url=args.rpc_url)
    
    # Parse account details
    account_private_key = int(args.account_key, 16)
    account_address = parse_address(args.account_address)
    
    # Create account signer
    key_pair = KeyPair.from_private_key(account_private_key)
    signer = StarkCurveSigner(account_address, key_pair, StarknetChainId.MAINNET)
    account = Account(client=client, address=account_address, signer=signer)
    
    print(f"Using account: {hex(account_address)}")
    
    # Calculate class hash
    print("Calculating class hash from Sierra...")
    class_hash = calculate_class_hash_from_sierra(sierra_json)
    print(f"Class hash: {hex(class_hash)}")
    
    # Check if class is already declared
    print("Checking if class is already declared...")
    try:
        declared_class = await client.get_class_by_hash(class_hash)
        print(f"✓ Class already declared at hash {hex(class_hash)}")
    except ClientError:
        print("Class not declared yet. Declaring...")
        
        # Declare the contract
        declare_result = await account.declare(
            compiled_contract=sierra_json,
            compiled_class_hash=ACCOUNT_WITHOUT_VALIDATIONS_COMPILED_CLASS_HASH_V2,
            max_fee=int(1e15),  # 0.001 STRK
        )
        
        print(f"Declare transaction sent: {hex(declare_result.transaction_hash)}")
        print("Waiting for transaction to be accepted...")
        
        try:
            await client.wait_for_tx(declare_result.transaction_hash, wait_for_accept=True)
            print("✓ Declaration successful!")
        except Exception as e:
            print(f"Error waiting for declaration: {e}")
            print(f"Transaction hash: {hex(declare_result.transaction_hash)}")
            print("Please check the transaction status manually.")
            sys.exit(1)
    
    # Generate new keypair for the new account
    print("\nGenerating new account keypair...")
    new_key_pair = KeyPair.from_random()
    new_private_key = new_key_pair.private_key
    print(f"New private key: {hex(new_private_key)}")
    
    # Calculate new account address
    salt = int(args.salt, 16) if args.salt else int.from_bytes(os.urandom(32), 'big')
    print(f"Using salt: {hex(salt)}")
    
    # Constructor calldata is empty for this account
    constructor_calldata = []
    
    print("Calculating new account address...")
    new_account_address = compute_address(
        class_hash=class_hash,
        constructor_calldata=constructor_calldata,
        salt=salt,
        deployer_address=0,  # Deploy account uses 0 as deployer
    )
    print(f"New account address: {hex(new_account_address)}")
    
    # Check balance of existing account
    print(f"\nChecking STRK balance of funding account...")
    try:
        balance_call = Call(
            to_addr=STRK_TOKEN_ADDRESS,
            selector=encode_shortstring("balanceOf"),
            calldata=[account_address],
        )
        balance_response = await client.call_contract(call=balance_call, block_number="latest")
        current_balance = balance_response[0] if isinstance(balance_response, (list, tuple)) else balance_response
        print(f"Current STRK balance: {current_balance / 10**18:.6f} STRK")
        
        if current_balance < FUND_AMOUNT:
            print(f"Warning: Account balance ({current_balance / 10**18:.6f} STRK) is less than funding amount (5 STRK)")
    except Exception as e:
        print(f"Warning: Could not check balance: {e}")
    
    # Transfer 5 STRK to the new account
    print(f"\nTransferring 5 STRK to new account {hex(new_account_address)}...")
    transfer_call = Call(
        to_addr=STRK_TOKEN_ADDRESS,
        selector=encode_shortstring("transfer"),
        calldata=[
            new_account_address,  # recipient
            FUND_AMOUNT & ((1 << 128) - 1),  # amount_low (low 128 bits)
            FUND_AMOUNT >> 128,  # amount_high (high 128 bits)
        ],
    )
    
    try:
        transfer_invoke = await account.execute(calls=[transfer_call], max_fee=int(1e15))
        print(f"Transfer transaction sent: {hex(transfer_invoke.transaction_hash)}")
        print("Waiting for transaction to be accepted...")
        
        await client.wait_for_tx(transfer_invoke.transaction_hash, wait_for_accept=True)
        print("✓ Transfer successful!")
    except Exception as e:
        print(f"Error during transfer: {e}")
        print("Please transfer funds manually and then deploy the account.")
        sys.exit(1)
    
    # Wait a bit for the transfer to be included
    import asyncio
    await asyncio.sleep(2)
    
    # Deploy the account
    print(f"\nDeploying new account...")
    new_account_signer = StarkCurveSigner(new_account_address, new_key_pair, StarknetChainId.MAINNET)
    new_account = Account(client=client, address=new_account_address, signer=new_account_signer)
    
    try:
        deploy_result = await new_account.deploy_account(
            class_hash=class_hash,
            salt=salt,
            constructor_calldata=constructor_calldata,
            max_fee=int(1e15),
        )
        
        print(f"Deploy account transaction sent: {hex(deploy_result.transaction_hash)}")
        print("Waiting for transaction to be accepted...")
        
        await client.wait_for_tx(deploy_result.transaction_hash, wait_for_accept=True)
        print("✓ Account deployment successful!")
    except Exception as e:
        print(f"Error during deployment: {e}")
        print(f"Transaction hash: {hex(deploy_result.transaction_hash) if 'deploy_result' in locals() else 'N/A'}")
        sys.exit(1)
    
    # Print summary
    print("\n" + "="*60)
    print("DEPLOYMENT SUMMARY")
    print("="*60)
    print(f"Class hash: {hex(class_hash)}")
    print(f"New account address: {hex(new_account_address)}")
    print(f"New account private key: {hex(new_private_key)}")
    print(f"Salt used: {hex(salt)}")
    print("="*60)
    print("\nSave these credentials securely!")
    print(f"\nTo use in tests, set:")
    print(f"  NEW_ACCOUNT_ADDRESS={hex(new_account_address)}")
    print(f"  NEW_ACCOUNT_PRIVATE_KEY={hex(new_private_key)}")


if __name__ == "__main__":
    import asyncio
    asyncio.run(main())
