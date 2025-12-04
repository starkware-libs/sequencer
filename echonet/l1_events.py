"""
L1 Event Decoding - mirrors crates/papyrus_base_layer/src/eth_events.rs
"""

from dataclasses import dataclass
from typing import List

import eth_abi
from l1_constants import LOG_MESSAGE_TO_L2_EVENT_SIGNATURE


class L1Events:
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

    @staticmethod
    def decode_log(log: dict) -> "L1Events.L1Event":
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

        calldata = [int(from_address, 16)] + list(payload)

        return L1Events.L1Event(
            contract_address=to_address,
            entry_point_selector=selector,
            calldata=calldata,
            nonce=nonce,
            fee=fee,
            l1_tx_hash=log["transactionHash"],
            block_timestamp=int(log["blockTimestamp"], 16),
            block_number=int(log["blockNumber"], 16),
        )
