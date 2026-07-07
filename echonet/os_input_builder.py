"""
Build the `OsCliInput` JSON for `committer-and-os-cli os run-os-stateless` from
a cende blob (`AerospikeBlob`). The Rust input types live in
`starknet_committer_and_os_cli::os_cli::commands` and `starknet_os::io::os_input`;
they use `serde(deny_unknown_fields)`, so every emitted key must match exactly.

The blob is produced by apollo_consensus_orchestrator's cende `AerospikeBlob`
(`central_objects.rs`), so its field shapes are fixed and known: gas prices and
felts are 0x-hex strings, data-availability modes are u32, block numbers and
sizes are integers, and every documented field is always present. We index into
the blob directly rather than guarding against shapes it never takes.
"""

from __future__ import annotations

import base64
import io
import json
from typing import Any, Dict, List, Optional

import zstandard

from echonet.echonet_types import JsonObject


class OsInputBuildError(RuntimeError):
    """Raised when the cende blob lacks a field required to assemble OsHints."""


def decompress_state_commitment_infos(compressed: str) -> JsonObject:
    """
    Reverse of the committer's `base64(zstd(serde_json(StateCommitmentInfos)))`
    pipeline. The streaming decompressor is required: the Rust frame omits the
    content size, which the one-shot `zstandard.decompress()` rejects.
    """
    try:
        raw = base64.b64decode(compressed)
        decompressed = zstandard.ZstdDecompressor().stream_reader(io.BytesIO(raw)).read()
        return json.loads(decompressed)
    except (ValueError, zstandard.ZstdError) as exc:
        raise OsInputBuildError(
            f"failed to decode compressed state_commitment_infos: {exc}"
        ) from exc


def build_os_cli_input(
    blob: JsonObject,
    *,
    state_commitment_infos: JsonObject,
    block_number: int,
    prev_block_hash: str,
    new_block_hash: str,
    block_hash_commitments_payload: JsonObject,
    chain_id: str,
    strk_fee_token_address: str,
    layout: str,
    cairo_pie_zip_path: str,
    raw_os_output_path: str,
) -> JsonObject:
    """
    Assemble the `OsCliInput` JSON for `committer-and-os-cli os run-os-stateless`.

    `state_commitment_infos` is pre-picked by the caller (block N's entry lives
    in block N+1's blob — see `pick_state_commitment_infos`), and
    `block_hash_commitments_payload` is the block-hash CLI's response.
    """
    os_block_input = _build_os_block_input(
        blob=blob,
        state_commitment_infos=state_commitment_infos,
        block_number=block_number,
        prev_block_hash=prev_block_hash,
        new_block_hash=new_block_hash,
        block_hash_commitments_payload=block_hash_commitments_payload,
    )
    os_input = {
        "os_block_inputs": [os_block_input],
        # Both class maps are filled in by the caller from `resolve_classes_for_os`.
        "deprecated_compiled_classes": {},
        "compiled_classes": {},
    }
    os_hints_config = {
        "debug_mode": False,
        "full_output": False,
        "use_kzg_da": blob["state_diff"]["block_info"]["use_kzg_da"],
        "chain_info": {
            "chain_id": _chain_id_to_hex(chain_id),
            "strk_fee_token_address": strk_fee_token_address,
        },
        "public_keys": None,
        "rng_seed_salt": None,
    }
    return {
        "layout": layout,
        "os_hints": {"os_input": os_input, "os_hints_config": os_hints_config},
        "cairo_pie_zip_path": cairo_pie_zip_path,
        "raw_os_output_path": raw_os_output_path,
    }


def _build_os_block_input(
    *,
    blob: JsonObject,
    state_commitment_infos: JsonObject,
    block_number: int,
    prev_block_hash: str,
    new_block_hash: str,
    block_hash_commitments_payload: JsonObject,
) -> JsonObject:
    return {
        "contract_state_commitment_info": state_commitment_infos["contracts_trie_commitment_info"],
        "address_to_storage_commitment_info": state_commitment_infos[
            "storage_tries_commitment_infos"
        ],
        "contract_class_commitment_info": state_commitment_infos["classes_trie_commitment_info"],
        "transactions": [_central_tx_to_executable(entry["tx"]) for entry in blob["transactions"]],
        "tx_execution_infos": blob["execution_infos"],
        "declared_class_hash_to_component_hashes": _declared_class_hash_to_component_hashes(blob),
        "block_info": _central_block_info_to_block_info(blob["state_diff"]["block_info"]),
        "block_hash_commitments": _block_hash_commitments_from_payload(
            block_hash_commitments_payload
        ),
        "prev_block_hash": prev_block_hash,
        "new_block_hash": new_block_hash,
        "old_block_number_and_hash": _old_block_number_and_hash(blob, block_number),
        "class_hashes_to_migrate": _class_hashes_to_migrate(blob),
        "initial_reads": blob["initial_reads"],
    }


