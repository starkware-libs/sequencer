import json  # pyright: ignore[reportMissingModuleSource]
import os
import pathlib
import time
from typing import Any, Dict, Optional

import atexit
import threading
from consts import (
    CUSTOM_FIELDS,
    SIGNATURE_CONST,
    START_BLOCK_DEFAULT,
    TX_SENDER_CONTROL_FILE,
    TX_SENDER_STATUS_FILE,
)
from feeder_client import FeederClient
from flask import Flask, Response, request  # pyright: ignore[reportMissingImports]
from shared_context import shared
from transaction_sender import (
    is_sender_running,
    start_background_sender,
    stop_background_sender,
)

app = Flask(__name__)
_state = {"last_block": 0}
_lock = threading.Lock()
feeder_client = FeederClient()

_BASE_BLOCK_NUMBER = START_BLOCK_DEFAULT
_BASE_BLOCK_HASH_HEX: Optional[str] = None
_BASE_STATE_ROOT_HEX: Optional[str] = None

_control_thread_started = False
_control_thread_lock = threading.Lock()


def _write_status_file(content: str) -> None:
    try:
        p = pathlib.Path(TX_SENDER_STATUS_FILE)
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(content, encoding="utf-8")
    except Exception:
        pass


def _control_loop() -> None:
    """Background control loop reading commands from a file to start/stop the tx sender."""
    ctrl_path = pathlib.Path(TX_SENDER_CONTROL_FILE)
    ctrl_path.parent.mkdir(parents=True, exist_ok=True)
    while True:
        try:
            if ctrl_path.exists():
                try:
                    data = ctrl_path.read_text(encoding="utf-8").strip().lower()
                except Exception:
                    data = ""
                if data:
                    # Clear the command file immediately to avoid reprocessing
                    try:
                        ctrl_path.write_text("", encoding="utf-8")
                    except Exception:
                        pass

                    if data.startswith("start"):
                        started = start_background_sender()
                        _write_status_file(f"running={is_sender_running()} started={started}")
                    elif data.startswith("stop"):
                        stopped = stop_background_sender(join_timeout_seconds=2.0)
                        _write_status_file(f"running={is_sender_running()} stopped={stopped}")
                    elif data.startswith("status"):
                        _write_status_file(f"running={is_sender_running()}")
        except Exception:
            pass
        time.sleep(1.0)


def _start_control_thread_once() -> None:
    global _control_thread_started
    with _control_thread_lock:
        if _control_thread_started:
            return
        # Avoid duplicate thread under Flask dev reloader
        if os.environ.get("WERKZEUG_RUN_MAIN") not in (None, "true"):
            # In the reloader parent process; don't start
            return
        t = threading.Thread(target=_control_loop, name="TxSenderControl", daemon=True)
        t.start()
        atexit.register(lambda: _write_status_file("running=false"))
        _control_thread_started = True


def _format_0x_hex(value: int, width: int = 64) -> str:
    mod = 1 << (width * 4)
    v = value % mod
    return "0x" + format(v, f"0{width}x")


def _json_response(payload: Any, status: int = 200) -> Response:
    raw = json.dumps(payload, ensure_ascii=False).encode("utf-8")
    return Response(raw, status=status, headers=[["Content-Type", "application/json"]])


def _parse_block_number(bn_raw: str):
    bn = bn_raw.strip()
    return "latest" if bn.lower() == "latest" else int(bn)


def _init_base_values() -> None:
    """Initialize base block_hash and state_root from feeder for (START_BLOCK_DEFAULT - 1)."""
    global _BASE_BLOCK_HASH_HEX, _BASE_STATE_ROOT_HEX
    block = feeder_client.get_block(_BASE_BLOCK_NUMBER - 1)
    _BASE_BLOCK_HASH_HEX = block["block_hash"]
    _BASE_STATE_ROOT_HEX = block["state_root"]


def _fetch_block_meta_from_shared(block_number: int) -> Optional[Dict[str, Any]]:
    """Fetch timestamp and gas prices for a block from shared fgw snapshot if available."""
    obj = shared.get_fgw_block(block_number)
    if not isinstance(obj, dict):
        obj = feeder_client.get_block(block_number, with_fee_market_info=True)
    return {
        "timestamp": obj.get("timestamp"),
        "l1_gas_price": obj.get("l1_gas_price"),
        "l1_data_gas_price": obj.get("l1_data_gas_price"),
        "l2_gas_price": obj.get("l2_gas_price"),
    }


_init_base_values()

_start_control_thread_once()


def _compute_hashes_for_block(block_number: int):
    # Compute deterministic hashes based on offset from the configured base block number
    offset = int(block_number) - int(_BASE_BLOCK_NUMBER)
    base_hex = _BASE_BLOCK_HASH_HEX
    base = int(base_hex, 16) if base_hex else 0
    block_hash_int = base + (offset + 1)
    parent_hash_int = base + offset
    return _format_0x_hex(block_hash_int), _format_0x_hex(parent_hash_int)


