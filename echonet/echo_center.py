import base64
import json
import logging
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, List, Literal, Optional, Tuple, Union

import flask  # pyright: ignore[reportMissingImports]
import requests

from echonet import report_web
from echonet.echonet_types import CONFIG, BlockDumpKind, JsonObject, TxType
from echonet.feeder_client import FeederClient
from echonet.helpers import format_hex
from echonet.l1_logic.l1_manager import L1Manager
from echonet.logger import get_logger
from echonet.shared_context import SharedContext, l1_manager, shared
from echonet.transaction_sender import start_background_sender

from .reports import RevertClassifier, RevertComparisonTextReport, SnapshotTextReport

BlockNumberParam = Union[int, Literal["latest"]]

flask_logger = get_logger("flask")
logger = get_logger("echo_center")


@dataclass(frozen=True, slots=True)
class _BootstrapConstants:
    """
    Values fetched once from the upstream feeder gateway at block_number.

    These are then reused throughout the run (merged into every stored block, and returned
    from /feeder_gateway/get_signature for any stored block + latest).
    """

    block_number: int
    custom_fields: JsonObject
    signature_const: JsonObject


def _fetch_bootstrap_constants(
    feeder_client: FeederClient, block_number: int
) -> _BootstrapConstants:
    """
    Fetch fixed block metadata + signature from the feeder gateway using block_number.
    """
    block = feeder_client.get_block(block_number, with_fee_market_info=True)

    custom_keys = [
        "state_root",
        "transaction_commitment",
        "event_commitment",
        "receipt_commitment",
        "state_diff_length",
        "status",
        "l1_da_mode",
        "l2_gas_consumed",
        "next_l2_gas_price",
        "sequencer_address",
        "starknet_version",
        "state_diff_commitment",
    ]
    custom_fields: JsonObject = {k: block[k] for k in custom_keys}

    signature_const = feeder_client.get_signature(block_number)
    return _BootstrapConstants(
        block_number=int(block_number),
        custom_fields=custom_fields,
        signature_const=signature_const,
    )


@dataclass
class _BaseValues:
    """Base values used for deterministic hashes/roots for stored blocks."""

    base_block_number: int
    base_block_hash_hex: str
    base_state_root_hex: str


class DeterministicChain:
    """
    Deterministic hash/root generation used by echo_center.
    """

    def __init__(
        self, feeder_client: FeederClient, shared_ctx: SharedContext, logger_obj: logging.Logger
    ) -> None:
        self._feeder_client = feeder_client
        self._shared: SharedContext = shared_ctx
        self._logger: logging.Logger = logger_obj
        self._base: Optional[_BaseValues] = None
        self.refresh_base()

    def refresh_base(self) -> None:
        """
        Refresh base values from upstream feeder if needed.

        Base values are derived from (configured_start_block - 1) so that the first
        locally-stored block at configured_start_block has a consistent parent.
        """
        start_block = self._shared.get_current_start_block(
            default_start_block=CONFIG.blocks.start_block
        )
        if self._base and self._base.base_block_number == start_block:
            return

        block = self._feeder_client.get_block(start_block - 1)
        assert block is not None, f"Block {start_block - 1} not found"
        if start_block == 6099201:
            base_block_hash_hex = (
                "0x36499E1BF6F64DE94A4287FEF41E64F5110B54696A6FA1693A9D9D5280AEA9A"
            )
            base_state_root_hex = (
                "0x3a59f3745fa3d868def2669e1658834bbf7ca03891635a089c4b6dfbcda414A"
            )
        else:
            base_block_hash_hex = block["block_hash"]
            base_state_root_hex = block["state_root"]
        self._base = _BaseValues(
            base_block_number=start_block,
            base_block_hash_hex=base_block_hash_hex,
            base_state_root_hex=base_state_root_hex,
        )
        # Used by SharedContext.get_last_proved_block_callback().
        self._shared.set_base_block_hash(base_block_hash_hex)
        self._logger.info(
            f"Initialized deterministic base: base_block_number={start_block} base_block_hash={base_block_hash_hex}"
        )

    def _compute_current_and_previous(self, block_number: int, base_hex: str) -> Tuple[str, str]:
        """
        Shared helper: compute (current, previous) given a base 0x-hex value.

        Both hashes and roots follow the same pattern:
        - previous = base + offset
        - current  = base + offset + 1
        """
        offset = int(block_number) - int(self._base.base_block_number)
        base = int(base_hex, 16)
        previous_int = base + offset
        current_int = base + offset + 1
        return format_hex(current_int), format_hex(previous_int)

    def compute_current_and_previous_hash(self, block_number: int) -> Tuple[str, str]:
        """Return (current_block_hash, parent_block_hash) for `block_number`."""
        self.refresh_base()
        assert self._base is not None
        return self._compute_current_and_previous(block_number, self._base.base_block_hash_hex)

    def compute_current_and_previous_root(self, block_number: int) -> Tuple[str, str]:
        """Return (new_root, old_root) for `block_number`."""
        self.refresh_base()
        assert self._base is not None
        return self._compute_current_and_previous(block_number, self._base.base_state_root_hex)

    @property
    def base_block_number(self) -> int:
        self.refresh_base()
        return self._base.base_block_number