def pick_state_commitment_infos(blob: JsonObject, block_number: int) -> JsonObject:
    """
    Pull `block_number`'s entry from the blob's `recent_state_commitment_infos`.

    The vector only holds *prior* blocks (the current block's commit races the
    blob preparation), so block N's entry must come from block N+1's blob.
    """
    entries = blob["recent_state_commitment_infos"]
    for entry in entries:
        if entry["block_number"] == block_number:
            return decompress_state_commitment_infos(entry["state_commitment_infos"])
    available = sorted(entry["block_number"] for entry in entries)
    raise OsInputBuildError(
        f"no recent_state_commitment_infos entry for block_number {block_number}; "
        f"vector contains {len(available)} entries for block_numbers={available}"
    )


def _central_block_info_to_block_info(central_block_info: JsonObject) -> JsonObject:
    """
    Map `CentralBlockInfo` → `BlockInfo`: central's per-resource
    `{price_in_wei, price_in_fri}` objects fold into `gas_prices`'
    `{eth_gas_prices, strk_gas_prices}`.
    """
    l1 = central_block_info["l1_gas_price"]
    l1_data = central_block_info["l1_data_gas_price"]
    l2 = central_block_info["l2_gas_price"]
    return {
        "block_number": central_block_info["block_number"],
        "block_timestamp": central_block_info["block_timestamp"],
        "starknet_version": central_block_info["starknet_version"],
        "sequencer_address": central_block_info["sequencer_address"],
        "gas_prices": {
            "eth_gas_prices": {
                "l1_gas_price": _gas_price_to_hex16(l1["price_in_wei"]),
                "l1_data_gas_price": _gas_price_to_hex16(l1_data["price_in_wei"]),
                "l2_gas_price": _gas_price_to_hex16(l2["price_in_wei"]),
            },
            "strk_gas_prices": {
                "l1_gas_price": _gas_price_to_hex16(l1["price_in_fri"]),
                "l1_data_gas_price": _gas_price_to_hex16(l1_data["price_in_fri"]),
                "l2_gas_price": _gas_price_to_hex16(l2["price_in_fri"]),
            },
        },
        "use_kzg_da": central_block_info["use_kzg_da"],
    }


def _central_tx_to_executable(central_tx: JsonObject) -> JsonObject:
    """
    Convert a `CentralTransaction` to the executable `Transaction` JSON. The
    executable enums are externally tagged (`{"Account":{"Invoke":{"V3":{...}}}}`,
    `{"L1Handler":{...}}`); the central types are tagged by `type` and V3-only.
    """
    tx_type = central_tx["type"]
    if tx_type == "INVOKE_FUNCTION":
        return {"Account": {"Invoke": _invoke_v3_to_executable(central_tx)}}
    if tx_type == "DEPLOY_ACCOUNT":
        return {"Account": {"DeployAccount": _deploy_account_v3_to_executable(central_tx)}}
    if tx_type == "DECLARE":
        return {"Account": {"Declare": _declare_v3_to_executable(central_tx)}}
    if tx_type == "L1_HANDLER":
        return {"L1Handler": _l1_handler_to_executable(central_tx)}
    raise OsInputBuildError(f"unknown transaction type {tx_type!r} in blob")


def _common_account_v3_fields(central_tx: JsonObject) -> JsonObject:
    """The V3 fields shared by the invoke, deploy_account, and declare wrappers."""
    return {
        "resource_bounds": _resource_bounds_central_to_executable(central_tx["resource_bounds"]),
        # `Tip` serializes as hex on both sides — pass through.
        "tip": central_tx["tip"],
        "signature": central_tx["signature"],
        "nonce": central_tx["nonce"],
        "nonce_data_availability_mode": _da_mode_to_str(central_tx["nonce_data_availability_mode"]),
        "fee_data_availability_mode": _da_mode_to_str(central_tx["fee_data_availability_mode"]),
        "paymaster_data": central_tx["paymaster_data"],
    }


