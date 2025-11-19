from dataclasses import dataclass
from typing import List, Optional

import requests

ALCHEMY_URL = "https://eth-mainnet.g.alchemy.com/v2/fSw5uvMqdG7d2Y6cwexg7"


@dataclass
class Log:
    """
    Ethereum log entry
    """

    address: str
    topics: List[str]
    data: str
    block_number: int
    block_hash: str
    transaction_hash: str
    transaction_index: int
    log_index: int
    removed: bool
    block_timestamp: int


def get_logs(from_block: int, to_block: int, rpc_url: str = ALCHEMY_URL) -> List[Log]:
    """
    Get logs from Ethereum using eth_getLogs RPC method.
    """
    payload = {
        "jsonrpc": "2.0",
        "method": "eth_getLogs",
        "params": [
            {
                "fromBlock": hex(from_block),
                "toBlock": hex(to_block),
                "address": "0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4",  # Starknet L1 contract
                "topics": [
                    "0xdb80dd488acf86d17c747445b0eabb5d57c541d3bd7b6b87af987858e5066b2b"  # LogMessageToL2 event signature
                ],
            }
        ],
        "id": 1,
    }

    response = requests.post(rpc_url, json=payload, timeout=10)
    response.raise_for_status()
    data = response.json()

    results = data.get("result", [])
    logs = []

    for result in results:
        log = Log(
            address=result["address"],
            topics=result["topics"],
            data=result["data"],
            block_number=int(result["blockNumber"], 16),
            block_hash=result["blockHash"],
            transaction_hash=result["transactionHash"],
            transaction_index=int(result["transactionIndex"], 16),
            log_index=int(result["logIndex"], 16),
            removed=result["removed"],
            block_timestamp=int(result["blockTimestamp"], 16),
        )
        logs.append(log)

    return logs


def get_timestamp_of_block_by_number(
    block_number: int, rpc_url: str = ALCHEMY_URL
) -> Optional[int]:
    """
    Get block timestamp by block number using eth_getBlockByNumber RPC method.
    """
    payload = {
        "jsonrpc": "2.0",
        "method": "eth_getBlockByNumber",
        "params": [hex(block_number), False],
        "id": 1,
    }

    response = requests.post(rpc_url, json=payload, timeout=10)
    response.raise_for_status()
    result = response.json()

    block = result.get("result")
    if block is None:
        # Block not found
        return None

    # Timestamp is hex string, convert to int.
    return int(block["timestamp"], 16)