class BlobTransformer:
    """Transforms blobs recieved from the sequencer into block + state_update block documents, formatted as Feeder Gateway outputs."""

    def __init__(
        self,
        feeder_client: FeederClient,
        shared_ctx: SharedContext,
        chain: DeterministicChain,
        custom_fields: JsonObject,
        logger_obj: logging.Logger,
    ) -> None:
        self._feeder_client = feeder_client
        self._shared: SharedContext = shared_ctx
        self._chain = chain
        self._custom_fields: JsonObject = dict(custom_fields)
        self._logger: logging.Logger = logger_obj
        self._latest_block_meta: Optional[JsonObject] = None

    @staticmethod
    def get_blob_tx_hashes(blob: JsonObject) -> List[str]:
        txs = blob["transactions"]
        tx_hashes: List[str] = []
        for entry in txs:
            tx_hashes.append(entry["tx"]["hash_value"])
        return tx_hashes

    @staticmethod
    def _halve_gas_prices(v: str) -> str:
        """By shifting the integer value right by 1, the gas prices are halved."""
        return hex(int(v, 16) >> 1)

    def _with_halved_gas_prices(self, price: JsonObject) -> JsonObject:
        out = dict(price)
        out["price_in_wei"] = self._halve_gas_prices(out["price_in_wei"])
        out["price_in_fri"] = self._halve_gas_prices(out["price_in_fri"])
        return out

    @staticmethod
    @dataclass(slots=True)
    class FlattenedCallInfo:
        """
        Result of flattening a call_info tree.

        - events_with_order: list of (order, event) where event is feeder-style:
          {from_address, keys, data}
        - l2_to_l1_messages: normalized messages in feeder-gateway shape:
          {from_address, to_address, payload}
        """

        events_with_order: List[Tuple[Optional[int], JsonObject]] = field(default_factory=list)
        l2_to_l1_messages: List[JsonObject] = field(default_factory=list)

    @staticmethod
    def _normalize_l2_to_l1_messages(
        from_address: str, raw_messages: List[JsonObject]
    ) -> List[JsonObject]:
        """
        Normalize blob l2->l1 messages into feeder-gateway style objects.
        """
        normalized: List[JsonObject] = []
        for m in raw_messages:
            inner = m["message"]
            normalized.append(
                {
                    "from_address": from_address,
                    "to_address": inner["to_address"],
                    "payload": inner["payload"],
                }
            )
        return normalized

    @staticmethod
    def _flatten_call_info(
        call_info: Optional[JsonObject],
    ) -> "BlobTransformer.FlattenedCallInfo":
        """
        Flatten a call-info tree (validate/execute/fee_transfer call info) into:
        - events: list[(order, {from_address, keys, data})]
        - l2_to_l1_messages: list[message]
        """
        if call_info is None:
            return BlobTransformer.FlattenedCallInfo()

        flat_call_info = BlobTransformer.FlattenedCallInfo()
        from_address = call_info["call"]["storage_address"]

        for exec in call_info["execution"]["events"]:
            flat_call_info.events_with_order.append(
                (
                    exec["order"],
                    {
                        "from_address": from_address,
                        "keys": exec["event"]["keys"],
                        "data": exec["event"]["data"],
                    },
                )
            )

        flat_call_info.l2_to_l1_messages.extend(
            BlobTransformer._normalize_l2_to_l1_messages(
                from_address, call_info["execution"]["l2_to_l1_messages"]
            )
        )

        for inner_call in call_info["inner_calls"]:
            flattened = BlobTransformer._flatten_call_info(inner_call)
            flat_call_info.events_with_order.extend(flattened.events_with_order)
            flat_call_info.l2_to_l1_messages.extend(flattened.l2_to_l1_messages)

        return flat_call_info

    @staticmethod
    def _transform_receipt_from_execution_info(
        tx_index: int, tx_hash: str, execution_info: JsonObject
    ) -> JsonObject:
        """
        Transform a blob execution_infos[i] entry into a feeder gateway type transaction receipt.
        """
        flat_call_info = BlobTransformer.FlattenedCallInfo()

        for key in ("validate_call_info", "execute_call_info", "fee_transfer_call_info"):
            flattened = BlobTransformer._flatten_call_info(execution_info[key])
            flat_call_info.events_with_order.extend(flattened.events_with_order)
            flat_call_info.l2_to_l1_messages.extend(flattened.l2_to_l1_messages)

        # The "order" field is globally assigned per-tx in the blob (when present).
        flat_call_info.events_with_order.sort(
            key=lambda p: (p[0] is None, p[0] if p[0] is not None else 0)
        )
        events = [ev for _order, ev in flat_call_info.events_with_order]

        revert_error = execution_info["revert_error"]
        execution_status = "SUCCEEDED" if revert_error is None else "REVERTED"

        actual_resources = execution_info["actual_resources"]
        da_gas = execution_info["da_gas"]
        total_gas = execution_info["total_gas"]

        builtin_instance_counter = {
            k: v for k, v in actual_resources.items() if k.endswith("_builtin")
        }

        receipt: JsonObject = {
            "execution_status": execution_status,
            "transaction_index": tx_index,
            "transaction_hash": tx_hash,
            "l2_to_l1_messages": flat_call_info.l2_to_l1_messages,
            "events": events,
            "execution_resources": {
                "n_steps": actual_resources["n_steps"],
                "builtin_instance_counter": builtin_instance_counter,
                "n_memory_holes": 0,
                "data_availability": {
                    "l1_gas": da_gas["l1_gas"],
                    "l1_data_gas": da_gas["l1_data_gas"],
                    "l2_gas": da_gas["l2_gas"],
                },
                "total_gas_consumed": {
                    "l1_gas": total_gas["l1_gas"],
                    "l1_data_gas": total_gas["l1_data_gas"],
                    "l2_gas": total_gas["l2_gas"],
                },
            },
            "actual_fee": execution_info["actual_fee"],
        }

        if revert_error:
            receipt["revert_error"] = revert_error

        return receipt

    def _fetch_upstream_block_meta(self, block_number: Optional[int]) -> JsonObject:
        """
        Fetch mainnet timestamp and gas prices for `block_number`.
        (fetched from shared FGW snapshot or upstream feeder gateway)
        """
        if block_number is None:
            if self._latest_block_meta is not None:
                return dict(self._latest_block_meta)
            block_number = self._chain.base_block_number

        obj = self._shared.get_fgw_block(block_number)
        if obj is None:
            obj = self._feeder_client.get_block(block_number)

        meta: JsonObject = {
            "timestamp": obj["timestamp"],
            "l1_gas_price": obj["l1_gas_price"],
            "l1_data_gas_price": obj["l1_data_gas_price"],
            "l2_gas_price": obj["l2_gas_price"],
        }
        self._latest_block_meta = meta
        return meta

    @staticmethod
    def _transform_transactions(tx_entries: List[JsonObject]) -> List[JsonObject]:
        """
        Transform feeder tx entries (each with a `tx` object) into the stored tx schema.

        Intentionally strict: relies on expected keys being present and will raise if not.
        """
        transformed_txs: List[JsonObject] = []
        for entry in tx_entries:
            tx = entry["tx"]
            tx_type = tx["type"]

            if tx_type == TxType.L1_HANDLER:
                tx_obj = {
                    k: tx[k]
                    for k in [
                        "nonce",
                        "contract_address",
                        "entry_point_selector",
                        "calldata",
                        "type",
                    ]
                }
                tx_obj["transaction_hash"] = tx["hash_value"]
                tx_obj["version"] = "0x0"
                transformed_txs.append(tx_obj)
                continue

            pass_through_keys = [
                "version",
                "nonce",
                "nonce_data_availability_mode",
                "fee_data_availability_mode",
                "tip",
                "sender_address",
                "type",
                "signature",
                "paymaster_data",
                "resource_bounds",
            ]
            tx_obj = {k: tx[k] for k in pass_through_keys}
            tx_obj["transaction_hash"] = tx["hash_value"]

            if tx_type == TxType.DEPLOY_ACCOUNT:
                extra_keys = ["contract_address_salt", "class_hash", "constructor_calldata"]
            elif tx_type == TxType.DECLARE:
                extra_keys = ["class_hash", "compiled_class_hash", "account_deployment_data"]
            else:  # invoke
                extra_keys = ["calldata", "account_deployment_data"]

            tx_obj.update({k: tx[k] for k in extra_keys})

            transformed_txs.append(tx_obj)
        return transformed_txs

    def transform_block(self, blob: JsonObject) -> JsonObject:
        """
        Build the stored "block" document from an incoming blob.

        Includes:
        - transactions + receipts (minimal schema expected by downstream consumers)
        - deterministic block hash + parent hash
        - fee market metadata (timestamp + gas prices)
        """
        block_number = int(blob["block_number"])
        tx_entries = blob["transactions"]

        transformed_txs = self._transform_transactions(tx_entries)

        receipts: List[JsonObject] = []
        execution_infos = blob["execution_infos"]
        assert len(execution_infos) == len(
            transformed_txs
        ), f"The number of transactions in the blob does not match the number of execution infos."
        for idx, (tx, execution_info) in enumerate(zip(transformed_txs, execution_infos)):
            receipts.append(
                self._transform_receipt_from_execution_info(
                    tx_index=idx,
                    tx_hash=tx["transaction_hash"],
                    execution_info=execution_info,
                )
            )

        block_document: JsonObject = {
            "block_number": block_number,
            "transactions": transformed_txs,
            "transaction_receipts": receipts,
        }

        (
            block_document["block_hash"],
            block_document["parent_block_hash"],
        ) = self._chain.compute_current_and_previous_hash(block_number)
        # Add custom fields to the block document, constant fields that are the same for all blocks.
        block_document.update(self._custom_fields)

        tx_hashes = self.get_blob_tx_hashes(blob)
        bn_for_meta: Optional[int] = (
            self._shared.get_sent_block_number(tx_hashes[0]) if tx_hashes else None
        )

        meta = self._fetch_upstream_block_meta(bn_for_meta)
        block_document["timestamp"] = meta["timestamp"]

        # The gas prices are halved in order for txs to pass the fee sequencer checks.
        for price in ("l1_gas_price", "l1_data_gas_price", "l2_gas_price"):
            block_document[price] = self._with_halved_gas_prices(meta[price])

        return block_document

    def transform_state_update(self, blob: JsonObject, block_number: int) -> JsonObject:
        """
        Build the stored "state_update" document for a blob/block_number pair.
        """
        # Build a state update object from the incoming blob's state_diff
        state_diff = blob["state_diff"]
        nonces = state_diff["nonces"]["L1"]

        storage_updates = state_diff["storage_updates"]["L1"]

        storage_diffs_out = {}
        for address, updates in storage_updates.items():
            # sort by numeric value for stable output.
            sorted_updates = sorted(updates.items(), key=lambda kv: int(kv[0], 16))
            storage_diffs_out[address] = [{"key": k, "value": v} for k, v in sorted_updates]

        new_root, old_root = self._chain.compute_current_and_previous_root(block_number)
        block_hash, _parent = self._chain.compute_current_and_previous_hash(block_number)

        deployed_contracts_map: JsonObject = {
            str(addr): class_hash
            for addr, class_hash in state_diff.get("address_to_class_hash", {}).items()
        }

        deployed_contracts_out = [
            {"address": a, "class_hash": c} for a, c in deployed_contracts_map.items()
        ]

        # TODO(Ron): Add declared classes to the state update.
        declared_classes: List[JsonObject] = []

        return {
            "block_hash": block_hash,
            "new_root": new_root,
            "old_root": old_root,
            "state_diff": {
                "storage_diffs": storage_diffs_out,
                "nonces": nonces,
                "deployed_contracts": deployed_contracts_out,
                "old_declared_contracts": [],
                "declared_classes": declared_classes,
                "replaced_classes": [],
                "migrated_compiled_classes": [],
            },
        }

    @staticmethod
    def extract_revert_error_mappings(blob: JsonObject) -> JsonObject:
        """
        Return {tx_hash: revert_error} from the blob, pairing entries by index:
        - execution_infos[i].revert_error
        - transactions[i].tx.hash_value
        """
        tx_entries = blob["transactions"]
        revert_error_mappings: JsonObject = {}

        for idx, item in enumerate(blob["execution_infos"]):
            err = item["revert_error"]
            if err is None:
                continue

            tx_hash = tx_entries[idx]["tx"]["hash_value"]
            revert_error_mappings[tx_hash] = err
        return revert_error_mappings


