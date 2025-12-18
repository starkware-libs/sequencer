from dataclasses import dataclass
from typing import Any, Callable, Dict, List, Optional

import eth_abi
import functools
import inspect
import logging
import requests

from echonet.constants import (
    LOG_MESSAGE_TO_L2_EVENT_SIGNATURE,
    STARKNET_L1_CONTRACT_ADDRESS,
)
from echonet.helpers import rpc_response


class L1ClientCache:
    def __init__(self):
        self.block_cache: dict[int, Dict] = {}
        self.logs_cache: dict[int, list[dict]] = {}

    def get_block(self, block_number: int) -> Optional[Dict]:
        return self.block_cache.get(block_number)

    def set_block(self, block_number: int, response: Optional[Dict]) -> None:
        if response is None:
            return

        self.block_cache[block_number] = response

    def get_logs(self, from_block: int, to_block: int) -> tuple[list[dict], Optional[int]]:
        cached_logs = []
        first_missing_block = None

        for block_num in range(from_block, to_block + 1):
            logs = self.logs_cache.get(block_num)
            if logs is None:
                first_missing_block = block_num
                break
            cached_logs.extend(logs)

        return cached_logs, first_missing_block

    def set_logs(self, response: Optional[Dict]) -> None:
        if response is None:
            return

        logs = response.get("result", [])
        for log in logs:
            block_number = int(log.get("blockNumber"), 16)
            self.logs_cache.setdefault(block_number, []).append(log)


class L1Client:
    L1_MAINNET_URL = "https://eth-mainnet.g.alchemy.com/v2/{api_key}"
    DATA_BLOCKS_BY_TIMESTAMP_URL_FMT = (
        "https://api.g.alchemy.com/data/v1/{api_key}/utility/blocks/by-timestamp"
    )

    @dataclass(frozen=True)
    class L1Event:
        contract_address: str
        entry_point_selector: int
        calldata: List[int]
        nonce: int
        fee: int
        l1_tx_hash: str
        block_number: int
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
        self.cache = L1ClientCache()

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

    def get_logs(self, from_block: int, to_block: int) -> Optional[Dict]:
        """
        Get logs from Ethereum using eth_getLogs RPC method.
        Caches results to avoid redundant API calls, fetches only missing logs.
        INVARIANT: missing blocks are always newer.
        Tries up to retries_count times. On failure, logs an error and returns None.
        """
        if from_block > to_block:
            raise ValueError("from_block must be less than or equal to to_block")

        cached_logs, first_missing_block = self.cache.get_logs(from_block, to_block)
        if first_missing_block is None:
            # All blocks cached, return cached response.
            return rpc_response(cached_logs)

        payload = {
            "jsonrpc": "2.0",
            "method": "eth_getLogs",
            "params": [
                {
                    "fromBlock": hex(first_missing_block),
                    "toBlock": hex(to_block),
                    "address": STARKNET_L1_CONTRACT_ADDRESS,
                    "topics": [LOG_MESSAGE_TO_L2_EVENT_SIGNATURE],
                }
            ],
            "id": 1,
        }

        request_func = functools.partial(requests.post, self.rpc_url, json=payload)
        response = self._run_request_with_retry(
            request_func=request_func,
            additional_log_context={
                "url": self.rpc_url,
                "from_block": first_missing_block,
                "to_block": to_block,
            },
        )

        if response is None:
            return None

        self.cache.set_logs(response)

        fetched_logs = response.get("result", [])
        all_logs = cached_logs + fetched_logs

        return rpc_response(all_logs)

    def get_block_number(self) -> Optional[Dict]:
        """
        Get the latest block number using eth_blockNumber RPC method.
        Tries up to retries_count times. On failure, logs an error and returns None.
        """
        payload = {
            "jsonrpc": "2.0",
            "method": "eth_blockNumber",
            "params": [],
            "id": 1,
        }

        request_func = functools.partial(requests.post, self.rpc_url, json=payload)
        return self._run_request_with_retry(
            request_func=request_func,
            additional_log_context={"url": self.rpc_url},
        )

    def get_block_by_number(self, block_number: int) -> Optional[Dict]:
        """
        Get block details by block number using eth_getBlockByNumber RPC method.
        Caches results to avoid redundant API calls.
        Tries up to retries_count times. On failure, logs an error and returns None.
        """
        cached = self.cache.get_block(block_number)
        if cached is not None:
            return cached

        payload = {
            "jsonrpc": "2.0",
            "method": "eth_getBlockByNumber",
            "params": [hex(block_number), False],
            "id": 1,
        }
        request_func = functools.partial(requests.post, self.rpc_url, json=payload)
        response = self._run_request_with_retry(
            request_func=request_func,
            additional_log_context={"url": self.rpc_url, "block_number": block_number},
        )
        self.cache.set_block(block_number, response)
        return response

    def get_timestamp_of_block(self, block_number: int) -> Optional[int]:
        """
        Get block timestamp by block number using eth_getBlockByNumber RPC method.
        Tries up to retries_count times. On failure, logs an error and returns None.
        """
        response = self.get_block_by_number(block_number)
        if response is None:
            return None

        block = response.get("result")
        if block is None:
            # Block not found
            return None

        # Timestamp is hex string, convert to int.
        return int(block["timestamp"], 16)

    @staticmethod
    def decode_log_response(log: dict) -> "L1Client.L1Event":
        """
        Decodes Ethereum log from Starknet L1 contract into DecodedLogMessageToL2 event.
        Event structure defined in: crates/papyrus_base_layer/resources/Starknet-0.10.3.4.json
        """
        if not all(
            key in log
            for key in ("topics", "data", "transactionHash", "blockTimestamp", "blockNumber")
        ):
            raise ValueError("Log is missing required fields for decoding")

        topics = log["topics"]
        if len(topics) < 4:
            raise ValueError("Log has insufficient topics for LogMessageToL2 event")
        event_signature = topics[0]
        if event_signature != LOG_MESSAGE_TO_L2_EVENT_SIGNATURE:
            raise ValueError(f"Unhandled event signature: {event_signature}")

        # Indexed params (topics): fromAddress, toAddress, selector
        from_address = hex(int(topics[1], 16))
        to_address = hex(int(topics[2], 16))
        selector = int(topics[3], 16)

        # Non-indexed params (data): payload[], nonce, fee
        data = log["data"]
        if not data.startswith("0x"):
            raise ValueError("Log data must start with '0x'")
        data_bytes = bytes.fromhex(data[2:])  # Remove 0x prefix and convert to bytes
        payload, nonce, fee = eth_abi.decode(["uint256[]", "uint256", "uint256"], data_bytes)

        # Prepend the L1 sender address to the calldata.
        calldata = [int(from_address, 16)] + list(payload)

        return L1Client.L1Event(
            contract_address=to_address,
            entry_point_selector=selector,
            calldata=calldata,
            nonce=nonce,
            fee=fee,
            l1_tx_hash=log["transactionHash"],
            block_timestamp=int(log["blockTimestamp"], 16),
            block_number=int(log["blockNumber"], 16),
        )
