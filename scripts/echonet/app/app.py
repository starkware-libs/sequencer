import json  # pyright: ignore[reportMissingModuleSource]
import os
import pathlib
from typing import Any, Dict

import requests
import threading
from flask import Flask, Response, jsonify, request  # pyright: ignore[reportMissingImports]

app = Flask(__name__)
_state = {"last_block": 0}
_lock = threading.Lock()
_session = requests.Session()

# Directory where per-block JSON files are stored.
_BLOCKS_DIR = "data/blocks"
# Area to add arbitrary key/value pairs to each stored block JSON
CUSTOM_FIELDS = {
    "block_hash": "0x49511028a81925b41f7807a9165f7a982eacfcf0a678a1c8e37eabb5b3490cf",
    "parent_block_hash": "0x49511028a81925b41f7807a9165f7a982eacfcf0a678a1c8e37eabb5b3490cf",
    "state_root": "0x6138090a2ceae6c179b01b9bbdc13c74d03063cf3f801017cc5fc6bae514881",
    "transaction_commitment": "0x1432ac404fda8b7c921df9c55f1b1539e3f982837a62f6b9db6c843de9f2e85",
    "event_commitment": "0x76747e447201cbd15d044fffff15d30aca29681cef8442ee771cd90692b4b2e",
    "receipt_commitment": "0x107308016a4910c9ad57fc4cf7ce2fc2bfafddd6ca6def98bbfdbeea16429d2",
    "state_diff_commitment": "0x536b1ceb8ddb02e4a94ecab8ef316c828bcf657053ac160784e843d986ca10",
    "state_diff_length": 159,
    "status": "ACCEPTED_ON_L1",
    "l1_da_mode": "BLOB",
    "timestamp": 1761987400,
    "sequencer_address": "0x1176a1bd84444c89232ec27754698e5d2e7e1a7f1539f12027f28b23ec9f3d8",
    "starknet_version": "0.14.0",
    "l2_gas_consumed": 606837191,
    "next_l2_gas_price": "0xb2d05e0",
}

# Constant signature response content for local blocks
_SIGNATURE_CONST = {
    "block_hash": "0x49511028a81925b41f7807a9165f7a982eacfcf0a678a1c8e37eabb5b3490cf",
    "signature": [
        "0x5447b6b452f704af805b2166f863c3b31a0b864f56ed2e5adce5fe64fec162e",
        "0x61dee68ee8a9ade268bc6b3515484350d29dd6ffc5804ff300eda40460575ef",
    ],
}

from block_storage import BlockStorage

_storage = BlockStorage(_BLOCKS_DIR)


def _format_0x_hex(value: int, width: int = 64) -> str:
    mod = 1 << (width * 4)
    v = value % mod
    return "0x" + format(v, f"0{width}x")


def _compute_hashes_for_block(block_number: int):
    idx = _storage.get_index(block_number)
    if idx is None:
        return None, None
    base_hex = CUSTOM_FIELDS.get("block_hash")
    try:
        base = int(base_hex, 16)
    except Exception:
        base = 0
    # First stored block -> base + 1, parent = base + 0
    block_hash_int = base + (idx + 1)
    parent_hash_int = base + (idx if idx > 0 else 0)
    return _format_0x_hex(block_hash_int), _format_0x_hex(parent_hash_int)


def _compute_roots_for_block(block_number: int):
    idx = _storage.get_index(block_number)
    if idx is None:
        return None, None
    base_hex = CUSTOM_FIELDS.get("state_root")
    try:
        base = int(base_hex, 16) if base_hex else 0
    except Exception:
        base = 0
    old_root_int = base + idx
    new_root_int = base + idx + 1
    return _format_0x_hex(new_root_int), _format_0x_hex(old_root_int)


# Ensure storage directory exists
pathlib.Path(_BLOCKS_DIR).mkdir(parents=True, exist_ok=True)


def _proxy_upstream(upstream: str, args: Dict[str, Any]) -> Response:
    resp = _session.get(upstream, params=args, timeout=20)
    passthru = []
    for h in ("Content-Type", "Content-Length", "ETag", "Cache-Control", "Last-Modified"):
        if h in resp.headers:
            passthru.append((h, resp.headers[h]))
    passthru.append(("X-Dummy-Impl", "flask"))
    return Response(resp.content, status=resp.status_code, headers=passthru)


def _json_response(payload: Any, status: int = 200) -> Response:
    raw = json.dumps(payload, ensure_ascii=False).encode("utf-8")
    return Response(raw, status=status, headers=[["Content-Type", "application/json"]])


def _parse_block_number(bn_raw: str):
    bn = bn_raw.strip()
    return "latest" if bn.lower() == "latest" else int(bn)