def _compute_roots_for_block(block_number: int):
    # Compute deterministic roots based on offset from the configured base block number
    offset = int(block_number) - int(_BASE_BLOCK_NUMBER)
    base_hex = _BASE_STATE_ROOT_HEX
    base = int(base_hex, 16) if base_hex else 0
    old_root_int = base + offset
    new_root_int = base + offset + 1
    return _format_0x_hex(new_root_int), _format_0x_hex(old_root_int)


def _get_blob_tx_hashes(blob: dict):
    return [tx["tx"]["hash_value"] for tx in blob.get("transactions", [])]


def _transform_blob_for_storage(blob: dict):
    block_number = int(blob["block_number"])

    def _hex_div(v, divisor: int):
        if isinstance(v, str):
            return hex(max(int(v, 16) >> divisor, 0))
        return v

    tx_entries = blob["transactions"]
    out_txs = []
    for entry in tx_entries:
        tx = entry["tx"]
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
        # Specialize DEPLOY_ACCOUNT to FeederGateway shape
        if tx["type"] == "DEPLOY_ACCOUNT":
            deploy_pass_through = ["contract_address_salt", "class_hash", "constructor_calldata"]
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
    block_hash, parent_block_hash = _compute_hashes_for_block(block_number)
    out["block_hash"] = block_hash
    out["parent_block_hash"] = parent_block_hash

    out.update(CUSTOM_FIELDS)

    bn_for_meta = START_BLOCK_DEFAULT
    tx_hashes_for_bn = _get_blob_tx_hashes(blob)
    if tx_hashes_for_bn:
        bn_for_meta = shared.get_sent_block_number(tx_hashes_for_bn[0])
    meta = _fetch_block_meta_from_shared(bn_for_meta)
    out["timestamp"] = meta.get("timestamp")
    l1_price = meta["l1_gas_price"]
    l1_price["price_in_wei"] = _hex_div(l1_price["price_in_wei"], 1)
    l1_price["price_in_fri"] = _hex_div(l1_price["price_in_fri"], 1)
    out["l1_gas_price"] = l1_price
    out["l1_data_gas_price"] = meta["l1_data_gas_price"]
    out["l2_gas_price"] = meta["l2_gas_price"]

    return out


# Build a state update document from the incoming blob's state_diff
def _transform_state_update_from_blob(blob: dict, block_number: int):
    state_diff = blob["state_diff"]
    nonces_src = state_diff["nonces"]
    storage_updates_src = state_diff["storage_updates"]

    nonces_map = nonces_src["L1"]
    storage_updates_map = storage_updates_src["L1"]

    storage_diffs_out = {
        address: [{"key": k, "value": v} for k, v in updates.items()]
        for address, updates in storage_updates_map.items()
    }

    nonces_out = nonces_map

    new_root, old_root = _compute_roots_for_block(block_number)
    block_hash, _parent = _compute_hashes_for_block(block_number)

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

    return {
        "block_hash": block_hash,
        "new_root": new_root,
        "old_root": old_root,
        "state_diff": {
            "storage_diffs": storage_diffs_out,
            "nonces": nonces_out,
            "deployed_contracts": deployed_contracts_out,
            "old_declared_contracts": [],
            "declared_classes": [],
            "replaced_classes": [],
        },
    }


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


def _update_tx_tracking_and_reverts(blob: dict, block_number: int) -> None:
    hashes = _get_blob_tx_hashes(blob)
    if hashes:
        # Update in-memory committed map and evict from sent set
        for h in hashes:
            shared.mark_committed_tx(h, block_number)
    mappings = _extract_revert_error_mappings(blob)
    if mappings:
        # Update in-memory revert map for echonet-side reverts
        # Only add echonet_error if there isn't already a mainnet_error.
        # If there is a mainnet_error, remove the entry entirely (treat as matched on both).
        for m in mappings:
            for h, err in m.items():
                shared.add_echonet_revert_error(h, err)


@app.route("/cende_recorder/write_blob", methods=["POST"])
def write_blob():
    body = request.get_data()
    print(f"[FLASK] WRITE_BLOB len={len(body)} ct={request.headers.get('Content-Type')}")

    # Parse body JSON
    blob = json.loads(body)
    block_number = int(blob["block_number"])
    with _lock:
        _state["last_block"] = block_number
    print(f"[FLASK] last_block={block_number}")

    # Transform and store selected data
    to_store = _transform_blob_for_storage(blob)
    state_update = _transform_state_update_from_blob(blob, block_number)

    shared.store_block(block_number, blob=blob, block=to_store, state_update=state_update)
    print(f"[FLASK] block {block_number} tx hashes: {' '.join(_get_blob_tx_hashes(blob))}")

    _update_tx_tracking_and_reverts(blob, block_number)

    return ("", 200)


