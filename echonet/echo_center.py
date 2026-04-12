import base64
import json
import logging
import os
import subprocess
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, List, Literal, Optional, Tuple, Union

import flask  # pyright: ignore[reportMissingImports]
import requests

from echonet.constants import IGNORED_L2_GAS_MISMATCH_ATTESTATION_CALLDATA
from echonet.echonet_types import CONFIG, BlockDumpKind, JsonObject, TxType
from echonet.feeder_client import FeederClient
from echonet.helpers import format_hex
from echonet.l1_logic.l1_manager import L1Manager
from echonet.logger import get_logger
from echonet.reports import (
    RevertClassifier,
    RevertComparisonTextReport,
    SnapshotTextReport,
    build_report_view_model,
    filter_mainnet_reverts_for_reporting,
)
from echonet.sequencer_manager import _read_namespace_from_serviceaccount
from echonet.shared_context import SharedContext, l1_manager, shared
from echonet.transaction_sender import start_background_sender

BlockNumberParam = Union[int, Literal["latest"]]

flask_logger = get_logger("flask")
logger = get_logger("echo_center")


def _static_file_b64(filename: str) -> str:
    p = Path(__file__).resolve().parent / "static" / filename
    return base64.b64encode(p.read_bytes()).decode("ascii")


def _build_gcp_logs_context() -> dict[str, str]:
    """Context used by the report UI to build GCP Logs Explorer links."""
    ns = _read_namespace_from_serviceaccount()
    return {
        "gcp_project_id": CONFIG.gcp_logs.project_id,
        "gcp_location": CONFIG.gcp_logs.location,
        "gke_cluster_name": CONFIG.gcp_logs.gke_cluster_name,
        "k8s_namespace": ns,
        "duration": "PT2H",
    }


def _total_l2_gas_consumed(receipt: JsonObject) -> int:
    return receipt["execution_resources"]["total_gas_consumed"]["l2_gas"]


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
        "event_commitment",
        "receipt_commitment",
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

    def compute_current_hash(self, block_number: int) -> str:
        """Return current block hash for `block_number`."""
        self.refresh_base()
        assert self._base is not None
        current_hash, _previous_hash = self._compute_current_and_previous(
            block_number, self._base.base_block_hash_hex
        )
        return current_hash

    def compute_current_and_previous_root(self, block_number: int) -> Tuple[str, str]:
        """Return (new_root, old_root) for `block_number`."""
        self.refresh_base()
        assert self._base is not None
        return self._compute_current_and_previous(block_number, self._base.base_state_root_hex)

    @property
    def base_block_number(self) -> int:
        self.refresh_base()
        return self._base.base_block_number

    @property
    def base_block_hash_hex(self) -> str:
        self.refresh_base()
        return self._base.base_block_hash_hex