class EchoCenterService:
    """
    Encapsulates the core logic and state for the Echo Center.

    Flask routes defined at module level delegate to an instance of this class.
    """

    def __init__(
        self,
        feeder_client: FeederClient,
        shared_ctx: SharedContext,
        l1_mgr: L1Manager,
        flask_logger: logging.Logger,
        logger: logging.Logger,
    ) -> None:
        self.feeder_client: FeederClient = feeder_client
        self.shared: SharedContext = shared_ctx
        self.l1_manager: L1Manager = l1_mgr
        self.flask_logger: logging.Logger = flask_logger
        self.logger: logging.Logger = logger

        # Fetch constants once using the start block number and reuse for the full run.
        self._bootstrap = _fetch_bootstrap_constants(
            feeder_client=self.feeder_client, block_number=CONFIG.blocks.start_block
        )

        self._chain = DeterministicChain(self.feeder_client, self.shared, self.logger)
        self._transformer = BlobTransformer(
            feeder_client=self.feeder_client,
            shared_ctx=self.shared,
            chain=self._chain,
            custom_fields=self._bootstrap.custom_fields,
            logger_obj=self.logger,
        )

    @staticmethod
    def _maybe_int(v: Any) -> Optional[int]:
        if v is None:
            return None
        if isinstance(v, bool):
            # Prevent bool-as-int surprises.
            return None
        if isinstance(v, int):
            return v
        if isinstance(v, str):
            s = v.strip()
            if not s:
                return None
            try:
                return int(s, 16) if s.startswith("0x") else int(s)
            except Exception:
                return None
        return None

    def _extract_blob_timestamp_seconds(self, blob: JsonObject) -> Optional[int]:
        """
        Extract the timestamp (seconds) from an incoming blob.

        Expected schema (per captured blobs): `state_diff.block_info.block_timestamp`.
        """
        # If this ever changes, we want to fail loudly rather than silently accept wrong data.
        return int(blob["state_diff"]["block_info"]["block_timestamp"])

    def _maybe_get_blob_source_block_number(self, blob: JsonObject) -> Optional[int]:
        """
        Best-effort: infer source feeder block number for this blob using tx tracking.
        """
        try:
            tx_hashes = self._transformer.get_blob_tx_hashes(blob)
            if not tx_hashes:
                return None
            bn = self.shared.get_sent_block_number(tx_hashes[0])
            return int(bn) if bn is not None else None
        except Exception:
            return None

    def _log_timestamp_mismatch_if_any(self, blob: JsonObject, echo_block_number: int) -> None:
        """
        Compare blob timestamp vs the source timestamp we recorded at send time.

        If they differ, log with block numbers + both timestamps.
        """
        try:
            blob_ts = self._extract_blob_timestamp_seconds(blob)
            if blob_ts is None:
                return

            mismatches: list[dict[str, object]] = []
            for entry in blob.get("transactions", []):
                tx = entry.get("tx", {}) if isinstance(entry, dict) else {}
                raw_hash = tx.get("hash_value")
                if raw_hash is None:
                    continue
                tx_hash = str(raw_hash)
                if not tx_hash:
                    continue

                # L1_HANDLER txs are allowed to have different timestamps.
                # (Their execution/commit timing can legitimately differ from L2 txs.)
                if str(tx.get("type")) == TxType.L1_HANDLER.value:
                    continue

                expected_ts = self.shared.get_tx_timestamp(tx_hash)
                if expected_ts is None:
                    continue
                if int(blob_ts) == int(expected_ts):
                    continue
                source_block: Optional[int] = None
                try:
                    source_block = int(self.shared.get_sent_block_number(tx_hash))
                except Exception:
                    source_block = None
                mismatches.append(
                    {
                        "tx": tx_hash,
                        "source_block": source_block,
                        "source_ts": int(expected_ts),
                    }
                )
                # Record for UI diagnostics (best-effort).
                try:
                    self.shared.record_timestamp_mismatch(
                        tx_hash=tx_hash,
                        echo_block=int(echo_block_number),
                        source_block=source_block,
                        source_ts=int(expected_ts),
                        blob_ts=int(blob_ts),
                    )
                except Exception:
                    pass

            if not mismatches:
                return

            # Avoid extremely verbose logs if a blob contains many txs.
            max_items = 5
            sample = mismatches[:max_items]
            sample_str = " ".join(
                f"tx={m['tx']} src_bn={m['source_block'] if m['source_block'] is not None else 'unknown'} src_ts={m['source_ts']}"
                for m in sample
            )
            source_block_fallback = self._maybe_get_blob_source_block_number(blob)
            self.logger.warning(
                "timestamp mismatch: "
                f"echo_block={int(echo_block_number)} "
                f"source_block={source_block_fallback if source_block_fallback is not None else 'unknown'} "
                f"blob_ts={int(blob_ts)} "
                f"mismatched_txs={len(mismatches)} "
                f"sample=[{sample_str}]" + ("" if len(mismatches) <= max_items else " ...")
            )
        except Exception as err:
            # Never break write_blob on diagnostics.
            self.logger.debug(f"timestamp mismatch check failed: {err}")

    def _check_l2_gas_mismatches(self, stored_block: JsonObject, echo_block_number: int) -> None:
        """
        Compare L2 gas used in the written blob vs the source feeder receipt.

        IMPORTANT: DO NOT REMOVE. This log is used to detect correctness regressions.
        """
        try:
            ignore_calldata_marker = (
                "0x10398fe631af9ab2311840432d507bf7ef4b959ae967f1507928f5afe888a99"
            )
            txs: list[JsonObject] = stored_block.get("transactions", [])
            receipts: list[JsonObject] = stored_block.get("transaction_receipts", [])
            if not txs or not receipts or len(txs) != len(receipts):
                return

            for tx, receipt in zip(txs, receipts):
                tx_hash = tx.get("transaction_hash")
                if not isinstance(tx_hash, str):
                    continue

                # Ignore noisy known pattern by calldata marker.
                calldata = tx.get("calldata")
                if isinstance(calldata, list) and any(
                    str(x).lower() == ignore_calldata_marker for x in calldata
                ):
                    continue
                if isinstance(calldata, str) and ignore_calldata_marker in calldata.lower():
                    continue

                # Only compare txs that originated from feeder forwarding (i.e., we know their source block).
                if not self.shared.is_pending_tx(tx_hash):
                    continue
                try:
                    source_block = int(self.shared.get_sent_block_number(tx_hash))
                except Exception:
                    continue

                fgw_block = self.shared.get_fgw_block(source_block)
                if not fgw_block:
                    continue

                # Find the tx index in the feeder block so we can read the matching receipt.
                fgw_txs = fgw_block.get("transactions", [])
                fgw_receipts = fgw_block.get("transaction_receipts", [])
                if not fgw_txs or not fgw_receipts or len(fgw_txs) != len(fgw_receipts):
                    continue

                fgw_idx: Optional[int] = None
                for i, fgw_tx in enumerate(fgw_txs):
                    if fgw_tx.get("transaction_hash") == tx_hash:
                        fgw_idx = i
                        break
                if fgw_idx is None:
                    continue

                blob_l2_gas = self._maybe_int(
                    receipt.get("execution_resources", {})
                    .get("total_gas_consumed", {})
                    .get("l2_gas")
                )
                fgw_l2_gas = self._maybe_int(
                    fgw_receipts[fgw_idx]
                    .get("execution_resources", {})
                    .get("total_gas_consumed", {})
                    .get("l2_gas")
                )
                if blob_l2_gas is None or fgw_l2_gas is None:
                    continue

                if blob_l2_gas != fgw_l2_gas:
                    # Record for UI diagnostics (best-effort).
                    try:
                        self.shared.record_l2_gas_mismatch(
                            tx_hash=str(tx_hash),
                            echo_block=int(echo_block_number),
                            source_block=int(source_block),
                            blob_total_gas_l2=int(blob_l2_gas),
                            fgw_total_gas_consumed_l2=int(fgw_l2_gas),
                        )
                    except Exception:
                        pass
                    self.logger.warning(
                        "l2_gas mismatch: "
                        f"tx={tx_hash} "
                        f"echo_block={int(echo_block_number)} "
                        f"source_block={int(source_block)} "
                        f"blob_total_gas_l2={int(blob_l2_gas)} "
                        f"fgw_total_gas_consumed_l2={int(fgw_l2_gas)}"
                    )
        except Exception as err:
            # Never break write_blob on diagnostics.
            self.logger.debug(f"l2_gas mismatch check failed: {err}")

    @staticmethod
    def _json_response(payload: Any, status: int = requests.codes.ok) -> flask.Response:
        raw = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        return flask.Response(raw, status=status, headers=[["Content-Type", "application/json"]])

    @staticmethod
    def _empty_response(status: int = requests.codes.ok) -> flask.Response:
        return flask.Response(b"", status=status)

    @staticmethod
    def _parse_block_number(bn: str) -> BlockNumberParam:
        return "latest" if bn == "latest" else int(bn)

    def _update_tx_tracking_and_reverts(self, blob: JsonObject, block_number: int) -> None:
        hashes = self._transformer.get_blob_tx_hashes(blob)
        for h, err in self._transformer.extract_revert_error_mappings(blob).items():
            self.shared.record_echonet_revert_error(h, err)
        for h in hashes:
            self.shared.record_committed_tx(h, block_number)

    def handle_write_blob(self) -> flask.Response:
        """
        POST /cende_recorder/write_blob

        Stores the raw blob plus derived block/state_update documents in SharedContext.
        """
        body = flask.request.get_data()
        self.flask_logger.info(
            f"WRITE_BLOB len={len(body)} ct={flask.request.headers.get('Content-Type')}"
        )

        blob = json.loads(body)
        self._chain.refresh_base()
        block_number = int(blob["block_number"])

        if self.shared.has_block(block_number):
            self.flask_logger.info(f"Duplicate WRITE_BLOB for block {block_number}; no-op")
            return self._empty_response(requests.codes.ok)

        self.shared.set_last_block(block_number)
        self.flask_logger.info(f"last_block={block_number}")

        to_store = self._transformer.transform_block(blob)
        state_update = self._transformer.transform_state_update(blob, block_number)

        # Keep the latest block timestamp available via /echonet/timestamp.
        try:
            ts = self._maybe_int(to_store.get("timestamp"))
            if ts is not None:
                self.shared.set_block_timestamp(int(ts))
        except Exception:
            # Never break write_blob on progress marker updates.
            pass

        # Diagnostic log: detect timestamp mismatches between incoming blob and upstream meta.
        self._log_timestamp_mismatch_if_any(blob, echo_block_number=block_number)

        # Critical diagnostic log: keep this check in place.
        self._check_l2_gas_mismatches(to_store, echo_block_number=block_number)

        self.shared.store_block(
            block_number, blob=blob, fgw_block=to_store, state_update=state_update
        )
        self.flask_logger.info(
            f"block {block_number} tx hashes: {' '.join(self._transformer.get_blob_tx_hashes(blob))}"
        )

        self._update_tx_tracking_and_reverts(blob, block_number)

        return self._empty_response(requests.codes.ok)

    def handle_write_pre_confirmed_block(self) -> flask.Response:
        self.flask_logger.debug("Received pre-confirmed block")
        return self._empty_response(requests.codes.ok)

    def handle_report_snapshot(self) -> flask.Response:
        """Return current in-memory tx tracking snapshot."""
        snap = self.shared.get_report_snapshot()
        return self._json_response(snap.to_dict(), requests.codes.ok)

    def handle_report_ui(self) -> flask.Response:
        """
        HTML report dashboard.

        Kept separate from /echonet/report (JSON) so scripts keep working.
        """
        snap = self.shared.get_report_snapshot()
        diag = self.shared.get_diagnostics_snapshot()
        vm = report_web.build_report_view_model(snap, diagnostics=diag)
        return flask.render_template("report.html", export=False, inline_css_b64="", **vm)

    def handle_report_ui_download(self) -> flask.Response:
        """
        Download a static, self-contained HTML snapshot of the current report.

        No JS; CSS is inlined so the file can be opened offline.
        """
        snap = self.shared.get_report_snapshot()
        diag = self.shared.get_diagnostics_snapshot()
        vm = report_web.build_report_view_model(snap, diagnostics=diag)

        # Inline CSS from our static file (keeps it self-contained).
        css_path = Path(__file__).resolve().parent / "static" / "report.css"
        inline_css_b64 = ""
        try:
            inline_css_b64 = base64.b64encode(css_path.read_bytes()).decode("ascii")
        except Exception:
            inline_css_b64 = ""

        html = flask.render_template(
            "report.html", export=True, inline_css_b64=inline_css_b64, **vm
        )
        ts = str(vm.get("meta", {}).get("generated_at_utc", ""))
        safe_ts = (
            ts.replace(":", "")
            .replace("-", "")
            .replace(".", "")
            .replace("Z", "Z")
            .replace("T", "T")
        )
        filename = f"echonet_report_{safe_ts or 'snapshot'}.html"

        resp = flask.Response(
            html.encode("utf-8"),
            status=requests.codes.ok,
            headers=[
                ["Content-Type", "text/html; charset=utf-8"],
                ["Content-Disposition", f'attachment; filename="{filename}"'],
            ],
        )
        return resp

    def handle_report_text(self) -> flask.Response:
        """
        Text report similar to `python reports.py --all`.
        """
        snap = self.shared.get_report_snapshot()

        snapshot_text = SnapshotTextReport(snap).render()
        reverts_text = RevertComparisonTextReport(
            classifier=RevertClassifier(), mode="grouped"
        ).render(
            mainnet_reverts=dict(snap.revert_errors_mainnet),
            echonet_reverts=dict(snap.revert_errors_echonet),
        )
        out = (
            snapshot_text.rstrip()
            + "\n\n"
            + "=== CLASSIFIED REVERTS (shortened) ===\n"
            + reverts_text
        )
        return flask.Response(
            out.encode("utf-8"),
            status=requests.codes.ok,
            headers=[["Content-Type", "text/plain; charset=utf-8"]],
        )

    def handle_get_tx_timestamp(self) -> flask.Response:
        """
        GET /echonet/tx_timestamp?transactionHash=0x...

        Return the source block timestamp (seconds) recorded when this tx was sent (pending).
        """
        args = flask.request.args.to_dict(flat=True)
        tx_hash = args.get("transactionHash") or args.get("txHash") or args.get("tx_hash")
        if not tx_hash:
            return self._json_response(
                {"error": "Missing required query param: transactionHash"},
                requests.codes.bad_request,
            )

        ts = self.shared.get_tx_timestamp(str(tx_hash))
        if ts is None:
            return self._empty_response(requests.codes.not_found)
        return self._json_response({"timestamp": int(ts)}, requests.codes.ok)

    def handle_block_dump(self) -> flask.Response:
        args = flask.request.args.to_dict(flat=True)
        bn = int(args["blockNumber"])
        kind_raw = args.get("kind", BlockDumpKind.BLOB.value)
        try:
            kind = BlockDumpKind(kind_raw)
        except ValueError:
            return self._json_response(
                {"error": f"Invalid kind: {kind_raw}"}, requests.codes.bad_request
            )

        payload = self.shared.get_block_field(bn, kind.value)
        if payload is None:
            return self._empty_response(requests.codes.not_found)
        return self._json_response(payload, requests.codes.ok)

    def handle_get_block(self) -> flask.Response:
        """
        GET /feeder_gateway/get_block

        Serves stored blocks at/after the configured start block, and proxies older blocks to
        the upstream feeder.
        """
        args = flask.request.args.to_dict(flat=True)
        bn_parsed = self._parse_block_number(args.get("blockNumber"))

        header_only_raw = args.get("headerOnly")
        header_only = header_only_raw == "true" if header_only_raw else False

        wfmi_raw = args.get("withFeeMarketInfo")
        with_fee_market_info = wfmi_raw == "true" if wfmi_raw else None

        # If explicitly requesting a block older than our configured starting block,
        # return it directly from the upstream feeder.
        if isinstance(bn_parsed, int) and bn_parsed < self._chain.base_block_number:
            upstream_obj = self.feeder_client.get_block(
                bn_parsed, header_only=header_only, with_fee_market_info=with_fee_market_info
            )
            return self._json_response(upstream_obj, requests.codes.ok)

        if bn_parsed == "latest":
            highest = self.shared.get_latest_block_number()
            if highest is None:
                return self._empty_response(requests.codes.not_found)
            obj = self.shared.get_block_field(highest, "block")
            if header_only:
                return self._json_response(
                    {"block_hash": obj["block_hash"], "block_number": highest}, requests.codes.ok
                )
            return self._json_response(obj, requests.codes.ok)

        requested = bn_parsed
        obj = self.shared.get_block_field(requested, "block")
        if obj:
            if header_only:
                return self._json_response(
                    {"block_hash": obj["block_hash"], "block_number": requested}, requests.codes.ok
                )
            return self._json_response(obj, requests.codes.ok)

        return self._empty_response(requests.codes.not_found)

    def handle_get_state_update(self) -> flask.Response:
        args = flask.request.args.to_dict(flat=True)
        bn_raw = args.get("blockNumber")
        bn_parsed = self._parse_block_number(bn_raw)

        if bn_parsed == "latest":
            highest = self.shared.get_latest_block_number()
            if highest is None:
                return self._empty_response(requests.codes.not_found)
            state_update = self.shared.get_block_field(highest, "state_update")
            if state_update:
                return self._json_response(state_update, requests.codes.ok)
            return self._empty_response(requests.codes.not_found)

        requested = int(bn_parsed)

        state_update = self.shared.get_block_field(requested, "state_update")
        if state_update:
            return self._json_response(state_update, requests.codes.ok)

        return self._empty_response(requests.codes.not_found)

    def handle_get_signature(self) -> flask.Response:
        args = flask.request.args.to_dict(flat=True)
        bn_raw = args.get("blockNumber")
        bn_parsed = self._parse_block_number(bn_raw)

        if bn_parsed == "latest":
            exists = self.shared.has_any_blocks()
        else:
            exists = self.shared.has_block(bn_parsed)

        if exists:
            return self._json_response(self._bootstrap.signature_const, requests.codes.ok)

        return self._empty_response(requests.codes.not_found)

    def handle_get_class_by_hash(self) -> flask.Response:
        args = flask.request.args.to_dict(flat=True)
        bn_raw = args.get("blockNumber")
        class_hash = args["classHash"]
        obj = self.feeder_client.get_class_by_hash(class_hash, block_number=bn_raw)
        return self._json_response(obj, requests.codes.ok)

    def handle_get_compiled_class_by_class_hash(self) -> flask.Response:
        args = flask.request.args.to_dict(flat=True)
        bn_raw = args.get("blockNumber")
        class_hash = args["classHash"]
        obj = self.feeder_client.get_compiled_class_by_class_hash(class_hash, block_number=bn_raw)
        return self._json_response(obj, requests.codes.ok)

    def handle_l1(self) -> flask.Response:
        """
        L1 endpoint used as a JSON-RPC entrypoint.

        - For JSON-RPC calls (POST with a body containing a "method" field), dispatch the method
          to the same handlers used by the explicit eth_* HTTP endpoints below.
        - For any other request, just return 200 with an empty body.
        """
        data = flask.request.get_json()
        method = data["method"]
        self.logger.info(f"Method: {method}")

        raw_params = data.get("params")
        self.logger.info(f"Raw params: {raw_params}")
        params = raw_params[0] if isinstance(raw_params, list) and raw_params else {}

        if method == "eth_blockNumber":
            payload = self.l1_manager.get_block_number()
            self.logger.info(f"eth_blockNumber payload: {payload}")
            return self._json_response(payload, requests.codes.ok)

        if method == "eth_getBlockByNumber":
            payload = self.l1_manager.get_block_by_number(params)
            return self._json_response(payload, requests.codes.ok)

        if method == "eth_getLogs":
            payload = self.l1_manager.get_logs(params)
            self.logger.info(f"eth_getLogs payload: {payload}")
            return self._json_response(payload, requests.codes.ok)

        if method == "eth_call":
            payload = self.l1_manager.get_call(params)
            self.logger.info(f"eth_call payload: {payload}")
            return self._json_response(payload, requests.codes.ok)

        error_payload = {
            "jsonrpc": "2.0",
            "id": 1,
            "error": {"code": -32601, "message": f"Method {method} not implemented"},
        }
        self.logger.info(
            f"Unhandled JSON-RPC method {method}, returning error payload: {error_payload}"
        )
        return self._json_response(error_payload, requests.codes.ok)


