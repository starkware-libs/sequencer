"""
L1 Event Decoding - mirrors crates/papyrus_base_layer/src/eth_events.rs
"""

from dataclasses import dataclass
from typing import List

from eth_abi import decode
from l1_constants import LOG_MESSAGE_TO_L2_EVENT_SIGNATURE


@dataclass(frozen=True)
class DecodedLogMessageToL2:
    from_address: str
    to_address: int
    selector: int
    payload: List[int]
    nonce: int
    fee: int
    l1_tx_hash: str
    block_timestamp: int


@dataclass(frozen=True)
class L1HandlerTransaction:
    """Mirrors Rust starknet_api::transaction::L1HandlerTransaction"""

    contract_address: int
    entry_point_selector: int
    calldata: List[int]
    nonce: int


@dataclass(frozen=True)
class L1Event:
    """Mirrors Rust papyrus_base_layer::events::L1Event"""

    tx: L1HandlerTransaction
    fee: int
    l1_tx_hash: str
    block_timestamp: int


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
        l1_tx_hash=log["transactionHash"],
        block_timestamp=int(log["blockTimestamp"], 16),
    )


def parse_event(log: dict) -> L1Event:
    decoded = decode_log(log)

    tx = L1HandlerTransaction(
        contract_address=decoded.to_address,
        entry_point_selector=decoded.selector,
        calldata=decoded.payload,
        nonce=decoded.nonce,
    )

    return L1Event(
        tx=tx,
        fee=decoded.fee,
        l1_tx_hash=decoded.l1_tx_hash,
        block_timestamp=decoded.block_timestamp,
    )


def l1_event_matches_feeder_tx(l1_event: L1Event, feeder_tx: dict) -> bool:
    """
    Compare L1Event with an L1_HANDLER feeder tx using only contract_address, entry_point_selector, nonce, and calldata.
    Transaction hashes are ignored.
    """
    if feeder_tx.get("type") != "L1_HANDLER":
        return False

    feeder_contract = int(feeder_tx["contract_address"], 16)
    if l1_event.tx.contract_address != feeder_contract:
        return False

    feeder_selector = int(feeder_tx["entry_point_selector"], 16)
    if l1_event.tx.entry_point_selector != feeder_selector:
        return False

    feeder_nonce = int(feeder_tx["nonce"], 16)
    if l1_event.tx.nonce != feeder_nonce:
        return False

    feeder_calldata = [int(item, 16) for item in feeder_tx["calldata"]]
    if l1_event.tx.calldata != feeder_calldata:
        return False

    return True