def _transform_blob_for_storage(blob: dict):
    # 1) Determine block number
    block_number = blob["block_number"]

    # 2) Extract gas price objects from state_diff.block_info
    block_info = blob.get("state_diff", {}).get("block_info", {})
    l1_gas_price = block_info.get("l1_gas_price")
    l1_data_gas_price = block_info.get("l1_data_gas_price")
    l2_gas_price = block_info.get("l2_gas_price")

    def _hex_div(v, divisor: int):
        if isinstance(v, str):
            try:
                return hex(max(int(v, 16) >> divisor, 0))
            except Exception:
                return v
        return v

    # 3) Transform transactions
    tx_entries = blob.get("transactions", [])
    out_txs = []
    for entry in tx_entries:
        tx = entry.get("tx")
        out_txs.append(
            {
                "transaction_hash": tx.get("hash_value"),
                "version": tx.get("version"),
                "signature": tx.get("signature", []),
                "nonce": tx.get("nonce"),
                "nonce_data_availability_mode": tx.get("nonce_data_availability_mode"),
                "fee_data_availability_mode": tx.get("fee_data_availability_mode"),
                "resource_bounds": tx.get("resource_bounds", {}),
                "tip": tx.get("tip"),
                "paymaster_data": tx.get("paymaster_data", []),
                "sender_address": tx.get("sender_address"),
                "calldata": tx.get("calldata", []),
                "account_deployment_data": tx.get("account_deployment_data", []),
                "type": tx.get("type"),
            }
        )

    # Build transaction receipts aligned with transactions
    receipts = []
    for idx, txo in enumerate(out_txs):
        th = txo.get("transaction_hash")
        receipts.append(
            {
                "transaction_index": idx,
                "transaction_hash": th,
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

    # Attach gas prices at the top level if present
    if l1_gas_price is not None:
        l1_out = dict(l1_gas_price)
        l1_out["price_in_wei"] = _hex_div(l1_out.get("price_in_wei"), 4)
        l1_out["price_in_fri"] = _hex_div(l1_out.get("price_in_fri"), 4)
        out["l1_gas_price"] = l1_out
    if l1_data_gas_price is not None:
        out["l1_data_gas_price"] = l1_data_gas_price
    if l2_gas_price is not None:
        out["l2_gas_price"] = l2_gas_price

    # Merge custom fields last so users can override/add fields deliberately
    if isinstance(CUSTOM_FIELDS, dict) and CUSTOM_FIELDS:
        out.update(CUSTOM_FIELDS)

    return out


# Build a state update document from the incoming blob's compressed_state_diff
def _transform_state_update_from_blob(blob: dict, block_number: int):
    csd = blob.get("compressed_state_diff", {}) or {}
    nonces_src = csd.get("nonces", {}) or {}
    storage_updates_src = csd.get("storage_updates", {}) or {}

    # Prefer the L1 subsection when present; otherwise pass through
    nonces_map = nonces_src.get("L1", nonces_src)
    storage_updates_map = storage_updates_src.get("L1", storage_updates_src)

    # Convert storage updates map-of-maps to map-of-lists with {key,value}
    storage_diffs_out = {}
    if isinstance(storage_updates_map, dict):
        for address, updates in storage_updates_map.items():
            if isinstance(updates, dict):
                lst = [{"key": k, "value": v} for k, v in updates.items()]
            elif isinstance(updates, list):
                lst = updates
            else:
                lst = []
            storage_diffs_out[address] = lst

    # Nonces remain a mapping address -> nonce string
    nonces_out = nonces_map if isinstance(nonces_map, dict) else {}

    # Deterministic roots per stored index
    new_root, old_root = _compute_roots_for_block(block_number)
    block_hash, _parent = _compute_hashes_for_block(block_number)

    return {
        "block_hash": block_hash,
        "new_root": new_root,
        "old_root": old_root,
        "state_diff": {
            "storage_diffs": storage_diffs_out,
            "nonces": nonces_out,
            "deployed_contracts": [],
            "old_declared_contracts": [],
            "declared_classes": [],
            "replaced_classes": [],
        },
    }


def _get_blob_tx_hashes(blob: dict):
    hashes = [tx["tx"]["hash_value"] for tx in blob.get("transactions", [])]
    return hashes


@app.route("/cende_recorder/write_blob", methods=["POST"])
def write_blob():
    body = request.get_data()
    print(f"[FLASK] WRITE_BLOB len={len(body)} ct={request.headers.get('Content-Type')}")

    # Parse body JSON
    blob = json.loads(body)
    bn = int(blob["block_number"])
    with _lock:
        _state["last_block"] = bn
    print(f"[FLASK] last_block={bn}")

    # Transform and store selected data
    to_store = _transform_blob_for_storage(blob)
    su = _transform_state_update_from_blob(blob, bn)
    block_path, _ = _storage.write_block_and_state_update(bn, to_store, su)
    # Also persist the full incoming blob for reference
    _storage.write_blob(bn, blob)
    target = block_path

    print(f"[FLASK] block {bn} tx hashes: {' '.join(_get_blob_tx_hashes(blob))}")

    return (
        jsonify(
            {"ok": True, "stored": True, "block_number": bn, "path": str(target) if target else ""}
        ),
        200,
    )


@app.route("/cende_recorder/write_pre_confirmed_block", methods=["POST"])
def write_pre_confirmed_block():
    body = request.get_data()
    print(
        f"[FLASK] WRITE_PRE_CONFIRMED_BLOCK len={len(body)} ct={request.headers.get('Content-Type')}"
    )
    return jsonify({"ok": True, "len": len(body)}), 200


@app.route("/feeder_gateway/get_block", methods=["GET"])
def get_block():
    args = request.args.to_dict(flat=True)
    bn_raw = args.get("blockNumber")
    if not bn_raw:
        return _json_response({"error": "blockNumber is required"}, 400)
    bn_parsed = _parse_block_number(bn_raw)

    if bn_parsed == "latest":
        highest = _storage.get_highest_block_number()
        if highest is None:
            return ("", 404)
        obj = _storage.read_block(highest)
        if obj is None:
            return ("", 404)
        bh, ph = _compute_hashes_for_block(highest)
        obj["block_hash"] = bh
        obj["parent_block_hash"] = ph
        return _json_response(obj, 200)

    requested = int(bn_parsed)
    obj = _storage.read_block(requested)
    if obj is not None:
        bh, ph = _compute_hashes_for_block(requested)
        obj["block_hash"] = bh
        obj["parent_block_hash"] = ph
        return _json_response(obj, 200)
    lowest = _storage.get_lowest_block_number()
    if lowest is not None and requested < lowest:
        return _proxy_upstream(
            "https://feeder.alpha-mainnet.starknet.io/feeder_gateway/get_block", args
        )

    return ("", 404)


@app.route("/feeder_gateway/get_state_update", methods=["GET"])
def get_state_update():
    args = request.args.to_dict(flat=True)
    bn_raw = args.get("blockNumber")
    if not bn_raw:
        return _json_response({"error": "blockNumber is required"}, 400)
    bn_parsed = _parse_block_number(bn_raw)

    if bn_parsed == "latest":
        highest = _storage.get_highest_block_number()
        if highest is None:
            return ("", 404)
        su = _storage.read_state_update(highest)
        if su is not None:
            return _json_response(su, 200)
        blk = _storage.read_block(highest)
        if blk is not None:
            su = _transform_state_update_from_blob(blk, highest)
            return _json_response(su, 200)
        return ("", 404)

    requested = int(bn_parsed)

    su = _storage.read_state_update(requested)
    if su is not None:
        return _json_response(su, 200)

    # older-than-all -> proxy upstream
    lowest = _storage.get_lowest_block_number()
    if lowest is not None and requested < lowest:
        return _proxy_upstream(
            "https://feeder.alpha-mainnet.starknet.io/feeder_gateway/get_state_update", args
        )

    return ("", 404)


@app.route("/feeder_gateway/get_signature", methods=["GET"])
def get_signature():
    args = request.args.to_dict(flat=True)
    bn_raw = args.get("blockNumber")
    if not bn_raw:
        return _json_response({"error": "blockNumber is required"}, 400)
    bn_parsed = _parse_block_number(bn_raw)

    if bn_parsed == "latest":
        highest = _storage.get_highest_block_number()
        if highest is None:
            return ("", 404)
        return _json_response(_SIGNATURE_CONST, 200)

    requested = int(bn_parsed)
    if _storage.contains_block(requested):
        return _json_response(_SIGNATURE_CONST, 200)

    lowest = _storage.get_lowest_block_number()
    if lowest is not None and requested < lowest:
        return _proxy_upstream(
            "https://feeder.alpha-mainnet.starknet.io/feeder_gateway/get_signature", args
        )

    return ("", 404)


def _purge_stored_blocks():
    return _storage.purge()


if __name__ == "__main__":
    import sys

    if len(sys.argv) > 1 and sys.argv[1] == "purge-stored-blocks":
        n = _purge_stored_blocks()
        print(f"Purged {n} stored block files from {_BLOCKS_DIR}")
    else:
        port = int(os.environ.get("PORT", "8000"))
        app.run(host="0.0.0.0", port=port)