def _get_fgw_block_or_upstream(
    feeder_client: FeederClient, shared_ctx: SharedContext, block_number: int
) -> JsonObject:
    """Return the cached FGW block if available, otherwise fetch it from the feeder client."""
    obj = shared_ctx.get_fgw_block(block_number)
    if not obj:
        obj = feeder_client.get_block(block_number, with_fee_market_info=True)
    return dict(obj)


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

    @staticmethod
    def get_blob_tx_hashes(blob: JsonObject) -> List[str]:
        txs = blob["transactions"]
        tx_hashes: List[str] = []
        for entry in txs:
            tx_hashes.append(entry["tx"]["hash_value"])
        return tx_hashes

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
        tx_index: int, tx_hash: str, execution_info: JsonObject, tx_type: str
    ) -> JsonObject:
        """
        Transform a blob execution_infos[i] entry into a feeder gateway type transaction receipt.
        """
        events: List[JsonObject] = []
        l2_to_l1_messages: List[JsonObject] = []
        call_info_keys = (
            ("execute_call_info", "validate_call_info", "fee_transfer_call_info")
            if tx_type == TxType.DEPLOY_ACCOUNT
            else ("validate_call_info", "execute_call_info", "fee_transfer_call_info")
        )
        for key in call_info_keys:
            flattened = BlobTransformer._flatten_call_info(execution_info[key])
            # Keep event order stable per execution phase and then concatenate phases in
            # mainnet-consistent order.
            flattened.events_with_order.sort(
                key=lambda p: (p[0] is None, p[0] if p[0] is not None else 0)
            )
            events.extend(ev for _order, ev in flattened.events_with_order)
            l2_to_l1_messages.extend(flattened.l2_to_l1_messages)

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
            "l2_to_l1_messages": l2_to_l1_messages,
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

    def _fetch_upstream_source_block(self, block_number: Optional[int]) -> JsonObject:
        """
        Fetch the upstream/source block used for non-constant block fields.
        """
        if block_number is None:
            block_number = self._chain.base_block_number
        return _get_fgw_block_or_upstream(self._feeder_client, self._shared, block_number)

    @staticmethod
    def _extract_blob_block_meta(blob: JsonObject) -> JsonObject:
        block_info = blob["state_diff"]["block_info"]
        return {
            "timestamp": int(block_info["block_timestamp"]),
            "l1_gas_price": block_info["l1_gas_price"],
            "l1_data_gas_price": block_info["l1_data_gas_price"],
            "l2_gas_price": block_info["l2_gas_price"],
        }

    def _resolve_parent_block_hash(self) -> str:
        latest_block_number = self._shared.get_latest_block_number()
        if latest_block_number is None:
            return self._chain.base_block_hash_hex
        return self._shared.get_block_field(latest_block_number, "block")["block_hash"]

    def _run_block_hash_cli(self, subcommand: str, payload: JsonObject) -> Any:
        cli_path = CONFIG.paths.block_hash_cli_path
        if not cli_path.exists():
            raise RuntimeError(f"Missing block-hash calculator CLI binary at: {cli_path}")
        repo_root = Path(__file__).resolve().parent.parent
        with tempfile.TemporaryDirectory(prefix="echonet_block_hash_", dir=repo_root) as tmp:
            input_path = Path(tmp) / "input.json"
            output_path = Path(tmp) / "output.json"
            input_path.write_text(json.dumps(payload), encoding="utf-8")
            command = [
                str(cli_path),
                "block-hash",
                subcommand,
                "--input-path",
                str(input_path),
                "--output-path",
                str(output_path),
            ]
            subprocess.run(
                command, cwd=repo_root, capture_output=True, text=True, check=True, timeout=30
            )
            return json.loads(output_path.read_text(encoding="utf-8"))

    @staticmethod
    def _build_thin_state_diff(blob: JsonObject) -> JsonObject:
        state_diff = blob["state_diff"]
        return {
            "deployed_contracts": state_diff["address_to_class_hash"],
            "storage_diffs": state_diff["storage_updates"]["L1"],
            "class_hash_to_compiled_class_hash": state_diff["class_hash_to_compiled_class_hash"],
            "deprecated_declared_classes": [],
            "nonces": state_diff["nonces"]["L1"],
        }

    @staticmethod
    def _build_transactions_data_for_commitments(
        transformed_txs: List[JsonObject],
        receipts: List[JsonObject],
        execution_infos: List[JsonObject],
    ) -> List[JsonObject]:
        return [
            {
                "transaction_signature": tx.get("signature", []),
                "transaction_output": {
                    "actual_fee": receipt["actual_fee"],
                    "events": receipt["events"],
                    "execution_status": (
                        {
                            "execution_status": "REVERTED",
                            "revert_reason": receipt.get("revert_error", ""),
                        }
                        if receipt["execution_status"] == "REVERTED"
                        else {"execution_status": "SUCCEEDED"}
                    ),
                    "gas_consumed": execution_info["total_gas"],
                    "messages_sent": receipt["l2_to_l1_messages"],
                },
                "transaction_hash": tx["transaction_hash"],
            }
            for tx, receipt, execution_info in zip(transformed_txs, receipts, execution_infos)
        ]

    def _compute_block_commitments(
        self,
        blob: JsonObject,
        transformed_txs: List[JsonObject],
        receipts: List[JsonObject],
        execution_infos: List[JsonObject],
    ) -> JsonObject:
        block_info = blob["state_diff"]["block_info"]
        return self._run_block_hash_cli(
            "block-hash-commitments",
            {
                "transactions_data": self._build_transactions_data_for_commitments(
                    transformed_txs, receipts, execution_infos
                ),
                "state_diff": self._build_thin_state_diff(blob),
                "l1_da_mode": "BLOB" if block_info.get("use_kzg_da") else "CALLDATA",
                "starknet_version": block_info["starknet_version"],
            },
        )

    @staticmethod
    def _compute_state_diff_length(blob: JsonObject) -> int:
        state_diff = blob["state_diff"]
        storage_updates = state_diff["storage_updates"]["L1"]
        return (
            len(state_diff["address_to_class_hash"])
            + len(state_diff["class_hash_to_compiled_class_hash"])
            + len(state_diff["nonces"]["L1"])
            + sum(len(slots) for slots in storage_updates.values())
        )

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
                    tx_type=tx["type"],
                )
            )

        block_document: JsonObject = {
            "block_number": block_number,
            "transactions": transformed_txs,
            "transaction_receipts": receipts,
        }

        block_document["block_hash"] = self._chain.compute_current_hash(block_number)
        block_document["parent_block_hash"] = self._resolve_parent_block_hash()
        # Add custom fields to the block document, constant fields that are the same for all blocks.
        block_document.update(self._custom_fields)

        tx_hashes = self.get_blob_tx_hashes(blob)
        bn_for_meta: Optional[int] = (
            self._shared.get_sent_block_number(tx_hashes[0]) if tx_hashes else None
        )

        source_block = self._fetch_upstream_source_block(bn_for_meta)
        block_commitments = self._compute_block_commitments(
            blob,
            transformed_txs,
            receipts,
            execution_infos,
        )
        block_document["status"] = source_block["status"]
        block_document["starknet_version"] = source_block["starknet_version"]
        block_document["sequencer_address"] = source_block["sequencer_address"]
        block_document["transaction_commitment"] = str(block_commitments["transaction_commitment"])
        block_document["l2_gas_consumed"] = blob["fee_market_info"]["l2_gas_consumed"]
        block_document["next_l2_gas_price"] = blob["fee_market_info"]["next_l2_gas_price"]
        block_document["l1_da_mode"] = (
            "BLOB" if blob["state_diff"]["block_info"]["use_kzg_da"] else "CALLDATA"
        )
        block_document["state_diff_length"] = self._compute_state_diff_length(blob)
        block_document.update(self._extract_blob_block_meta(blob))

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
        block_hash = self._chain.compute_current_hash(block_number)

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

    def _check_l2_gas_mismatches(self, stored_block: JsonObject, echo_block_number: int) -> None:
        txs = stored_block.get("transactions", [])
        receipts = stored_block.get("transaction_receipts", [])
        if not txs:
            return

        fgw_l2_cache: dict[int, dict[str, int]] = {}
        for tx, receipt in zip(txs, receipts):
            tx_hash: str = tx["transaction_hash"]
            if any(
                str(x) == IGNORED_L2_GAS_MISMATCH_ATTESTATION_CALLDATA
                for x in (tx.get("calldata", []))
            ):
                continue

            source_block = self.shared.get_sent_block_number(tx_hash)
            fgw_l2_by_hash = fgw_l2_cache.get(source_block)
            if fgw_l2_by_hash is None:
                fgw_block = self.shared.get_fgw_block(source_block)
                fgw_l2_by_hash = {
                    t["transaction_hash"]: _total_l2_gas_consumed(r)
                    for t, r in zip(fgw_block["transactions"], fgw_block["transaction_receipts"])
                }
                fgw_l2_cache[source_block] = fgw_l2_by_hash

            blob_l2_gas: int = _total_l2_gas_consumed(receipt)
            fgw_l2_gas = fgw_l2_by_hash.get(tx_hash)
            if blob_l2_gas != fgw_l2_gas:
                self.logger.warning(
                    "l2_gas mismatch: "
                    f"tx={tx_hash} "
                    f"echo_block={echo_block_number} "
                    f"source_block={source_block} "
                    f"blob_total_gas_l2={blob_l2_gas} "
                    f"fgw_total_gas_consumed_l2={fgw_l2_gas}"
                )
                self.shared.record_l2_gas_mismatch(
                    tx_hash=tx_hash,
                    echo_block=echo_block_number,
                    source_block=source_block,
                    blob_total_gas_l2=blob_l2_gas,
                    fgw_total_gas_consumed_l2=fgw_l2_gas,
                )

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

    def handle_get_latest_received_block(self) -> flask.Response:
        """Returns the latest received block number, or start_block - 1 if nothing has been received."""
        latest = self.shared.get_latest_block_number()
        block_number = (
            latest
            if latest
            else self.shared.get_current_start_block(default_start_block=CONFIG.blocks.start_block)
            - 1
        )
        return self._json_response({"block_number": block_number}, requests.codes.ok)

    def handle_report_snapshot(self) -> flask.Response:
        """Return current in-memory tx tracking snapshot."""
        snap = self.shared.get_report_snapshot()
        return self._json_response(snap.to_dict(), requests.codes.ok)

    def handle_report_ui(self) -> flask.Response:
        """HTML report dashboard."""
        vm = build_report_view_model(self.shared.get_report_snapshot())
        return flask.render_template(
            "report.html",
            export=False,
            inline_favicon_b64=_static_file_b64("favicon.svg"),
            logs=_build_gcp_logs_context(),
            **vm,
        )

    def handle_report_ui_download(self) -> flask.Response:
        """
        Download a static, self-contained HTML snapshot of the current report.
        """
        vm = build_report_view_model(self.shared.get_report_snapshot())

        html = flask.render_template(
            "report.html",
            export=True,
            inline_favicon_b64=_static_file_b64("favicon.svg"),
            inline_css_b64=_static_file_b64("report.css"),
            inline_js_b64=_static_file_b64("report.js"),
            logs=_build_gcp_logs_context(),
            **vm,
        )
        resp = flask.Response(
            html.encode("utf-8"),
            status=requests.codes.ok,
            headers=[
                ["Content-Type", "text/html; charset=utf-8"],
                ["Content-Disposition", 'attachment; filename="echonet_report.html"'],
            ],
        )
        return resp

    def handle_report_text(self) -> flask.Response:
        """
        Plain-text snapshot report (similar to the old `reports.py` output).
        """
        snap = self.shared.get_report_snapshot()

        snapshot_text = SnapshotTextReport(snap).render()
        reverts_text = RevertComparisonTextReport(
            classifier=RevertClassifier(),
        ).render(
            mainnet_reverts=filter_mainnet_reverts_for_reporting(snap),
            echonet_reverts=dict(snap.revert_errors_echonet),
        )
        out = snapshot_text.rstrip() + "\n\n" + reverts_text
        return flask.Response(
            out.encode("utf-8"),
            status=requests.codes.ok,
            headers=[["Content-Type", "text/plain; charset=utf-8"]],
        )

    def handle_get_tx_block_metadata(self) -> flask.Response:
        """
        GET /echonet/get_tx_block_metadata?tx_hash=0x...

        Returns a JSON object containing source block timestamp and block number.
        """
        args = flask.request.args.to_dict(flat=True)
        tx_hash = args.get("tx_hash")
        if not tx_hash:
            return self._json_response(
                {"error": "Missing required query param: tx_hash"},
                requests.codes.bad_request,
            )

        payload = self.shared.get_sent_tx_timestamp_and_block_number(tx_hash)
        return self._json_response(payload, requests.codes.ok)

    def handle_get_starknet_version(self) -> flask.Response:
        """
        GET /echonet/get_starknet_version

        Returns the starknet_version string of the configured start block.
        """
        block = self.feeder_client.get_block(CONFIG.blocks.start_block)
        return flask.Response(
            block["starknet_version"].encode("utf-8"),
            status=requests.codes.ok,
            headers=[["Content-Type", "text/plain; charset=utf-8"]],
        )

    def handle_get_block_metadata(self) -> flask.Response:
        """
        GET /echonet/get_block_metadata?block_number=<n>

        Returns metadata for the given block from the mainnet FGW.
        """
        logger.info(f"handle_get_block_metadata: {flask.request.args}")
        args = flask.request.args.to_dict(flat=True)
        block_number = int(args["block_number"])
        block = _get_fgw_block_or_upstream(self.feeder_client, self.shared, block_number)
        return self._json_response(
            {
                "timestamp": block["timestamp"],
                "l1_gas_price_wei": block["l1_gas_price"]["price_in_wei"],
                "l1_gas_price_fri": block["l1_gas_price"]["price_in_fri"],
                "l1_data_gas_price_wei": block["l1_data_gas_price"]["price_in_wei"],
                "l1_data_gas_price_fri": block["l1_data_gas_price"]["price_in_fri"],
                "l2_gas_price_fri": block["l2_gas_price"]["price_in_fri"],
            },
            requests.codes.ok,
        )

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

        payload = self.shared.get_block_field_with_disk_fallback(bn, kind.value)
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
        raw_params = data.get("params")
        params = raw_params[0] if isinstance(raw_params, list) and raw_params else {}

        if method == "eth_blockNumber":
            payload = self.l1_manager.get_block_number()
            result = payload.get("result")
            self.logger.info(f"eth_blockNumber: {result} ({int(result, 16) if result else 'N/A'})")
            return self._json_response(payload, requests.codes.ok)

        if method == "eth_getBlockByNumber":
            self.logger.info(
                f"eth_getBlockByNumber: {params}"
                + (f" ({int(params, 16)})" if params.startswith("0x") else "")
            )
            payload = self.l1_manager.get_block_by_number(params)
            return self._json_response(payload, requests.codes.ok)

        if method == "eth_getLogs":
            from_block = params.get("fromBlock", "0x0")
            to_block = params.get("toBlock", "0x0")
            from_dec = f" ({int(from_block, 16)})" if from_block.startswith("0x") else ""
            to_dec = f" ({int(to_block, 16)})" if to_block.startswith("0x") else ""
            self.logger.info(f"eth_getLogs: from={from_block}{from_dec}, to={to_block}{to_dec}")
            payload = self.l1_manager.get_logs(params)
            self.logger.info(f"eth_getLogs: {len(payload.get('result', []))} logs")
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


