import json
import os
from dataclasses import dataclass
from typing import Any, Dict, List, Mapping, Optional, Tuple, Union

import consts
import requests
from feeder_client import FeederClient
from flask import Flask, Response, request  # pyright: ignore[reportMissingImports]
from helpers import format_hex
from logger import get_logger
from shared_context import l1_manager, shared
from transaction_sender import start_background_sender
from tx_types import TxType

JsonDict = Dict[str, Any]
BlockNumberParam = Union[int, str]

flask_logger = get_logger("flask")
logger = get_logger("echo_center")


CUSTOM_FIELDS: JsonDict = {
    "state_root": "0x6138090a2ceae6c179b01b9bbdc13c74d03063cf3f801017cc5fc6bae514881",
    "transaction_commitment": "0x1432ac404fda8b7c921df9c55f1b1539e3f982837a62f6b9db6c843de9f2e85",
    "event_commitment": "0x76747e447201cbd15d044fffff15d30aca29681cef8442ee771cd90692b4b2e",
    "receipt_commitment": "0x107308016a4910c9ad57fc4cf7ce2fc2bfafddd6ca6def98bbfdbeea16429d2",
    "state_diff_length": 159,
    "status": "ACCEPTED_ON_L1",
    "l1_da_mode": "BLOB",
    "l2_gas_consumed": 606837191,
    "next_l2_gas_price": "0xb2d05e0",
    "sequencer_address": "0x1176a1bd84444c89232ec27754698e5d2e7e1a7f1539f12027f28b23ec9f3d8",
    "starknet_version": "0.14.1",
}


SIGNATURE_CONST: JsonDict = {
    "block_hash": "0x8470dbc0e524e713c926511c3b1b5c8512b083f925bf0bd247f0a46ed91a4a",
    "signature": [
        "0x5447b6b452f704af805b2166f863c3b31a0b864f56ed2e5adce5fe64fec162e",
        "0x61dee68ee8a9ade268bc6b3515484350d29dd6ffc5804ff300eda40460575ef",
    ],
}


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

    def __init__(self, feeder_client: FeederClient, shared_ctx, logger_obj) -> None:
        self._feeder_client = feeder_client
        self._shared = shared_ctx
        self._logger = logger_obj
        self._base: Optional[_BaseValues] = None
        self.refresh_base()

    def refresh_base(self) -> None:
        """
        Refresh base values from upstream feeder if needed.

        Base values are derived from (configured_start_block - 1) so that the first
        locally-stored block at configured_start_block has a consistent parent.
        """
        start_block = self._shared.get_current_start_block(
            default_start_block=int(consts.CONFIG.blocks.start_block)
        )
        if self._base is not None and self._base.base_block_number == start_block:
            return

        block = self._feeder_client.get_block(start_block - 1)
        assert block is not None
        base_block_hash_hex = block["block_hash"]
        base_state_root_hex = block["state_root"]
        self._base = _BaseValues(
            base_block_number=start_block,
            base_block_hash_hex=base_block_hash_hex,
            base_state_root_hex=base_state_root_hex,
        )
        # Used by SharedContext.get_last_proved_block_callback().
        self._shared.set_base_block_info(start_block, base_block_hash_hex)
        self._logger.info(
            f"Initialized deterministic base: base_block_number={start_block} base_block_hash={base_block_hash_hex}"
        )

    def compute_hashes_for_block(self, block_number: int) -> Tuple[str, str]:
        """
        Return (block_hash, parent_block_hash) for a given L2 block number.

        Hashes are computed deterministically from the current base block hash
        (fetched from feeder at configured_start_block - 1) plus an offset.
        """
        self.refresh_base()
        offset = int(block_number) - int(self._base.base_block_number)
        base = int(self._base.base_block_hash_hex, 16)
        block_hash_int = base + (offset + 1)
        parent_hash_int = base + offset
        return format_hex(block_hash_int), format_hex(parent_hash_int)

    def compute_roots_for_block(self, block_number: int) -> Tuple[str, str]:
        """
        Return (new_root, old_root) for a given L2 block number.

        Roots are computed deterministically from the current base state root
        (fetched from feeder at configured_start_block - 1) plus an offset.
        """
        self.refresh_base()
        offset = int(block_number) - int(self._base.base_block_number)
        base = int(self._base.base_state_root_hex, 16)
        old_root_int = base + offset
        new_root_int = base + offset + 1
        return format_hex(new_root_int), format_hex(old_root_int)

    @property
    def base_block_number(self) -> int:
        self.refresh_base()
        return self._base.base_block_number


