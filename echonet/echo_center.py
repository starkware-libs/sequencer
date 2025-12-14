import json
import os
from typing import Any, Dict, Optional

import consts
from feeder_client import FeederClient
from flask import Flask, Response, request  # pyright: ignore[reportMissingImports]
from logger import get_flask_logger, get_logger
from shared_context import l1_manager, shared
from transaction_sender import start_background_sender

app = Flask(__name__)
feeder_client = FeederClient()

flask_logger = get_flask_logger()
logger = get_logger("echo_center")


CUSTOM_FIELDS = {
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


SIGNATURE_CONST: Dict[str, Any] = {
    "block_hash": "0x8470dbc0e524e713c926511c3b1b5c8512b083f925bf0bd247f0a46ed91a4a",
    "signature": [
        "0x5447b6b452f704af805b2166f863c3b31a0b864f56ed2e5adce5fe64fec162e",
        "0x61dee68ee8a9ade268bc6b3515484350d29dd6ffc5804ff300eda40460575ef",
    ],
}


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

        self._base_block_number: int = consts.START_BLOCK_DEFAULT
        self._base_block_hash_hex: Optional[str] = None
        self._base_state_root_hex: Optional[str] = None

        self._init_base_values()

    @staticmethod
    def _format_0x_hex(value: int, width: int = 64) -> str:
        mod = 1 << (width * 4)
        v = value % mod
        return "0x" + format(v, f"0{width}x")

    @staticmethod
    def _json_response(payload: Any, status: int = consts.HTTP_OK) -> Response:
        raw = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        return Response(raw, status=status, headers=[["Content-Type", "application/json"]])

    @staticmethod
    def _parse_block_number(bn_raw: str):
        bn = bn_raw.strip()
        return "latest" if bn.lower() == "latest" else int(bn)

    def _init_base_values(self) -> None:
        """
        Initialize base block_hash and state_root from feeder for
        (consts.START_BLOCK_DEFAULT - 1).
        """
        block = self.feeder_client.get_block(self._base_block_number - 1)
        self._base_block_hash_hex = block["block_hash"]
        self._base_state_root_hex = block["state_root"]

    def _refresh_base_if_needed(self) -> None:
        """
        If consts.START_BLOCK_DEFAULT changed (after resync), refresh base
        number and hashes.
        """
        if self._base_block_number != consts.START_BLOCK_DEFAULT:
            self._base_block_number = consts.START_BLOCK_DEFAULT
            self._init_base_values()

    def _fetch_block_meta_from_shared(self, block_number: int) -> Optional[Dict[str, Any]]:
        """
        Fetch timestamp and gas prices for a block from shared fgw snapshot if available.
        """
        obj = self.shared.get_fgw_block(block_number)
        if not isinstance(obj, dict):
            obj = self.feeder_client.get_block(block_number, with_fee_market_info=True)
        return {
            "timestamp": obj.get("timestamp"),
            "l1_gas_price": (dict(obj.get("l1_gas_price"))),
            "l1_data_gas_price": (dict(obj.get("l1_data_gas_price"))),
            "l2_gas_price": (dict(obj.get("l2_gas_price"))),
        }

    def _compute_hashes_for_block(self, block_number: int):
        # Compute deterministic hashes based on offset from the configured base block number
        offset = int(block_number) - int(self._base_block_number)
        base_hex = self._base_block_hash_hex
        base = int(base_hex, 16) if base_hex else 0
        block_hash_int = base + (offset + 1)
        parent_hash_int = base + offset
        return self._format_0x_hex(block_hash_int), self._format_0x_hex(parent_hash_int)

    def _compute_roots_for_block(self, block_number: int):
        # Compute deterministic roots based on offset from the configured base block number
        offset = int(block_number) - int(self._base_block_number)
        base_hex = self._base_state_root_hex
        base = int(base_hex, 16) if base_hex else 0
        old_root_int = base + offset
        new_root_int = base + offset + 1
        return self._format_0x_hex(new_root_int), self._format_0x_hex(old_root_int)

    @staticmethod
    def _get_blob_tx_hashes(blob: dict):
        return [tx["tx"]["hash_value"] for tx in blob.get("transactions", [])]

    def _transform_blob_for_storage(self, blob: dict):
        block_number = int(blob["block_number"])

        def _hex_div(v, divisor: int):
            if isinstance(v, str):
                return hex(max(int(v, 16) >> divisor, 0))
            return v

        tx_entries = blob["transactions"]
        out_txs = []
        for entry in tx_entries:
            tx_type = tx.get("type")

            # L1 handler transactions should match the feeder-gateway L1 handler
            # schema and not include account-transaction specific fields such as
            # paymaster data or resource bounds.
            if tx_type == "L1_HANDLER":
                l1_handler_keys = [
                    "version",
                    "nonce",
                    "contract_address",
                    "entry_point_selector",
                    "calldata",
                    "type",
                ]
                tx_obj = {k: tx[k] for k in l1_handler_keys if k in tx}
                tx_obj["transaction_hash"] = tx["hash_value"]
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
            if tx_type == "DEPLOY_ACCOUNT":
                deploy_pass_through = [
                    "contract_address_salt",
                    "class_hash",
                    "constructor_calldata",
                ]
                tx_obj.update({k: tx[k] for k in deploy_pass_through})
            else:
                invoke_pass_through = ["calldata", "account_deployment_data"]
                tx_obj.update({k: tx[k] for k in invoke_pass_through})
            out_txs.append(tx_obj)

        receipts = []
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

        out = {
            "block_number": block_number,
            "transactions": out_txs,
            "transaction_receipts": receipts,
        }
        block_hash, parent_block_hash = self._compute_hashes_for_block(block_number)
        out["block_hash"] = block_hash
        out["parent_block_hash"] = parent_block_hash

        out.update(CUSTOM_FIELDS)

        bn_for_meta = self._base_block_number
        tx_hashes_for_bn = self._get_blob_tx_hashes(blob)
        if tx_hashes_for_bn:
            bn_for_meta = self.shared.get_sent_block_number(tx_hashes_for_bn[0])
        meta = self._fetch_block_meta_from_shared(bn_for_meta)
        out["timestamp"] = meta.get("timestamp")
        l1_price = dict(meta["l1_gas_price"])
        l1_price["price_in_wei"] = _hex_div(l1_price["price_in_wei"], 1)
        l1_price["price_in_fri"] = _hex_div(l1_price["price_in_fri"], 1)
        out["l1_gas_price"] = l1_price
        l1_data_price = dict(meta["l1_data_gas_price"])
        l1_data_price["price_in_wei"] = _hex_div(l1_data_price["price_in_wei"], 1)
        l1_data_price["price_in_fri"] = _hex_div(l1_data_price["price_in_fri"], 1)
        out["l1_data_gas_price"] = l1_data_price
        out["l2_gas_price"] = meta["l2_gas_price"]

        return out

    def _transform_state_update_from_blob(self, blob: dict, block_number: int):
        # Build a state update document from the incoming blob's state_diff
        state_diff = blob["state_diff"]
        nonces_src = state_diff["nonces"]
        storage_updates_src = state_diff["storage_updates"]

        nonces_out = nonces_src["L1"]
        storage_updates_map = storage_updates_src["L1"]

        storage_diffs_out = {
            address: [{"key": k, "value": v} for k, v in updates.items()]
            for address, updates in storage_updates_map.items()
        }

        new_root, old_root = self._compute_roots_for_block(block_number)
        block_hash, _parent = self._compute_hashes_for_block(block_number)

        deployed_contracts_map = {}
        address_to_class = blob.get("state_diff", {}).get("address_to_class_hash", {}) or {}
        for addr, class_hash in address_to_class.items():
            deployed_contracts_map[addr] = class_hash
        tx_entries = blob.get("transactions", []) or []
        for entry in tx_entries:
            tx = entry["tx"] if "tx" in entry else entry
            if tx.get("type") == "DEPLOY_ACCOUNT":
                addr = tx.get("sender_address")
                class_hash = tx.get("class_hash")
                deployed_contracts_map[addr] = class_hash

        deployed_contracts_out = [
            {"address": a, "class_hash": c} for a, c in deployed_contracts_map.items()
        ]

        # Build declared_classes from class_hash_to_compiled_class_hash mapping if present.
        # Note: class_hash_to_compiled_class_hash in the blob includes both:
        #   - newly declared classes, and
        #   - classes whose compiled class hash was migrated.
        # The subset corresponding to migrations is given explicitly by
        # compiled_class_hashes_for_migration at the top level of the blob, so we
        # must exclude those from declared_classes.
        class_hash_to_compiled_map = state_diff.get("class_hash_to_compiled_class_hash", {}) or {}

        compiled_class_hashes_for_migration = blob.get("compiled_class_hashes_for_migration") or []
        # Each entry is serialized as [class_hash, compiled_class_hash].
        migrated_class_hashes = {
            entry[0]
            for entry in compiled_class_hashes_for_migration
            if isinstance(entry, (list, tuple)) and len(entry) >= 1
        }

        declared_classes_out = [
            {"class_hash": class_hash, "compiled_class_hash": compiled_hash}
            for class_hash, compiled_hash in class_hash_to_compiled_map.items()
            if class_hash not in migrated_class_hashes
        ]
        declared_classes_out = []

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
    def _extract_revert_error_mappings(blob: dict):
        """
        Return list of objects mapping {hash_value: revert_error}, pairing entries by index:
        - execution_infos[i].revert_error
        - transactions[i].tx.hash_value
        """
        infos = blob.get("execution_infos", [])
        tx_entries = blob.get("transactions", [])
        out = []
        for idx, item in enumerate(infos):
            err = item.get("revert_error")
            if err is None:
                continue
            hash_value = tx_entries[idx]["tx"]["hash_value"]
            out.append({hash_value: err})
        return out

    def _update_tx_tracking_and_reverts(self, blob: dict, block_number: int) -> None:
        hashes = self._get_blob_tx_hashes(blob)
        if hashes:
            # Update in-memory committed map and evict from sent set
            for h in hashes:
                self.shared.mark_committed_tx(h, block_number)
        mappings = self._extract_revert_error_mappings(blob)
        if mappings:
            # Update in-memory revert map for echonet-side reverts
            # Only add echonet_error if there isn't already a mainnet_error.
            # If there is a mainnet_error, remove the entry entirely.
            for m in mappings:
                for h, err in m.items():
                    self.shared.add_echonet_revert_error(h, err)

    # Public handlers used by Flask routes

    def handle_write_blob(self):
        body = request.get_data()
        self.flask_logger.info(
            f"WRITE_BLOB len={len(body)} ct={request.headers.get('Content-Type')}"
        )

        # Parse body JSON
        blob = json.loads(body)
        self._refresh_base_if_needed()
        block_number = int(blob["block_number"])
        self.shared.set_last_block(block_number)
        self.flask_logger.info(f"last_block={block_number}")

        # Transform and store selected data
        to_store = self._transform_blob_for_storage(blob)
        state_update = self._transform_state_update_from_blob(blob, block_number)

        self.shared.store_block(block_number, blob=blob, block=to_store, state_update=state_update)
        self.flask_logger.info(
            f"block {block_number} tx hashes: {' '.join(self._get_blob_tx_hashes(blob))}"
        )

        self._update_tx_tracking_and_reverts(blob, block_number)

        return ("", consts.HTTP_OK)

    def handle_write_pre_confirmed_block(self):
        self.flask_logger.info("Received pre-confirmed block")
        return ("", consts.HTTP_OK)

    def handle_report_snapshot(self):
        """Return current in-memory tx tracking snapshot."""
        snap = self.shared.get_report_snapshot()
        return self._json_response(snap, consts.HTTP_OK)

    def handle_block_dump(self):
        args = request.args.to_dict(flat=True)
        bn_raw = args.get("blockNumber")
        kind = args.get("kind", "blob")
        try:
            bn = int(bn_raw)
        except Exception:
            return ("", consts.HTTP_BAD_REQUEST)
        if kind not in ("blob", "block", "state_update"):
            return ("", consts.HTTP_BAD_REQUEST)
        payload = self.shared.get_block_field(bn, kind)
        if payload is None:
            return ("", consts.HTTP_NOT_FOUND)
        return self._json_response(payload, consts.HTTP_OK)

    def handle_get_block(self):
        self._refresh_base_if_needed()
        args = request.args.to_dict(flat=True)
        bn_raw = args.get("blockNumber")

        header_only_raw = args.get("headerOnly")
        header_only = header_only_raw.lower() == "true" if header_only_raw else False

        wfmi_raw = args.get("withFeeMarketInfo")
        with_fee_market_info = wfmi_raw.lower() == "true" if wfmi_raw else None
        bn_parsed = self._parse_block_number(bn_raw)

        # If explicitly requesting a block older than our configured starting block,
        # return it directly from the upstream feeder.
        if isinstance(bn_parsed, int) and bn_parsed < self._base_block_number:
            upstream_obj = self.feeder_client.get_block(
                bn_parsed, header_only=header_only, with_fee_market_info=with_fee_market_info
            )
            return self._json_response(upstream_obj, consts.HTTP_OK)

        if bn_parsed == "latest":
            highest = self.shared.get_latest_block_number()
            if highest is None:
                return ("", consts.HTTP_NOT_FOUND)
            obj = self.shared.get_block_field(highest, "block")
            if header_only:
                stored_bh = obj.get("block_hash")
                if not stored_bh:
                    stored_bh, _ = self._compute_hashes_for_block(highest)
                return self._json_response(
                    {"block_hash": stored_bh, "block_number": highest}, consts.HTTP_OK
                )
            return self._json_response(obj, consts.HTTP_OK)

        requested = int(bn_parsed)
        obj = self.shared.get_block_field(requested, "block")
        if obj is not None:
            if header_only:
                stored_bh = obj.get("block_hash")
                if not stored_bh:
                    stored_bh, _ = self._compute_hashes_for_block(requested)
                return self._json_response(
                    {"block_hash": stored_bh, "block_number": requested}, consts.HTTP_OK
                )
            return self._json_response(obj, consts.HTTP_OK)

        return ("", consts.HTTP_NOT_FOUND)

    def handle_get_state_update(self):
        self._refresh_base_if_needed()
        args = request.args.to_dict(flat=True)
        bn_raw = args.get("blockNumber")
        bn_parsed = self._parse_block_number(bn_raw)

        if bn_parsed == "latest":
            highest = self.shared.get_latest_block_number()
            if highest is None:
                return ("", consts.HTTP_NOT_FOUND)
            state_update = self.shared.get_block_field(highest, "state_update")
            if state_update is not None:
                return self._json_response(state_update, consts.HTTP_OK)
            return ("", consts.HTTP_NOT_FOUND)

        requested = int(bn_parsed)

        state_update = self.shared.get_block_field(requested, "state_update")
        if state_update is not None:
            return self._json_response(state_update, consts.HTTP_OK)

        return ("", consts.HTTP_NOT_FOUND)

    def handle_get_signature(self):
        self._refresh_base_if_needed()
        args = request.args.to_dict(flat=True)
        bn_raw = args.get("blockNumber")
        bn_parsed = self._parse_block_number(bn_raw)

        if bn_parsed == "latest":
            if not self.shared.has_any_blocks():
                return ("", consts.HTTP_NOT_FOUND)
            return self._json_response(SIGNATURE_CONST, consts.HTTP_OK)

        requested = int(bn_parsed)
        exists = self.shared.has_block(requested)
        if exists:
            return self._json_response(SIGNATURE_CONST, consts.HTTP_OK)

        return ("", consts.HTTP_NOT_FOUND)

    def handle_get_class_by_hash(self):
        args = request.args.to_dict(flat=True)
        bn_raw = args.get("blockNumber")
        if bn_raw.lower() == "pending":
            class_hash = args.get("classHash") or args.get("class_hash")
            if not isinstance(class_hash, str) or not class_hash:
                return ("", consts.HTTP_NOT_FOUND)
            obj = self.feeder_client.get_class_by_hash(class_hash, block_number="pending")
            return self._json_response(obj, consts.HTTP_OK)
        return ("", consts.HTTP_NOT_FOUND)

    def handle_get_compiled_class_by_class_hash(self):
        """
        Proxy compiled class lookups by class hash to the upstream feeder-gateway.
        Currently only supports blockNumber=pending, matching handle_get_class_by_hash.
        """
        args = request.args.to_dict(flat=True)
        bn_raw = args.get("blockNumber")
        if bn_raw.lower() == "pending":
            class_hash = args.get("classHash") or args.get("class_hash")
            if not isinstance(class_hash, str) or not class_hash:
                return ("", consts.HTTP_NOT_FOUND)
            obj = self.feeder_client.get_compiled_class_by_class_hash(
                class_hash, block_number="pending"
            )
            return self._json_response(obj, consts.HTTP_OK)
        return ("", consts.HTTP_NOT_FOUND)

    def handle_l1(self):
        """
        L1 endpoint used as a JSON-RPC entrypoint.

        - For JSON-RPC calls (POST with a body containing a "method" field), dispatch the method
          to the same handlers used by the explicit eth_* HTTP endpoints below.
        - For any other request, just return 200 with an empty body.
        """
        if request.method != "POST":
            return ("", consts.HTTP_OK)

        data = request.get_json(silent=True) or {}
        method = data.get("method")
        rpc_id = data.get("id", 1)
        self.logger.info(f"Method: {method}")
        if not isinstance(method, str):
            return ("", consts.HTTP_OK)

        # JSON-RPC params can be an array or an object; keep both forms for logging,
        # but normalize to a dict-like value where convenient.
        raw_params = data.get("params")
        self.logger.info(f"Raw params: {raw_params}")
        if isinstance(raw_params, list) and raw_params:
            params = raw_params[0]
        elif isinstance(raw_params, dict):
            params = raw_params
        else:
            params = {}

        if method == "eth_blockNumber":
            payload = self.l1_manager.get_block_number()
            logger.info(f"eth_blockNumber payload: {payload}")
            return self._json_response(payload, consts.HTTP_OK)

        if method == "eth_getBlockByNumber":
            payload = self.l1_manager.get_block_by_number(params)
            logger.info(f"eth_getBlockByNumber payload: {payload}")
            return self._json_response(payload, consts.HTTP_OK)

        if method == "eth_getLogs":
            payload = self.l1_manager.get_logs(params if isinstance(params, dict) else {})
            logger.info(f"eth_getLogs payload: {payload}")
            return self._json_response(payload, consts.HTTP_OK)

        if method == "eth_call":
            # Return the base block number - 1 (used for initializing base hashes)
            # encoded as a 32-byte word, so that it is ABI-decodable by clients.
            result_word = self._format_0x_hex(int(self._base_block_number), width=64)
            payload = {"jsonrpc": "2.0", "id": rpc_id, "result": result_word}
            logger.info(f"eth_call payload: {payload}")
            return self._json_response(payload, consts.HTTP_OK)

        # Fallback for unimplemented JSON-RPC methods: return a proper JSON-RPC
        # error object instead of an empty body, so clients don't fail with
        # "EOF while parsing a value".
        error_payload = {
            "jsonrpc": "2.0",
            "id": rpc_id,
            "error": {"code": -32601, "message": f"Method {method} not implemented"},
        }
        logger.info(f"Unhandled JSON-RPC method {method}, returning error payload: {error_payload}")
        return self._json_response(error_payload, consts.HTTP_OK)


service = EchoCenterService(
    feeder_client=feeder_client,
    shared_ctx=shared,
    l1_mgr=l1_manager,
    flask_logger=flask_logger,
    logger=logger,
)

# Start the transaction sender automatically on startup. The internal
# TransactionSenderRunner is idempotent, so calling this more than once
# in a given process is safe; the underlying runner will simply no-op
# if already running.
if os.environ.get("WERKZEUG_RUN_MAIN") in (None, "true"):
    start_background_sender()


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


if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8000)