@app.route("/cende_recorder/get_latest_received_block", methods=["GET"])
def get_latest_received_block() -> flask.Response:
    return service.handle_get_latest_received_block()


@app.route("/echonet/report", methods=["GET"])
def report_snapshot() -> flask.Response:
    return service.handle_report_snapshot()


@app.route("/echonet/get_tx_block_metadata", methods=["GET"])
def get_tx_block_metadata() -> flask.Response:
    return service.handle_get_tx_block_metadata()


@app.route("/echonet/get_block_metadata", methods=["GET"])
def get_block_metadata() -> flask.Response:
    return service.handle_get_block_metadata()


@app.route("/echonet/get_starknet_version", methods=["GET"])
def get_starknet_version() -> flask.Response:
    return service.handle_get_starknet_version()


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


@app.route("/echonet/report/ui", methods=["GET"])
def report_ui() -> flask.Response:
    return service.handle_report_ui()


@app.route("/echonet/report/ui/download", methods=["GET"])
def report_ui_download() -> flask.Response:
    return service.handle_report_ui_download()


@app.route("/echonet/report/text", methods=["GET"])
def report_text() -> flask.Response:
    return service.handle_report_text()


# Start the transaction sender automatically on startup.
# Werkzeug is the WSGI/server toolkit Flask uses.
# Werkzeug sets WERKZEUG_RUN_MAIN="true" in the child, so this guard prevents
# starting the background sender twice during local development.
if os.environ.get("WERKZEUG_RUN_MAIN") in (None, "true"):
    start_background_sender()
