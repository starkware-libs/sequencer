from dataclasses import dataclass
from datetime import datetime, timezone
from typing import List, Optional

import logging
import requests

logger = logging.getLogger(__name__)


class L1Client:
    L1_MAINNET_URL = "https://eth-mainnet.g.alchemy.com/v2/{api_key}"
    DATA_BLOCKS_BY_TIMESTAMP_URL_FMT = (
        "https://api.g.alchemy.com/data/v1/{api_key}/utility/blocks/by-timestamp"
    )
    # Taken from apollo_l1_provider/src/lib.rs
    LOG_MESSAGE_TO_L2_EVENT_SIGNATURE = (
        "0xdb80dd488acf86d17c747445b0eabb5d57c541d3bd7b6b87af987858e5066b2b"
    )
    # Taken from ethereum_base_layer_contracts.rs
    STARKNET_L1_CONTRACT_ADDRESS = "0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4"
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
                    "address": L1Client.STARKNET_L1_CONTRACT_ADDRESS,
                    "topics": [L1Client.LOG_MESSAGE_TO_L2_EVENT_SIGNATURE],
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
                f"get_logs failed after {L1Client.RETRIES_COUNT} attempts, returning []",
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

    @staticmethod
    def get_block_number_by_timestamp(timestamp: int, api_key: str) -> Optional[int]:
        """
        Get the block number at/after a given timestamp using blocks-by-timestamp API.
        Tries up to RETRIES_COUNT times. On failure, logs an error and returns None.
        """
        rpc_url = L1Client.DATA_BLOCKS_BY_TIMESTAMP_URL_FMT.format(api_key=api_key)

        timestamp_iso = (
            datetime.fromtimestamp(timestamp, tz=timezone.utc).isoformat().replace("+00:00", "Z")
        )

        params = {
            "networks": "eth-mainnet",
            "timestamp": timestamp_iso,
            "direction": "AFTER",
        }

        for attempt in range(L1Client.RETRIES_COUNT):
            try:
                response = requests.get(rpc_url, params=params, timeout=10)
                response.raise_for_status()
                data = response.json()
                logger.debug(
                    f"get_block_number_by_timestamp succeeded on attempt {attempt + 1}",
                    extra={"url": rpc_url, "timestamp": timestamp},
                )
                break  # success -> exit loop
            except (requests.RequestException, ValueError) as exc:
                logger.debug(
                    f"get_block_number_by_timestamp attempt {attempt + 1}/{L1Client.RETRIES_COUNT} failed",
                    extra={"url": rpc_url, "timestamp": timestamp},
                    exc_info=True,
                )
        else:
            logger.error(
                f"get_block_number_by_timestamp failed after {L1Client.RETRIES_COUNT} attempts, returning None",
                extra={"url": rpc_url, "timestamp": timestamp},
            )
            return None

        items = data.get("data", [])
        if not items:
            return None

        block = items[0].get("block", {})
        return block.get("number")
