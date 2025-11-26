"""
L1 Event Decoding - mirrors crates/papyrus_base_layer/src/eth_events.rs
"""

from dataclasses import dataclass
from typing import List

import eth_abi
from l1_client import L1Client
from l1_constants import LOG_MESSAGE_TO_L2_EVENT_SIGNATURE


class L1Events:
    @dataclass(frozen=True)
    class DecodedLogMessageToL2:
        from_address: str
        to_address: str
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

        tx: "L1Events.L1HandlerTransaction"
        fee: int
        l1_tx_hash: str
        block_timestamp: int

    def decode_log(log: L1Client.Log) -> DecodedLogMessageToL2:
        """
        Decodes Ethereum log from Starknet L1 contract into DecodedLogMessageToL2 event.
        Event structure defined in: crates/papyrus_base_layer/resources/Starknet-0.10.3.4.json
        """
        topics = log.topics
        if len(topics) < 4:
            raise ValueError("Log has no topics or insufficient topics for LogMessageToL2 event")
        event_signature = topics[0]
        if event_signature != LOG_MESSAGE_TO_L2_EVENT_SIGNATURE:
            raise ValueError(f"Unhandled event signature: {event_signature}")

        # Indexed params (topics): fromAddress, toAddress, selector
        from_address = hex(int(topics[1], 16))
        to_address = hex(int(topics[2], 16))
        selector = int(topics[3], 16)

        # Non-indexed params (data): payload[], nonce, fee
        if not log.data.startswith("0x"):
            raise ValueError("Log data must start with '0x'")
        data_bytes = bytes.fromhex(log.data[2:])  # Remove 0x prefix and convert to bytes
        payload, nonce, fee = eth_abi.decode(["uint256[]", "uint256", "uint256"], data_bytes)

        return L1Events.DecodedLogMessageToL2(
            from_address=from_address,
            to_address=to_address,
            selector=selector,
            payload=list(payload),
            nonce=nonce,
            fee=fee,
            l1_tx_hash=log.transaction_hash,
            block_timestamp=log.block_timestamp,
        )

    @staticmethod
    def parse_event(log: L1Client.Log) -> "L1Events.L1Event":
        decoded = L1Events.decode_log(log)
        calldata = [int(decoded.from_address, 16)] + decoded.payload

        tx = L1Events.L1HandlerTransaction(
            contract_address=decoded.to_address,
            entry_point_selector=decoded.selector,
            calldata=calldata,
            nonce=decoded.nonce,
        )

        return L1Events.L1Event(
            tx=tx,
            fee=decoded.fee,
            l1_tx_hash=decoded.l1_tx_hash,
            block_timestamp=decoded.block_timestamp,
        )

    def l1_event_matches_feeder_tx(l1_event: L1Event, feeder_tx: dict) -> bool:
        """
        Compares L1Event with an L1_HANDLER feeder tx using only contract_address, entry_point_selector, nonce, and calldata.
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