@app.route("/cende_recorder/write_pre_confirmed_block", methods=["POST"])
def write_pre_confirmed_block():
    print("Received pre-confirmed block")
    return ("", 200)


@app.route("/echonet/report", methods=["GET"])
def report_snapshot():
    """Return current in-memory tx tracking snapshot."""
    snap = shared.get_report_snapshot()
    payload = {
        **snap,
        "sent_empty": len(snap.get("sent_tx_hashes")) == 0,
        "running": is_sender_running(),
    }
    return _json_response(payload, 200)


@app.route("/echonet/block_dump", methods=["GET"])
def block_dump():
    args = request.args.to_dict(flat=True)
    bn_raw = args.get("blockNumber")
    kind = args.get("kind", "blob")
    try:
        bn = int(bn_raw)
    except Exception:
        return ("", 400)
    if kind not in ("blob", "block", "state_update"):
        return ("", 400)
    payload = shared.get_block_field(bn, kind)
    if payload is None:
        return ("", 404)
    return _json_response(payload, 200)


@app.route("/feeder_gateway/get_block", methods=["GET"])
def get_block():
    args = request.args.to_dict(flat=True)
    bn_raw = args.get("blockNumber")
    # Support headerOnly=true to return only minimal header info
    header_only_raw = args.get("headerOnly")
    header_only = header_only_raw.lower() == "true" if header_only_raw else False
    # Support withFeeMarketInfo=true passthrough to upstream feeder
    wfmi_raw = args.get("withFeeMarketInfo")
    with_fee_market_info = wfmi_raw.lower() == "true" if wfmi_raw else None
    bn_parsed = _parse_block_number(bn_raw)

    # If explicitly requesting a block older than our configured starting block,
    # return it directly from the upstream feeder (honor headerOnly and withFeeMarketInfo).
    if isinstance(bn_parsed, int) and bn_parsed < _BASE_BLOCK_NUMBER:
        upstream_obj = feeder_client.get_block(
            bn_parsed, header_only=header_only, with_fee_market_info=with_fee_market_info
        )
        return _json_response(upstream_obj, 200)

    if bn_parsed == "latest":
        highest = shared.get_latest_block_number()
        if highest is None:
            return ("", 404)
        obj = shared.get_block_field(highest, "block")
        if header_only:
            stored_bh = obj.get("block_hash")
            if not stored_bh:
                stored_bh, _ = _compute_hashes_for_block(highest)
            return _json_response({"block_hash": stored_bh, "block_number": highest}, 200)
        return _json_response(obj, 200)

    requested = int(bn_parsed)
    obj = shared.get_block_field(requested, "block")
    if obj is not None:
        if header_only:
            stored_bh = obj.get("block_hash")
            if not stored_bh:
                stored_bh, _ = _compute_hashes_for_block(requested)
            return _json_response({"block_hash": stored_bh, "block_number": requested}, 200)
        return _json_response(obj, 200)

    return ("", 404)


@app.route("/feeder_gateway/get_state_update", methods=["GET"])
def get_state_update():
    args = request.args.to_dict(flat=True)
    bn_raw = args.get("blockNumber")
    bn_parsed = _parse_block_number(bn_raw)

    if bn_parsed == "latest":
        highest = shared.get_latest_block_number()
        if highest is None:
            return ("", 404)
        state_update = shared.get_block_field(highest, "state_update")
        if state_update is not None:
            return _json_response(state_update, 200)
        return ("", 404)

    requested = int(bn_parsed)

    state_update = shared.get_block_field(requested, "state_update")
    if state_update is not None:
        return _json_response(state_update, 200)

    return ("", 404)


@app.route("/feeder_gateway/get_signature", methods=["GET"])
def get_signature():
    args = request.args.to_dict(flat=True)
    bn_raw = args.get("blockNumber")
    bn_parsed = _parse_block_number(bn_raw)

    if bn_parsed == "latest":
        has_any = shared.has_any_blocks()
        if not has_any:
            return ("", 404)
        return _json_response(SIGNATURE_CONST, 200)

    requested = int(bn_parsed)
    exists = shared.has_block(requested)
    if exists:
        return _json_response(SIGNATURE_CONST, 200)

    return ("", 404)


@app.route("/feeder_gateway/get_class_by_hash", methods=["GET"])
def get_class_by_hash():
    args = request.args.to_dict(flat=True)
    bn_raw = args.get("blockNumber")
    if bn_raw.lower() == "pending":
        class_hash = args.get("classHash") or args.get("class_hash")
        if not isinstance(class_hash, str) or not class_hash:
            return ("", 404)
        obj = feeder_client.get_class_by_hash(class_hash, block_number="pending")
        return _json_response(obj, 200)
    return ("", 404)


if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8000)