class BlobTransformer:
    """Transforms incoming blobs into storage documents: block + state_update."""

    def __init__(
        self, *, feeder_client: FeederClient, shared_ctx, chain: DeterministicChain, logger_obj
    ) -> None:
        self._feeder_client = feeder_client
        self._shared = shared_ctx
        self._chain = chain
        self._logger = logger_obj

    @staticmethod
    def get_blob_tx_hashes(blob: Mapping[str, Any]) -> List[str]:
        txs = blob.get("transactions", [])
        out: List[str] = []
        for entry in txs:
            out.append(entry["tx"]["hash_value"])
        return out

    @staticmethod
    def _hex_shift_right(v: Any, shift_bits: int) -> Any:
        """Shift a 0x-prefixed hex string right by shift_bits."""
        return hex(int(v, 16) >> int(shift_bits))

    def _fetch_block_meta(self, block_number: int) -> JsonDict:
        """
        Fetch timestamp and gas prices for a block from shared FGW snapshot if available,
        otherwise from upstream feeder.
        """
        obj = self._shared.get_fgw_block(block_number)
        if obj is None:
            obj = self._feeder_client.get_block(block_number, with_fee_market_info=True)

        return {
            "timestamp": obj["timestamp"],
            "l1_gas_price": obj["l1_gas_price"],
            "l1_data_gas_price": obj["l1_data_gas_price"],
            "l2_gas_price": obj["l2_gas_price"],
        }

    @staticmethod
    def _transform_transactions(tx_entries: list) -> List[JsonDict]:
        """
        Transform feeder tx entries (each with a `tx` object) into the stored tx schema.

        Intentionally strict: relies on expected keys being present and will raise if not.
        """
        out_txs: List[JsonDict] = []
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
                out_txs.append(tx_obj)
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
                deploy_keys = ["contract_address_salt", "class_hash", "constructor_calldata"]
                tx_obj.update({k: tx[k] for k in deploy_keys})
            elif tx_type == TxType.DECLARE:
                declare_keys = ["class_hash", "compiled_class_hash"]
                tx_obj.update({k: tx[k] for k in declare_keys})
            else:  # invoke
                invoke_keys = ["calldata", "account_deployment_data"]
                tx_obj.update({k: tx[k] for k in invoke_keys})

            out_txs.append(tx_obj)
        return out_txs

    def transform_block(self, blob: JsonDict) -> JsonDict:
        """
        Build the stored "block" document from an incoming blob.

        Includes:
        - transactions + receipts (minimal schema expected by downstream consumers)
        - deterministic block hash + parent hash
        - fee market metadata (timestamp + gas prices)
        """
        block_number = int(blob["block_number"])
        tx_entries = blob.get("transactions", [])

        out_txs = self._transform_transactions(tx_entries)

        receipts: List[JsonDict] = []
        for idx, tx in enumerate(out_txs):
            receipts.append(
                {
                    "transaction_index": idx,
                    "transaction_hash": tx["transaction_hash"],
                    "l2_to_l1_messages": [],
                    "events": [],
                    "actual_fee": "0x0",
                }
            )

        out: JsonDict = {
            "block_number": block_number,
            "transactions": out_txs,
            "transaction_receipts": receipts,
        }

        out["block_hash"], out["parent_block_hash"] = self._chain.compute_hashes_for_block(
            block_number
        )
        out.update(CUSTOM_FIELDS)

        bn_for_meta = self._chain.base_block_number
        tx_hashes = self.get_blob_tx_hashes(blob)
        if tx_hashes:
            bn_for_meta = int(self._shared.get_sent_block_number(tx_hashes[0]))

        meta = self._fetch_block_meta(int(bn_for_meta))
        out["timestamp"] = meta["timestamp"]

        l1_price = dict(meta["l1_gas_price"])
        l1_price["price_in_wei"] = self._hex_shift_right(l1_price["price_in_wei"], 1)
        l1_price["price_in_fri"] = self._hex_shift_right(l1_price["price_in_fri"], 1)
        out["l1_gas_price"] = l1_price

        l1_data_price = dict(meta["l1_data_gas_price"])
        l1_data_price["price_in_wei"] = self._hex_shift_right(l1_data_price["price_in_wei"], 1)
        l1_data_price["price_in_fri"] = self._hex_shift_right(l1_data_price["price_in_fri"], 1)
        out["l1_data_gas_price"] = l1_data_price

        out["l2_gas_price"] = meta["l2_gas_price"]
        return out

    def transform_state_update(self, blob: JsonDict, block_number: int) -> JsonDict:
        """
        Build the stored "state_update" document for a blob/block_number pair.
        """
        # Build a state update object from the incoming blob's state_diff
        state_diff = blob["state_diff"]
        nonces_src = state_diff.get("nonces", {})
        storage_updates_src = state_diff.get("storage_updates", {})

        nonces_out = nonces_src.get("L1", {})
        storage_updates_map = storage_updates_src.get("L1", {})

        storage_diffs_out = {
            address: [{"key": k, "value": v} for k, v in (updates).items()]
            for address, updates in storage_updates_map.items()
        }

        new_root, old_root = self._chain.compute_roots_for_block(block_number)
        block_hash, _parent = self._chain.compute_hashes_for_block(block_number)

        # Deployed contracts can come either from state_diff mapping or inferred from txs.
        deployed_contracts_map: JsonDict = {}
        address_to_class = state_diff.get("address_to_class_hash", {})
        for addr, class_hash in address_to_class.items():
            deployed_contracts_map[str(addr)] = class_hash

        tx_entries = blob.get("transactions", [])
        for entry in tx_entries:
            tx = entry["tx"]
            if tx["type"] == TxType.DEPLOY_ACCOUNT:
                addr = tx["sender_address"]
                class_hash = tx["class_hash"]
                deployed_contracts_map[addr] = class_hash

        deployed_contracts_out = [
            {"address": a, "class_hash": c} for a, c in deployed_contracts_map.items()
        ]

        # class_hash_to_compiled_map = state_diff["class_hash_to_compiled_class_hash"]

        # compiled_class_hashes_for_migration = blob.get("compiled_class_hashes_for_migration", [])
        # # Each entry is serialized as [class_hash, compiled_class_hash].
        # migrated_class_hashes = {
        #     entry[0]
        #     for entry in compiled_class_hashes_for_migration
        #     if isinstance(entry, (list, tuple)) and len(entry) > 0
        # }

        # declared_classes_out = [
        #     {"class_hash": class_hash, "compiled_class_hash": compiled_hash}
        #     for class_hash, compiled_hash in class_hash_to_compiled_map.items()
        #     if class_hash not in migrated_class_hashes
        # ]

        declared_classes_out: List[JsonDict] = []

        return {
            "block_hash": block_hash,
            "new_root": new_root,
            "old_root": old_root,
            "state_diff": {
                "storage_diffs": storage_diffs_out,
                "nonces": nonces_out,
                "deployed_contracts": deployed_contracts_out,
                "old_declared_contracts": [],
                "declared_classes": declared_classes_out,
                "replaced_classes": [],
            },
        }

    @staticmethod
    def extract_revert_error_mappings(blob: Mapping[str, Any]) -> Dict[str, Any]:
        """
        Return {tx_hash: revert_error} from the blob, pairing entries by index:
        - execution_infos[i].revert_error
        - transactions[i].tx.hash_value
        """
        tx_entries = blob["transactions"]
        out: Dict[str, Any] = {}
        for idx, item in enumerate(blob["execution_infos"]):
            err = item["revert_error"]
            if err is None:
                continue
            out[tx_entries[idx]["tx"]["hash_value"]] = err
        return out


