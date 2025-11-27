from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any, Callable, Dict, List, Optional

import functools
import inspect
import logging
import requests
from l1_constants import LOG_MESSAGE_TO_L2_EVENT_SIGNATURE, STARKNET_L1_CONTRACT_ADDRESS


class L1Client:
    L1_MAINNET_URL = "https://eth-mainnet.g.alchemy.com/v2/{api_key}"
    DATA_BLOCKS_BY_TIMESTAMP_URL_FMT = (
        "https://api.g.alchemy.com/data/v1/{api_key}/utility/blocks/by-timestamp"
    )

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

    def __init__(
        self,
        api_key: str,
        timeout: int = 10,
        retries_count: int = 2,
    ):
        self.api_key = api_key
        self.logger = logging.Logger("L1Client")
        self.timeout = timeout
        self.retries_count = retries_count
        self.rpc_url = self.L1_MAINNET_URL.format(api_key=api_key)
        self.data_api_url = self.DATA_BLOCKS_BY_TIMESTAMP_URL_FMT.format(api_key=api_key)

    def _run_request_with_retry(
        self,
        request_func: Callable,
        additional_log_context: Dict[str, Any],
    ) -> Optional[Dict]:
        caller_name = inspect.currentframe().f_back.f_code.co_name

        for attempt in range(self.retries_count):
            try:
                response = request_func(timeout=self.timeout)
                response.raise_for_status()
                result = response.json()
                self.logger.debug(
                    f"{caller_name} succeeded on attempt {attempt + 1}",
                    extra=additional_log_context,
                )
                return result
            except (requests.RequestException, ValueError):
                self.logger.debug(
                    f"{caller_name} attempt {attempt + 1}/{self.retries_count} failed",
                    extra=additional_log_context,
                    exc_info=True,
                )

        self.logger.error(
            f"{caller_name} failed after {self.retries_count} attempts, returning None",
            extra=additional_log_context,
        )

        return None

    def get_logs(self, from_block: int, to_block: int) -> List["L1Client.Log"]:
        """
        Get logs from Ethereum using eth_getLogs RPC method.
        Tries up to retries_count times. On failure, logs an error and returns [].
        """
        if from_block > to_block:
            raise ValueError("from_block must be less than or equal to to_block")

        payload = {
            "jsonrpc": "2.0",
            "method": "eth_getLogs",
            "params": [
                {
                    "fromBlock": hex(from_block),
                    "toBlock": hex(to_block),
                    "address": STARKNET_L1_CONTRACT_ADDRESS,
                    "topics": [LOG_MESSAGE_TO_L2_EVENT_SIGNATURE],
                }
            ],
            "id": 1,
        }

        request_func = functools.partial(requests.post, self.rpc_url, json=payload)
        data = self._run_request_with_retry(
            request_func=request_func,
            additional_log_context={
                "url": self.rpc_url,
                "from_block": from_block,
                "to_block": to_block,
            },
        )

        if data is None:
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

    def get_timestamp_of_block(self, block_number: int) -> Optional[int]:
        """
        Get block timestamp by block number using eth_getBlockByNumber RPC method.
        Tries up to retries_count times. On failure, logs an error and returns None.
        """
        payload = {
            "jsonrpc": "2.0",
            "method": "eth_getBlockByNumber",
            "params": [hex(block_number), False],
            "id": 1,
        }

        request_func = functools.partial(requests.post, self.rpc_url, json=payload)
        result = self._run_request_with_retry(
            request_func=request_func,
            additional_log_context={"url": self.rpc_url, "block_number": block_number},
        )

        if result is None:
            return None

        block = result.get("result")
        if block is None:
            # Block not found
            return None

        # Timestamp is hex string, convert to int.
        return int(block["timestamp"], 16)

    def get_block_number_by_timestamp(self, timestamp: int) -> Optional[int]:
        """
        Get the block number at/after a given timestamp using blocks-by-timestamp API.
        Tries up to retries_count times. On failure, logs an error and returns None.
        """
        timestamp_iso = (
            datetime.fromtimestamp(timestamp, tz=timezone.utc).isoformat().replace("+00:00", "Z")
        )

        params = {
            "networks": "eth-mainnet",
            "timestamp": timestamp_iso,
            "direction": "AFTER",
        }

        request_func = functools.partial(requests.get, self.data_api_url, params=params)
        data = self._run_request_with_retry(
            request_func=request_func,
            additional_log_context={"url": self.data_api_url, "timestamp": timestamp},
        )

        if data is None:
            return None

        items = data.get("data", [])
        if not items:
            return None

        block = items[0].get("block", {})
        return block.get("number")