app = flask.Flask(__name__)
feeder_client = FeederClient()
service = EchoCenterService(
    feeder_client=feeder_client,
    shared_ctx=shared,
    l1_mgr=l1_manager,
    flask_logger=flask_logger,
    logger=logger,
)


@app.route("/cende_recorder/write_blob", methods=["POST"])
def write_blob() -> flask.Response:
    return service.handle_write_blob()


@app.route("/cende_recorder/write_pre_confirmed_block", methods=["POST"])
def write_pre_confirmed_block() -> flask.Response:
    return service.handle_write_pre_confirmed_block()


@app.route("/echonet/report", methods=["GET"])
def report_snapshot() -> flask.Response:
    return service.handle_report_snapshot()


@app.route("/echonet/report/ui", methods=["GET"])
@app.route("/echonet/report_html", methods=["GET"])
def report_ui() -> flask.Response:
    return service.handle_report_ui()


@app.route("/echonet/report/ui/download", methods=["GET"])
@app.route("/echonet/report_html_download", methods=["GET"])
def report_ui_download() -> flask.Response:
    return service.handle_report_ui_download()


@app.route("/echonet/report/text", methods=["GET"])
@app.route("/echonet/report_text", methods=["GET"])
def report_text() -> flask.Response:
    return service.handle_report_text()


