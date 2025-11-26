"""
L1 Event Decoding - mirrors crates/papyrus_base_layer/src/eth_events.rs
"""

from dataclasses import dataclass
from typing import List

from eth_abi import decode
from l1_constants import LOG_MESSAGE_TO_L2_EVENT_SIGNATURE


@dataclass
class DecodedLogMessageToL2:
    from_address: str
    to_address: int
    selector: int
    payload: List[int]
    nonce: int
    fee: int


def decode_log(log: dict) -> DecodedLogMessageToL2:
    """
    Decodes Ethereum log from Starknet L1 contract into DecodedLogMessageToL2 event.
    Event structure defined in: crates/papyrus_base_layer/resources/Starknet-0.10.3.4.json
    """
    if not log.get("topics") or len(log["topics"]) == 0:
        raise ValueError("Log has no topics")

    event_signature = log["topics"][0]
    if event_signature != LOG_MESSAGE_TO_L2_EVENT_SIGNATURE:
        raise ValueError(f"Unhandled event signature: {event_signature}")

    # Indexed params (topics): fromAddress, toAddress, selector
    from_address = "0x" + log["topics"][1][-40:]  # Eth address is 20 bytes, topics padded to 32
    to_address = int(log["topics"][2], 16)
    selector = int(log["topics"][3], 16)

    # Non-indexed params (data): payload[], nonce, fee
    data_bytes = bytes.fromhex(log["data"][2:])  # Remove 0x prefix and convert to bytes
    payload, nonce, fee = decode(["uint256[]", "uint256", "uint256"], data_bytes)

    return DecodedLogMessageToL2(
        from_address=from_address,
        to_address=to_address,
        selector=selector,
        payload=list(payload),
        nonce=nonce,
        fee=fee,
    )
