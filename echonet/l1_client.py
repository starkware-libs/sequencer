from dataclasses import dataclass
from typing import List, Optional

import logging
import requests

logger = logging.getLogger(__name__)


class L1Client:
    L1_MAINNET_URL = "https://eth-mainnet.g.alchemy.com/v2/{api_key}"
    RETRIES_COUNT = 2

    @dataclass(frozen=True)
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

    @staticmethod
    def get_logs(from_block: int, to_block: int, api_key: str) -> List["L1Client.Log"]:
        """
        Get logs from Ethereum using eth_getLogs RPC method.
        Tries up to RETRIES_COUNT times. On failure, logs an error and returns [].
        """
        if from_block > to_block:
            raise ValueError("from_block must be less than or equal to to_block")

        rpc_url = L1Client.L1_MAINNET_URL.format(api_key=api_key)

        payload = {
            "jsonrpc": "2.0",
            "method": "eth_getLogs",
            "params": [
                {
                    "fromBlock": hex(from_block),
                    "toBlock": hex(to_block),
                    "address": "0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4",  # Starknet L1 contract
                    "topics": [
                        "0xdb80dd488acf86d17c747445b0eabb5d57c541d3bd7b6b87af987858e5066b2b"
                        # LogMessageToL2 event signature
                    ],
                }
            ],
            "id": 1,
        }

        for attempt in range(L1Client.RETRIES_COUNT):
            try:
                response = requests.post(rpc_url, json=payload, timeout=10)
                response.raise_for_status()
                data = response.json()
                logger.debug(
                    f"get_logs succeeded on attempt {attempt + 1}",
                    extra={"url": rpc_url, "from_block": from_block, "to_block": to_block},
                )
                break
            except (requests.RequestException, ValueError):
                logger.debug(
                    f"get_logs attempt {attempt + 1}/{L1Client.RETRIES_COUNT} failed",
                    extra={"url": rpc_url, "from_block": from_block, "to_block": to_block},
                    exc_info=True,
                )
        else:
            logger.error(
                f"get_logs failed after {L1Client.RETRIES_COUNT} attempts",
                extra={"url": rpc_url, "from_block": from_block, "to_block": to_block},
            )
            return []

        results = data.get("result", [])

        return [
            L1Client.Log(
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
            for result in results
        ]

    @staticmethod
    def get_timestamp_of_block(block_number: int, api_key: str) -> Optional[int]:
        """
        Get block timestamp by block number using eth_getBlockByNumber RPC method.
        Tries up to RETRIES_COUNT times. On failure, logs an error and returns None.
        """
        rpc_url = L1Client.L1_MAINNET_URL.format(api_key=api_key)

        payload = {
            "jsonrpc": "2.0",
            "method": "eth_getBlockByNumber",
            "params": [hex(block_number), False],
            "id": 1,
        }

        for attempt in range(L1Client.RETRIES_COUNT):
            try:
                response = requests.post(rpc_url, json=payload, timeout=10)
                response.raise_for_status()
                result = response.json()
                logger.debug(
                    f"get_timestamp_of_block succeeded on attempt {attempt + 1}",
                    extra={"url": rpc_url, "block_number": block_number},
                )
                break  # success -> exit loop
            except (requests.RequestException, ValueError) as exc:
                logger.debug(
                    f"get_timestamp_of_block attempt {attempt + 1}/{L1Client.RETRIES_COUNT} failed",
                    extra={"url": rpc_url, "block_number": block_number},
                    exc_info=True,
                )

        else:
            logger.error(
                f"get_timestamp_of_block failed after {L1Client.RETRIES_COUNT} attempts, returning None",
                extra={"url": rpc_url, "block_number": block_number},
            )
            return None

        block = result.get("result")
        if block is None:
            # Block not found
            return None

        # Timestamp is hex string, convert to int.
        return int(block["timestamp"], 16)