class EchoCenterService:
    """
    Encapsulates the core logic and state for the Echo Center.

    Flask routes defined at module level delegate to an instance of this class.
    """

    def __init__(
        self,
        feeder_client: FeederClient,
        shared_ctx,
        l1_mgr,
        flask_logger,
        logger,
    ) -> None:
        self.feeder_client = feeder_client
        self.shared = shared_ctx
        self.l1_manager = l1_mgr
        self.flask_logger = flask_logger
        self.logger = logger

        self._chain = DeterministicChain(self.feeder_client, self.shared, self.logger)
        self._transformer = BlobTransformer(
            feeder_client=self.feeder_client,
            shared_ctx=self.shared,
            chain=self._chain,
            logger_obj=self.logger,
        )

    @staticmethod
    def _json_response(payload: Any, status: int = requests.codes.ok) -> Response:
        raw = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        return Response(raw, status=status, headers=[["Content-Type", "application/json"]])

    @staticmethod
    def _parse_block_number(bn: str) -> BlockNumberParam:
        return bn if bn == "latest" else int(bn)

    def _update_tx_tracking_and_reverts(self, blob: dict, block_number: int) -> None:
        hashes = self._transformer.get_blob_tx_hashes(blob)
        for h in hashes:
            self.shared.mark_committed_tx(h, block_number)
        for h, err in self._transformer.extract_revert_error_mappings(blob).items():
            self.shared.add_echonet_revert_error(h, err)

    def handle_write_blob(self):
        """
        POST /cende_recorder/write_blob

        Stores the raw blob plus derived block/state_update documents in SharedContext.
        """
        body = request.get_data()
        self.flask_logger.info(
            f"WRITE_BLOB len={len(body)} ct={request.headers.get('Content-Type')}"
        )

        blob = json.loads(body)
        self._chain.refresh_base()
        block_number = int(blob["block_number"])

        if self.shared.has_block(block_number):
            self.flask_logger.info(f"Duplicate WRITE_BLOB for block {block_number}; no-op")
            return ("", requests.codes.ok)

        self.shared.set_last_block(block_number)
        self.flask_logger.info(f"last_block={block_number}")

        to_store = self._transformer.transform_block(blob)
        state_update = self._transformer.transform_state_update(blob, block_number)

        self.shared.store_block(block_number, blob=blob, block=to_store, state_update=state_update)
        self.flask_logger.info(
            f"block {block_number} tx hashes: {' '.join(self._transformer.get_blob_tx_hashes(blob))}"
        )

        self._update_tx_tracking_and_reverts(blob, block_number)

        return ("", requests.codes.ok)

    def handle_write_pre_confirmed_block(self):
        self.flask_logger.debug("Received pre-confirmed block")
        return ("", requests.codes.ok)

    def handle_report_snapshot(self):
        """Return current in-memory tx tracking snapshot."""
        snap = self.shared.get_report_snapshot()
        return self._json_response(snap, requests.codes.ok)

    def handle_block_dump(self):
        args = request.args.to_dict(flat=True)
        bn = int(args["blockNumber"])
        kind = args.get("kind", "blob")
        payload = self.shared.get_block_field(bn, kind)
        if payload is None:
            return ("", requests.codes.not_found)
        return self._json_response(payload, requests.codes.ok)

    def handle_get_block(self):
        """
        GET /feeder_gateway/get_block

        Serves stored blocks at/after the configured start block, and proxies older blocks to
        the upstream feeder.
        """
        args = request.args.to_dict(flat=True)
        bn_raw = args.get("blockNumber")

        header_only_raw = args.get("headerOnly")
        header_only = header_only_raw == "true" if header_only_raw else False

        wfmi_raw = args.get("withFeeMarketInfo")
        with_fee_market_info = wfmi_raw == "true" if wfmi_raw else None
        bn_parsed = self._parse_block_number(bn_raw)

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
                return ("", requests.codes.not_found)
            obj = self.shared.get_block_field(highest, "block")
            if header_only:
                return self._json_response(
                    {"block_hash": obj["block_hash"], "block_number": highest}, requests.codes.ok
                )
            return self._json_response(obj, requests.codes.ok)

        requested = int(bn_parsed)
        obj = self.shared.get_block_field(requested, "block")
        if obj is not None:
            if header_only:
                return self._json_response(
                    {"block_hash": obj["block_hash"], "block_number": requested}, requests.codes.ok
                )
            return self._json_response(obj, requests.codes.ok)

        return ("", requests.codes.not_found)

    def handle_get_state_update(self):
        args = request.args.to_dict(flat=True)
        bn_raw = args.get("blockNumber")
        bn_parsed = self._parse_block_number(bn_raw)

        if bn_parsed == "latest":
            highest = self.shared.get_latest_block_number()
            if highest is None:
                return ("", requests.codes.not_found)
            state_update = self.shared.get_block_field(highest, "state_update")
            if state_update is not None:
                return self._json_response(state_update, requests.codes.ok)
            return ("", requests.codes.not_found)

        requested = int(bn_parsed)

        state_update = self.shared.get_block_field(requested, "state_update")
        if state_update is not None:
            return self._json_response(state_update, requests.codes.ok)

        return ("", requests.codes.not_found)

    def handle_get_signature(self):
        args = request.args.to_dict(flat=True)
        bn_raw = args.get("blockNumber")
        bn_parsed = self._parse_block_number(bn_raw)

        if bn_parsed == "latest":
            if not self.shared.has_any_blocks():
                return ("", requests.codes.not_found)
            return self._json_response(SIGNATURE_CONST, requests.codes.ok)

        exists = self.shared.has_block(bn_parsed)
        if exists:
            return self._json_response(SIGNATURE_CONST, requests.codes.ok)

        return ("", requests.codes.not_found)

    def handle_get_class_by_hash(self):
        args = request.args.to_dict(flat=True)
        bn_raw = args.get("blockNumber")
        class_hash = args["classHash"]
        obj = self.feeder_client.get_class_by_hash(class_hash, block_number=bn_raw)
        return self._json_response(obj, requests.codes.ok)

    def handle_get_compiled_class_by_class_hash(self):
        args = request.args.to_dict(flat=True)
        bn_raw = args.get("blockNumber")
        class_hash = args["classHash"]
        obj = self.feeder_client.get_compiled_class_by_class_hash(class_hash, block_number=bn_raw)
        return self._json_response(obj, requests.codes.ok)

    def handle_l1(self):
        """
        L1 endpoint used as a JSON-RPC entrypoint.

        - For JSON-RPC calls (POST with a body containing a "method" field), dispatch the method
          to the same handlers used by the explicit eth_* HTTP endpoints below.
        - For any other request, just return 200 with an empty body.
        """
        data = request.get_json(silent=True) or {}
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
            self.logger.info(f"eth_getBlockByNumber payload: {payload}")
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