@app.route("/echonet/tx_timestamp", methods=["GET"])
def get_tx_timestamp() -> flask.Response:
    return service.handle_get_tx_timestamp()


@app.route("/echonet/block_dump", methods=["GET"])
def block_dump() -> flask.Response:
    return service.handle_block_dump()


@app.route("/feeder_gateway/get_block", methods=["GET"])
def get_block() -> flask.Response:
    return service.handle_get_block()


@app.route("/feeder_gateway/get_state_update", methods=["GET"])
def get_state_update() -> flask.Response:
    return service.handle_get_state_update()


@app.route("/feeder_gateway/get_signature", methods=["GET"])
def get_signature() -> flask.Response:
    return service.handle_get_signature()


@app.route("/feeder_gateway/get_class_by_hash", methods=["GET"])
def get_class_by_hash() -> flask.Response:
    return service.handle_get_class_by_hash()


@app.route("/feeder_gateway/get_compiled_class_by_class_hash", methods=["GET"])
def get_compiled_class_by_class_hash() -> flask.Response:
    return service.handle_get_compiled_class_by_class_hash()


@app.route("/l1", methods=["GET", "POST"])
def l1() -> flask.Response:
    return service.handle_l1()


# Start the transaction sender automatically on startup.
# Werkzeug is the WSGI/server toolkit Flask uses.
# Werkzeug sets WERKZEUG_RUN_MAIN="true" in the child, so this guard prevents
# starting the background sender twice during local development.
if os.environ.get("WERKZEUG_RUN_MAIN") in (None, "true"):
    start_background_sender()