def _invoke_v3_to_executable(central_tx: JsonObject) -> JsonObject:
    inner = {
        **_common_account_v3_fields(central_tx),
        "sender_address": central_tx["sender_address"],
        "calldata": central_tx["calldata"],
        "account_deployment_data": central_tx["account_deployment_data"],
    }
    # `proof_facts` participates in the V3 invoke hash only when non-empty, and
    # central omits it when empty — pass it through only when present.
    proof_facts = central_tx.get("proof_facts")
    if proof_facts:
        inner["proof_facts"] = proof_facts
    return {"tx": {"V3": inner}, "tx_hash": central_tx["hash_value"]}


def _deploy_account_v3_to_executable(central_tx: JsonObject) -> JsonObject:
    """Central's `sender_address` becomes the executable wrapper's `contract_address`."""
    inner = {
        **_common_account_v3_fields(central_tx),
        "class_hash": central_tx["class_hash"],
        "contract_address_salt": central_tx["contract_address_salt"],
        "constructor_calldata": central_tx["constructor_calldata"],
    }
    return {
        "tx": {"V3": inner},
        "tx_hash": central_tx["hash_value"],
        "contract_address": central_tx["sender_address"],
    }


def _declare_v3_to_executable(central_tx: JsonObject) -> JsonObject:
    """
    Central `DECLARE` V3 → executable: the wrapper adds `class_info`, assembled
    from the size/version metadata the central tx already carries.
    """
    inner = {
        **_common_account_v3_fields(central_tx),
        "class_hash": central_tx["class_hash"],
        "compiled_class_hash": central_tx["compiled_class_hash"],
        "sender_address": central_tx["sender_address"],
        "account_deployment_data": central_tx["account_deployment_data"],
    }
    sierra_version = _sierra_version_to_executable(central_tx["sierra_version"])
    blank_casm = {
        "prime": "0x0",
        "compiler_version": "",
        "bytecode": [],
        "hints": [],
        "entry_points_by_type": {
            "EXTERNAL": [],
            "L1_HANDLER": [],
            "CONSTRUCTOR": [],
        },
    }
    class_info = {
        "contract_class": {"V1": [blank_casm, sierra_version]},
        "sierra_program_length": central_tx["sierra_program_size"],
        "abi_length": central_tx["abi_size"],
        "sierra_version": sierra_version,
    }
    return {
        "tx": {"V3": inner},
        "tx_hash": central_tx["hash_value"],
        "class_info": class_info,
    }


def _l1_handler_to_executable(central_tx: JsonObject) -> JsonObject:
    """Central `L1_HANDLER` → executable; the inner `version` field is always 0."""
    inner = {
        "version": "0x0",
        "nonce": central_tx["nonce"],
        "contract_address": central_tx["contract_address"],
        "entry_point_selector": central_tx["entry_point_selector"],
        "calldata": central_tx["calldata"],
    }
    return {
        "tx": inner,
        "tx_hash": central_tx["hash_value"],
        "paid_fee_on_l1": central_tx["paid_fee_on_l1"],
    }


def _sierra_version_to_executable(sierra_version_tuple: List[str]) -> str:
    """Central's hex 3-tuple (major, minor, patch) → the executable's semver string."""
    major, minor, patch = (int(part, 16) for part in sierra_version_tuple)
    return f"{major}.{minor}.{patch}"


def _resource_bounds_central_to_executable(central_rb: JsonObject) -> JsonObject:
    """Central's `L1_DATA_GAS` key is `L1_DATA` in the executable `Resource` enum."""
    return {
        "L1_GAS": central_rb["L1_GAS"],
        "L2_GAS": central_rb["L2_GAS"],
        "L1_DATA": central_rb["L1_DATA_GAS"],
    }


def _da_mode_to_str(mode: int) -> str:
    """Central encodes DataAvailabilityMode as u32 (0/1); the executable expects 'L1'/'L2'."""
    if mode == 0:
        return "L1"
    if mode == 1:
        return "L2"
    raise OsInputBuildError(f"unexpected data availability mode: {mode!r}")


def _block_hash_commitments_from_payload(payload: JsonObject) -> JsonObject:
    """Reshape the block-hash CLI's response into `BlockHeaderCommitments`."""
    return {
        "transaction_commitment": payload["transaction_commitment"],
        "event_commitment": payload["event_commitment"],
        "receipt_commitment": payload["receipt_commitment"],
        "state_diff_commitment": payload["state_diff_commitment"],
        "concatenated_counts": payload["concatenated_counts"],
    }


