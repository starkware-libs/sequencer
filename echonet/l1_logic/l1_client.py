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
from echonet.helpers import (
    timestamp_to_iso,
)


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
        Tries up to retries_count times. On failure, logs an error and returns None.
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
        return self._run_request_with_retry(
            request_func=request_func,
            additional_log_context={
                "url": self.rpc_url,
                "from_block": from_block,
                "to_block": to_block,
            },
        )

    def get_block_by_number(self, block_number: str) -> Optional[Dict]:
        """
        Get block details by block number using eth_getBlockByNumber RPC method.
        Tries up to retries_count times. On failure, logs an error and returns None.
        """
        payload = {
            "jsonrpc": "2.0",
            "method": "eth_getBlockByNumber",
            "params": [block_number, False],
            "id": 1,
        }

        request_func = functools.partial(requests.post, self.rpc_url, json=payload)
        return self._run_request_with_retry(
            request_func=request_func,
            additional_log_context={"url": self.rpc_url, "block_number": block_number},
        )

    def get_timestamp_of_block(self, block_number: str) -> Optional[int]:
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

    def get_block_number_by_timestamp(self, timestamp: int) -> Optional[int]:
        """
        Get the block number at/after a given timestamp using blocks-by-timestamp API.
        Tries up to retries_count times. On failure, logs an error and returns None.
        """
        timestamp_iso = timestamp_to_iso(timestamp)

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
