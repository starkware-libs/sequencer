import json  # pyright: ignore[reportMissingModuleSource]
import os
import pathlib
from typing import Any, Dict, Optional

import requests
import threading
from bisect import bisect_left
from flask import Flask, Response, jsonify, request  # pyright: ignore[reportMissingImports]

app = Flask(__name__)
_state = {"last_block": 0}
_lock = threading.Lock()
_session = requests.Session()

# Directory where per-block JSON files are stored.
_BLOCKS_DIR = os.environ.get("BLOCKS_DIR", "data/blocks")
# File paths for auxiliary logs (hashes and revert errors) under blocks dir
_HASHES_FILE = os.environ.get("HASHES_FILE", str(pathlib.Path(_BLOCKS_DIR) / "hashes.txt"))
_REVERT_ERRORS_FILE = os.environ.get(
    "REVERT_ERRORS_FILE", str(pathlib.Path(_BLOCKS_DIR) / "errors.jsonl")
)
# Upstream feeder base URL to fetch canonical data (e.g., timestamp) when storing blocks.
_UPSTREAM_FEEDER_BASE_URL = os.environ.get(
    "UPSTREAM_FEEDER_URL", "https://feeder.alpha-mainnet.starknet.io"
)
# Same header used by send_txs.py to avoid throttling (HTTP 429) on feeder requests.
_FEEDER_HEADERS = {"X-Throttling-Bypass": "QYHGVPY7PHER3QHI6LWBY25AGF5GGEZ"}
# Area to add arbitrary key/value pairs to each stored block JSON
CUSTOM_FIELDS = {
    "block_hash": "0x8470dbc0e524e713c926511c3b1b5c8512b083f925bf0bd247f0a46ed91a4a",
    "parent_block_hash": "0x8470dbc0e524e713c926511c3b1b5c8512b083f925bf0bd247f0a46ed91a4a",
    "state_root": "0x6138090a2ceae6c179b01b9bbdc13c74d03063cf3f801017cc5fc6bae514881",
    "transaction_commitment": "0x1432ac404fda8b7c921df9c55f1b1539e3f982837a62f6b9db6c843de9f2e85",
    "event_commitment": "0x76747e447201cbd15d044fffff15d30aca29681cef8442ee771cd90692b4b2e",
    "receipt_commitment": "0x107308016a4910c9ad57fc4cf7ce2fc2bfafddd6ca6def98bbfdbeea16429d2",
    "state_diff_commitment": "0x536b1ceb8ddb02e4a94ecab8ef316c828bcf657053ac160784e843d986ca10",
    "state_diff_length": 159,
    "status": "ACCEPTED_ON_L1",
    "l1_da_mode": "BLOB",
    "sequencer_address": "0x1176a1bd84444c89232ec27754698e5d2e7e1a7f1539f12027f28b23ec9f3d8",
    "starknet_version": "0.14.0",
    "l2_gas_consumed": 606837191,
    "next_l2_gas_price": "0xb2d05e0",
}