def _old_block_number_and_hash(blob: JsonObject, current_block_number: int) -> Optional[List[Any]]:
    """
    The OS wants the `(current - STORED_BLOCK_HASH_BUFFER)` block's number+hash;
    that's the oldest entry of the blob's `recent_block_hashes` (or None when the
    chain is younger than the buffer and the vector is empty).
    """
    recent = blob["recent_block_hashes"]
    if not recent:
        return None
    oldest = min(recent, key=lambda entry: entry["block_number"])
    return [oldest["block_number"], oldest["block_hash"]]


def _class_hashes_to_migrate(blob: JsonObject) -> List[List[str]]:
    """
    The blob carries `(casm_v2, casm_v1)` pairs whose class-hash keys were
    dropped by blockifier's `finalize_block` (`.into_values()`), while the OS
    expects `(class_hash, casm_v2)`. Recover each class hash by reverse lookup
    of the v1 hash in `initial_reads.compiled_class_hashes` — a migrating
    class's state entry still holds v1 (that is `should_migrate`'s condition).
    """
    pairs = blob["compiled_class_hashes_for_migration"]
    if not pairs:
        return []
    compiled_class_hashes = blob["initial_reads"]["compiled_class_hashes"]
    class_hashes_by_casm_v1: Dict[str, List[str]] = {}
    for class_hash, casm_hash in compiled_class_hashes.items():
        class_hashes_by_casm_v1.setdefault(casm_hash, []).append(class_hash)
    migration_pairs: List[List[str]] = []
    for casm_v2, casm_v1 in pairs:
        candidate_class_hashes = class_hashes_by_casm_v1.get(casm_v1, [])
        if len(candidate_class_hashes) != 1:
            raise OsInputBuildError(
                f"cannot recover the class hash for migration pair (v2={casm_v2}, "
                f"v1={casm_v1}): {len(candidate_class_hashes)} entries in "
                "initial_reads.compiled_class_hashes hold that v1 casm hash"
            )
        migration_pairs.append([candidate_class_hashes[0], casm_v2])
    return migration_pairs


def _declared_class_hash_to_component_hashes(blob: JsonObject) -> JsonObject:
    """
    Derive `{class_hash: ContractClassComponentHashes}` for each declared Sierra
    class via cairo-lang's `py_hash_class_components`.

    The cairo-lang import is kept local on purpose: this module is imported by the
    main flask process (for `pick_state_commitment_infos` etc.), and cairo-lang is
    a heavy dependency only the OS-runner worker actually needs — hoisting it would
    load it into every process, including the memory-sensitive flask one.
    """
    entries = blob["contract_classes"]
    if not entries:
        return {}

    from starkware.starknet.core.os.contract_class.class_hash_utils import (
        py_hash_class_components,
    )
    from starkware.starknet.services.api.contract_class.contract_class import ContractClass

    out: Dict[str, JsonObject] = {}
    for entry in entries:
        class_hash, wrapper = entry[0], entry[1]
        contract_class = ContractClass.load(wrapper["contract_class"])
        component_hashes = py_hash_class_components(contract_class)
        out[class_hash] = {
            "contract_class_version": _felt_hex(component_hashes.contract_class_version),
            "external_functions_hash": _felt_hex(component_hashes.external_functions_hash),
            "l1_handlers_hash": _felt_hex(component_hashes.l1_handlers_hash),
            "constructors_hash": _felt_hex(component_hashes.constructors_hash),
            "abi_hash": _felt_hex(component_hashes.abi_hash),
            "sierra_program_hash": _felt_hex(component_hashes.sierra_program_hash),
        }
    return out


def _felt_hex(value: int) -> str:
    """`Felt` serializes as `0x{value:x}` (no zero-padding)."""
    return f"0x{value:x}"


def _chain_id_to_hex(chain_id: str) -> str:
    """`ChainId` serializes as the UTF-8 hex of the chain string ('SN_MAIN' → '0x534e...')."""
    return "0x" + chain_id.encode("utf-8").hex()


def _gas_price_to_hex16(value: str) -> str:
    """`GasPrice` serializes as `PrefixedBytesAsHex<16>` (0x + 32 hex digits); normalize width."""
    return "0x" + format(int(value, 16), "032x")