app = Flask(__name__)
feeder_client = FeederClient()
service = EchoCenterService(
    feeder_client=feeder_client,
    shared_ctx=shared,
    l1_mgr=l1_manager,
    flask_logger=flask_logger,
    logger=logger,
)


@app.route("/cende_recorder/write_blob", methods=["POST"])
def write_blob():
    return service.handle_write_blob()


@app.route("/cende_recorder/write_pre_confirmed_block", methods=["POST"])
def write_pre_confirmed_block():
    return service.handle_write_pre_confirmed_block()


@app.route("/echonet/report", methods=["GET"])
def report_snapshot():
    return service.handle_report_snapshot()


@app.route("/echonet/block_dump", methods=["GET"])
def block_dump():
    return service.handle_block_dump()


@app.route("/feeder_gateway/get_block", methods=["GET"])
def get_block():
    return service.handle_get_block()


@app.route("/feeder_gateway/get_state_update", methods=["GET"])
def get_state_update():
    return service.handle_get_state_update()


@app.route("/feeder_gateway/get_signature", methods=["GET"])
def get_signature():
    return service.handle_get_signature()


@app.route("/feeder_gateway/get_class_by_hash", methods=["GET"])
def get_class_by_hash():
    return service.handle_get_class_by_hash()


@app.route("/feeder_gateway/get_compiled_class_by_class_hash", methods=["GET"])
def get_compiled_class_by_class_hash():
    return service.handle_get_compiled_class_by_class_hash()


@app.route("/l1", methods=["GET", "POST"])
def l1():
    return service.handle_l1()


# Start the transaction sender automatically on startup.
# Werkzeug is the WSGI/server toolkit Flask uses.
# Werkzeug sets WERKZEUG_RUN_MAIN="true" in the child, so this guard prevents
# starting the background sender twice during local development.
if os.environ.get("WERKZEUG_RUN_MAIN") in (None, "true"):
    start_background_sender()


if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8000)