# Constant signature response content for local blocks
_SIGNATURE_CONST = {
    "block_hash": "0x8470dbc0e524e713c926511c3b1b5c8512b083f925bf0bd247f0a46ed91a4a",
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
        # Use insertion index among current stored numbers to produce stable values pre-insert
        try:
            nums = _storage.get_sorted_block_numbers()
            idx = bisect_left(nums, block_number)
        except Exception:
            idx = 0
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
        # Use insertion index among current stored numbers to produce stable values pre-insert
        try:
            nums = _storage.get_sorted_block_numbers()
            idx = bisect_left(nums, block_number)
        except Exception:
            idx = 0
    base_hex = CUSTOM_FIELDS.get("state_root")
    try:
        base = int(base_hex, 16) if base_hex else 0
    except Exception:
        base = 0
    old_root_int = base + idx
    new_root_int = base + idx + 1
    return _format_0x_hex(new_root_int), _format_0x_hex(old_root_int)


# Best-effort fetch of canonical block timestamp from upstream feeder.
def _fetch_block_timestamp_from_feeder(block_number: int) -> Optional[int]:
    try:
        url = f"{_UPSTREAM_FEEDER_BASE_URL}/feeder_gateway/get_block"
        resp = _session.get(
            url, params={"blockNumber": block_number}, headers=_FEEDER_HEADERS, timeout=20
        )
        if resp.status_code != 200:
            return None
        obj = resp.json()
        ts = obj.get("timestamp")
        return ts
    except Exception:
        return None


def _fetch_block_gas_prices_from_feeder(block_number: int) -> Optional[Dict[str, Any]]:
    try:
        url = f"{_UPSTREAM_FEEDER_BASE_URL}/feeder_gateway/get_block"
        resp = _session.get(
            url, params={"blockNumber": block_number}, headers=_FEEDER_HEADERS, timeout=20
        )
        if resp.status_code != 200:
            return None
        obj = resp.json()
        out: Dict[str, Any] = {}
        for key in ("l1_gas_price", "l1_data_gas_price", "l2_gas_price"):
            val = obj.get(key)
            if isinstance(val, dict):
                out[key] = val
        return out or None
    except Exception:
        return None


def _fetch_block_number_by_tx_hash_from_feeder(tx_hash: str) -> Optional[int]:
    try:
        url = f"{_UPSTREAM_FEEDER_BASE_URL}/feeder_gateway/get_transaction"
        resp = _session.get(
            url,
            params={"transactionHash": tx_hash},
            headers=_FEEDER_HEADERS,
            timeout=20,
        )
        if resp.status_code != 200:
            return None
        obj = resp.json()
        bn = obj.get("block_number")
        if isinstance(bn, int):
            return bn
        if isinstance(bn, str):
            try:
                return int(bn, 0) if bn.startswith("0x") else int(bn)
            except Exception:
                return None
        return None
    except Exception:
        return None


# Ensure storage directory exists
pathlib.Path(_BLOCKS_DIR).mkdir(parents=True, exist_ok=True)


def _proxy_upstream(upstream: str, args: Dict[str, Any]) -> Response:
    resp = _session.get(upstream, params=args, headers=_FEEDER_HEADERS, timeout=20)
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
        tx_type = tx.get("type")
        tx_obj = {
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
            "type": tx_type,
        }
        # Only include class_hash when it is present and non-null (e.g., DEPLOY_ACCOUNT)
        ch = tx.get("class_hash")
        if ch is not None:
            tx_obj["class_hash"] = ch
        # Specialize DEPLOY_ACCOUNT to FeederGateway shape
        if tx_type == "DEPLOY_ACCOUNT":
            # Remove generic calldata/account_deployment_data
            tx_obj.pop("calldata", None)
            tx_obj.pop("account_deployment_data", None)
            # Map fields directly from the blob shape
            tx_obj["contract_address_salt"] = tx.get("contract_address_salt")
            tx_obj["constructor_calldata"] = tx.get("constructor_calldata", [])
        out_txs.append(tx_obj)

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
    # if l1_gas_price is not None:
    #     l1_out = dict(l1_gas_price)
    #     l1_out["price_in_wei"] = _hex_div(l1_out.get("price_in_wei"), 4)
    #     l1_out["price_in_fri"] = _hex_div(l1_out.get("price_in_fri"), 4)
    #     out["l1_gas_price"] = l1_out
    if l1_gas_price is not None:
        out["l1_gas_price"] = l1_gas_price
    if l1_data_gas_price is not None:
        out["l1_data_gas_price"] = l1_data_gas_price
    if l2_gas_price is not None:
        out["l2_gas_price"] = l2_gas_price

    # Merge custom fields last so users can override/add fields deliberately
    if isinstance(CUSTOM_FIELDS, dict) and CUSTOM_FIELDS:
        out.update(CUSTOM_FIELDS)

    return out


# Build a state update document from the incoming blob's state_diff
def _transform_state_update_from_blob(blob: dict, block_number: int):
    sd = blob.get("state_diff", {}) or {}
    nonces_src = sd.get("nonces", {}) or {}
    storage_updates_src = sd.get("storage_updates", {}) or {}

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

    # Build deployed_contracts from available data:
    # 1) Prefer explicit mapping in state_diff.address_to_class_hash when present
    deployed_contracts_map = {}
    try:
        atch = blob.get("state_diff", {}).get("address_to_class_hash", {}) or {}
        if isinstance(atch, dict):
            for addr, chash in atch.items():
                if isinstance(addr, str) and isinstance(chash, str):
                    deployed_contracts_map[addr] = chash
    except Exception:
        pass
    # 2) Also derive from DEPLOY_ACCOUNT transactions if present in the blob
    try:
        tx_entries = blob.get("transactions", []) or []
        # Blob may be raw (each entry has "tx") or transformed (flat tx dicts)
        for entry in tx_entries:
            tx = entry.get("tx") if isinstance(entry, dict) and "tx" in entry else entry
            if not isinstance(tx, dict):
                continue
            if tx.get("type") == "DEPLOY_ACCOUNT":
                addr = tx.get("sender_address")
                chash = tx.get("class_hash")
                if isinstance(addr, str) and isinstance(chash, str):
                    deployed_contracts_map[addr] = chash
    except Exception:
        pass

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


def _get_blob_tx_hashes(blob: dict):
    hashes = [tx["tx"]["hash_value"] for tx in blob.get("transactions", [])]
    return hashes


def _append_lines(path: str, lines: Any) -> None:
    """Append iterable of text lines to a file, one per line."""
    p = pathlib.Path(path)
    p.parent.mkdir(parents=True, exist_ok=True)
    with p.open("a", encoding="utf-8") as f:
        for line in lines:
            # Ensure each entry is a single line
            text = str(line).rstrip("\n")
            f.write(text + "\n")


def _extract_revert_errors(blob: dict):
    """Return list of revert_error strings from top-level 'execution_infos' entries."""
    infos = blob.get("execution_infos")
    if not isinstance(infos, list):
        return []
    out = []
    for item in infos:
        if not isinstance(item, dict):
            continue
        err = item.get("revert_error")
        if isinstance(err, str) and err:
            out.append(err)
    return out


def _extract_revert_error_mappings(blob: dict):
    """
    Return list of objects mapping {hash_value: revert_error}, pairing entries by index:
    - execution_infos[i].revert_error
    - transactions[i].tx.hash_value
    """
    infos = blob.get("execution_infos")
    tx_entries = blob.get("transactions")
    if not isinstance(infos, list) or not isinstance(tx_entries, list):
        return []
    out = []
    for idx, item in enumerate(infos):
        if not isinstance(item, dict):
            continue
        err = item.get("revert_error")
        if not (isinstance(err, str) and err):
            continue
        # Fetch corresponding transaction hash_value at the same index
        tx_entry = tx_entries[idx] if idx < len(tx_entries) else None
        tx = tx_entry.get("tx") if isinstance(tx_entry, dict) else None
        hash_value = tx.get("hash_value") if isinstance(tx, dict) else None
        if isinstance(hash_value, str) and hash_value:
            out.append({hash_value: err})
    return out


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
    # Override top-level timestamp with the canonical one from the upstream feeder (best-effort).
    bn_for_upstream = bn
    try:
        tx_hashes_for_bn = _get_blob_tx_hashes(blob)
        if isinstance(tx_hashes_for_bn, list) and tx_hashes_for_bn:
            first_tx_hash = tx_hashes_for_bn[0]
            derived_bn = _fetch_block_number_by_tx_hash_from_feeder(first_tx_hash)
            if isinstance(derived_bn, int):
                bn_for_upstream = derived_bn
    except Exception:
        pass
    upstream_ts = _fetch_block_timestamp_from_feeder(bn_for_upstream)
    if upstream_ts is not None:
        to_store["timestamp"] = upstream_ts
    # Override gas prices with canonical values from the upstream feeder (best-effort).
    upstream_gas = _fetch_block_gas_prices_from_feeder(bn_for_upstream)
    if upstream_gas is not None:
        if "l1_gas_price" in upstream_gas:
            to_store["l1_gas_price"] = upstream_gas["l1_gas_price"]
        if "l1_data_gas_price" in upstream_gas:
            to_store["l1_data_gas_price"] = upstream_gas["l1_data_gas_price"]
        if "l2_gas_price" in upstream_gas:
            to_store["l2_gas_price"] = upstream_gas["l2_gas_price"]
    su = _transform_state_update_from_blob(blob, bn)
    block_path, _ = _storage.write_block_and_state_update(bn, to_store, su)
    # Also persist the full incoming blob for reference
    blob_path = _storage.write_blob(bn, blob)
    print(f"[FLASK] wrote blob file: {blob_path}")
    target = block_path

    print(f"[FLASK] block {bn} tx hashes: {' '.join(_get_blob_tx_hashes(blob))}")

    # Persist tx hashes to file (one per line) and any revert errors as JSONL
    try:
        hashes = _get_blob_tx_hashes(blob)
        if hashes:
            with _lock:
                _append_lines(_HASHES_FILE, hashes)
    except Exception:
        pass
    try:
        mappings = _extract_revert_error_mappings(blob)
        if mappings:
            # Serialize each mapping as JSON for JSONL
            lines = [json.dumps(m, ensure_ascii=False) for m in mappings]
            with _lock:
                _append_lines(_REVERT_ERRORS_FILE, lines)
    except Exception:
        pass

    return (
        jsonify(
            {
                "ok": True,
                "stored": True,
                "block_number": bn,
                "path": str(target) if target else "",
                "blob_path": str(blob_path),
            }
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
    # Support headerOnly=true to return only minimal header info
    header_only_raw = args.get("headerOnly")
    header_only = False
    if isinstance(header_only_raw, str):
        header_only = header_only_raw.lower() == "true"
    bn_parsed = _parse_block_number(bn_raw)

    if bn_parsed == "latest":
        highest = _storage.get_highest_block_number()
        if highest is None:
            return ("", 404)
        obj = _storage.read_block(highest)
        if obj is None:
            return ("", 404)
        if header_only:
            # Prefer stored hash if present, otherwise compute deterministic one
            stored_bh = obj.get("block_hash")
            if not isinstance(stored_bh, str):
                stored_bh, _ = _compute_hashes_for_block(highest)
            return _json_response({"block_hash": stored_bh, "block_number": highest}, 200)
        else:
            bh, ph = _compute_hashes_for_block(highest)
            obj["block_hash"] = bh
            obj["parent_block_hash"] = ph
            return _json_response(obj, 200)

    requested = int(bn_parsed)
    obj = _storage.read_block(requested)
    if obj is not None:
        if header_only:
            stored_bh = obj.get("block_hash")
            if not isinstance(stored_bh, str):
                stored_bh, _ = _compute_hashes_for_block(requested)
            return _json_response({"block_hash": stored_bh, "block_number": requested}, 200)
        else:
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


@app.route("/feeder_gateway/get_class_by_hash", methods=["GET"])
def get_class_by_hash():
    args = request.args.to_dict(flat=True)
    bn_raw = args.get("blockNumber")
    if not bn_raw:
        return _json_response({"error": "blockNumber is required"}, 400)
    # For pending queries, always proxy to the real feeder gateway with the provided args
    if isinstance(bn_raw, str) and bn_raw.lower() == "pending":
        return _proxy_upstream(
            "https://feeder.alpha-mainnet.starknet.io/feeder_gateway/get_class_by_hash",
            args,
        )
    # For other blockNumber values, we do not have local handling; return 404
    return ("", 404)


def _purge_stored_blocks():
    n = _storage.purge()
    try:
        p = pathlib.Path(_HASHES_FILE)
        if p.exists():
            p.unlink()
    except Exception:
        pass
    try:
        p2 = pathlib.Path(_REVERT_ERRORS_FILE)
        if p2.exists():
            p2.unlink()
    except Exception:
        pass
    return n


if __name__ == "__main__":
    import sys

    if len(sys.argv) > 1 and sys.argv[1] == "purge-stored-blocks":
        n = _purge_stored_blocks()
        print(f"Purged {n} stored block files from {_BLOCKS_DIR}")
    else:
        port = int(os.environ.get("PORT", "8000"))
        app.run(host="0.0.0.0", port=port)
