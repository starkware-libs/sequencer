"""
Build the JSON input (`OsCliInput`) for the Starknet OS runner from a cende blob.

The Rust types are in `crates/starknet_committer_and_os_cli/src/os_cli/commands.rs`
(`OsCliInput`) and `crates/starknet_os/src/io/os_input.rs` (`OsHints`,
`StarknetOsInput`, `OsBlockInput`, `OsHintsConfig`). All structs use
`serde(deny_unknown_fields)`, so every emitted key must match exactly.

The cende blob (`AerospikeBlob` in
`crates/apollo_consensus_orchestrator/src/cende/mod.rs`) is the single source of
truth for OS inputs.
"""

from __future__ import annotations

from typing import Any, Dict, List, Mapping, Optional

from echonet.echonet_types import JsonObject

OS_DEFAULT_LAYOUT = "all_cairo"


class OsInputBuildError(RuntimeError):
    """Raised when the cende blob lacks a field required to assemble OsHints."""


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
    Assemble the `OsCliInput` JSON consumed by `committer-and-os-cli OS run-os-stateless`.

    `state_commitment_infos` is pre-picked by the caller. The cende blob carries
    `recent_state_commitment_infos` as a sliding window of the 10 *prior* blocks
    (for SNOS proof_facts), so to run the OS on block N the caller must pull N's
    entry from block N+1's blob — not block N's own blob. echo_center handles
    that one-block lag externally and passes the right entry here.

    `block_hash_commitments_payload` is the JSON returned by the existing
    `block-hash-commitments` CLI invocation in echo_center.
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
        "deprecated_compiled_classes": {},
        "compiled_classes": _compiled_classes_to_map(blob),
    }
    use_kzg_da = bool(blob["state_diff"]["block_info"].get("use_kzg_da", False))
    os_hints_config = {
        "debug_mode": False,
        "full_output": False,
        "use_kzg_da": use_kzg_da,
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
    sierra_by_class_hash = _sierra_by_class_hash(blob)
    return {
        "contract_state_commitment_info": state_commitment_infos["contracts_trie_commitment_info"],
        "address_to_storage_commitment_info": state_commitment_infos[
            "storage_tries_commitment_infos"
        ],
        "contract_class_commitment_info": state_commitment_infos["classes_trie_commitment_info"],
        "transactions": [
            _central_tx_to_executable(entry["tx"], sierra_by_class_hash)
            for entry in blob["transactions"]
        ],
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
        "initial_reads": _initial_reads_from_blob(blob),
    }


def pick_state_commitment_infos(blob: JsonObject, block_number: int) -> JsonObject:
    """
    Pull the `StateCommitmentInfos` matching `block_number` out of the blob's
    `recent_state_commitment_infos: Vec<StateCommitmentInfosAndNumber>` (gated on
    `os_input` in the cende crate; required for OS runs).

    Note: the orchestrator's `collect_recent_state_commitment_infos` populates the
    vector with the 10 *prior* committed blocks — the current block is not in its own
    blob because the batcher's commit task races against the blob preparation.
    So to get block N's `StateCommitmentInfos`, call this with **block N+1's blob**.
    """
    entries = blob.get("recent_state_commitment_infos")
    if entries is None:
        raise OsInputBuildError(
            "blob missing 'recent_state_commitment_infos'; sequencer must be "
            "built with --features os_input"
        )
    for entry in entries:
        if int(entry["block_number"]) == int(block_number):
            return entry["state_commitment_infos"]
    available = sorted(int(entry["block_number"]) for entry in entries)
    raise OsInputBuildError(
        f"no recent_state_commitment_infos entry for block_number {block_number}; "
        f"vector contains {len(available)} entries for block_numbers={available}"
    )


def _central_block_info_to_block_info(central_block_info: JsonObject) -> JsonObject:
    """
    Map `CentralBlockInfo` → `starknet_api::block::BlockInfo`.

    Central layout (separate `l1_gas_price` / `l1_data_gas_price` / `l2_gas_price`
    objects, each `{price_in_wei, price_in_fri}`) is folded into BlockInfo's
    `gas_prices: {eth_gas_prices, strk_gas_prices}` — eth ← price_in_wei,
    strk ← price_in_fri. `GasPrice` serializes as 16-byte hex.
    """
    l1 = central_block_info["l1_gas_price"]
    l1_data = central_block_info["l1_data_gas_price"]
    l2 = central_block_info["l2_gas_price"]
    return {
        "block_number": int(central_block_info["block_number"]),
        "block_timestamp": int(central_block_info["block_timestamp"]),
        "starknet_version": str(central_block_info["starknet_version"]),
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
        "use_kzg_da": bool(central_block_info.get("use_kzg_da", False)),
    }


def _central_tx_to_executable(
    central_tx: JsonObject, sierra_by_class_hash: Mapping[str, JsonObject]
) -> JsonObject:
    """
    Convert a `CentralTransaction` JSON object to the executable
    `starknet_api::executable_transaction::Transaction` JSON.

    The executable enum is externally tagged: `{"Account":{"Invoke":{...}}}` etc.
    The inner versioned tx is also externally tagged (`{"V3": {...}}`).

    All current mainnet account-txs are V3 (V0/V1/V2 are deprecated). L1_HANDLER
    is unversioned (its `version` field is 0).

    `sierra_by_class_hash` maps `class_hash → SierraContractClass` for any
    classes declared in this block (from `blob["contract_classes"]`); only
    DECLARE uses it.
    """
    tx_type = central_tx["type"]
    if tx_type == "INVOKE_FUNCTION":
        return {"Account": {"Invoke": _invoke_v3_to_executable(central_tx)}}
    if tx_type == "DEPLOY_ACCOUNT":
        return {"Account": {"DeployAccount": _deploy_account_v3_to_executable(central_tx)}}
    if tx_type == "DECLARE":
        return {"Account": {"Declare": _declare_v3_to_executable(central_tx, sierra_by_class_hash)}}
    if tx_type == "L1_HANDLER":
        return {"L1Handler": _l1_handler_to_executable(central_tx)}
    raise OsInputBuildError(f"unknown transaction type {tx_type!r} in blob")


def _invoke_v3_to_executable(central_tx: JsonObject) -> JsonObject:
    inner = {
        "resource_bounds": _resource_bounds_central_to_executable(central_tx["resource_bounds"]),
        # `Tip(u64)` uses `PrefixedBytesAsHex<8>` — serializes as a `0x`-prefixed
        # 8-byte hex string. The central blob already carries it in that exact form,
        # so pass through (do NOT convert to int).
        "tip": central_tx["tip"],
        "signature": list(central_tx.get("signature", [])),
        "nonce": central_tx["nonce"],
        "sender_address": central_tx["sender_address"],
        "calldata": list(central_tx.get("calldata", [])),
        "nonce_data_availability_mode": _da_mode_to_str(central_tx["nonce_data_availability_mode"]),
        "fee_data_availability_mode": _da_mode_to_str(central_tx["fee_data_availability_mode"]),
        "paymaster_data": list(central_tx.get("paymaster_data", [])),
        "account_deployment_data": list(central_tx.get("account_deployment_data", [])),
    }
    # SNIP-19 / SNOS proof-fact txs: non-empty `proof_facts` participates in
    # the V3 invoke hash (see `transaction_hash.rs::get_invoke_transaction_v3_hash`
    # — appended to the hash chain only when non-empty). The central blob
    # serializes the field with `skip_serializing_if = "ProofFacts::is_empty"`,
    # so it's absent for plain invokes; pass through when present.
    proof_facts = central_tx.get("proof_facts")
    if proof_facts:
        inner["proof_facts"] = list(proof_facts)
    return {"tx": {"V3": inner}, "tx_hash": central_tx["hash_value"]}


def _deploy_account_v3_to_executable(central_tx: JsonObject) -> JsonObject:
    """
    Central `DEPLOY_ACCOUNT` V3 → executable `DeployAccountTransaction`. Central
    carries `sender_address` (the deployed contract's address); the executable
    wrapper exposes it as `contract_address` (sibling of `tx` / `tx_hash`).
    See `crates/starknet_api/src/executable_transaction.rs::DeployAccountTransaction`.
    """
    inner = {
        "resource_bounds": _resource_bounds_central_to_executable(central_tx["resource_bounds"]),
        "tip": central_tx["tip"],
        "signature": list(central_tx.get("signature", [])),
        "nonce": central_tx["nonce"],
        "class_hash": central_tx["class_hash"],
        "contract_address_salt": central_tx["contract_address_salt"],
        "constructor_calldata": list(central_tx.get("constructor_calldata", [])),
        "nonce_data_availability_mode": _da_mode_to_str(central_tx["nonce_data_availability_mode"]),
        "fee_data_availability_mode": _da_mode_to_str(central_tx["fee_data_availability_mode"]),
        "paymaster_data": list(central_tx.get("paymaster_data", [])),
    }
    return {
        "tx": {"V3": inner},
        "tx_hash": central_tx["hash_value"],
        "contract_address": central_tx["sender_address"],
    }


def _declare_v3_to_executable(
    central_tx: JsonObject, sierra_by_class_hash: Mapping[str, JsonObject]
) -> JsonObject:
    """
    Central `DECLARE` V3 → executable `DeclareTransaction`. The executable
    wrapper adds `class_info` (the Sierra/CASM bundle) alongside `tx`/`tx_hash`;
    the OS pulls bytecode/abi sizes + the Sierra version from there for fee
    accounting. We assemble it from `blob["contract_classes"]` (Sierra, keyed
    by `class_hash`) and the metadata the central tx already carries
    (`sierra_program_size`, `abi_size`, `sierra_version`).
    """
    class_hash = central_tx["class_hash"]
    sierra_class = sierra_by_class_hash.get(class_hash)
    if sierra_class is None:
        raise OsInputBuildError(
            f"DECLARE tx references class_hash {class_hash} but it is not in "
            f"blob['contract_classes']; available={list(sierra_by_class_hash)[:5]}..."
        )

    inner = {
        "resource_bounds": _resource_bounds_central_to_executable(central_tx["resource_bounds"]),
        "tip": central_tx["tip"],
        "signature": list(central_tx.get("signature", [])),
        "nonce": central_tx["nonce"],
        "class_hash": class_hash,
        "compiled_class_hash": central_tx["compiled_class_hash"],
        "sender_address": central_tx["sender_address"],
        "nonce_data_availability_mode": _da_mode_to_str(central_tx["nonce_data_availability_mode"]),
        "fee_data_availability_mode": _da_mode_to_str(central_tx["fee_data_availability_mode"]),
        "paymaster_data": list(central_tx.get("paymaster_data", [])),
        "account_deployment_data": list(central_tx.get("account_deployment_data", [])),
    }

    sierra_version_tuple = central_tx.get("sierra_version")
    if not isinstance(sierra_version_tuple, list) or len(sierra_version_tuple) != 3:
        raise OsInputBuildError(
            f"DECLARE tx missing/invalid sierra_version: {sierra_version_tuple!r}"
        )
    sierra_version = _sierra_version_to_executable(sierra_version_tuple)

    # The OS deserializes class_info.contract_class as
    # `ContractClass::V1((CasmContractClass, SierraVersion))` and asserts the
    # `CasmContractClass` is a blank sentinel (prime = 0) — see
    # `validate_single_input` in starknet_committer_and_os_cli. Passing the
    # actual Sierra body here fails deserialization with
    # `missing field 'offset'`, because Sierra entry points carry
    # `function_idx` while `CasmContractEntryPoint` requires `offset`.
    # Sierra/abi length + version are the only class_info fields the OS
    # actually uses (for fee accounting); the CASM slot is a mandatory-shape
    # placeholder.
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
        "sierra_program_length": int(central_tx["sierra_program_size"]),
        "abi_length": int(central_tx["abi_size"]),
        "sierra_version": sierra_version,
    }
    return {
        "tx": {"V3": inner},
        "tx_hash": central_tx["hash_value"],
        "class_info": class_info,
    }


def _l1_handler_to_executable(central_tx: JsonObject) -> JsonObject:
    """
    Central `L1_HANDLER` → executable `L1HandlerTransaction`. The inner
    `L1HandlerTransaction` struct (from `starknet_api::transaction`) has a
    `version` field that's always 0 (per `L1HandlerTransaction::VERSION`).
    """
    inner = {
        "version": "0x0",
        "nonce": central_tx["nonce"],
        "contract_address": central_tx["contract_address"],
        "entry_point_selector": central_tx["entry_point_selector"],
        "calldata": list(central_tx.get("calldata", [])),
    }
    return {
        "tx": inner,
        "tx_hash": central_tx["hash_value"],
        "paid_fee_on_l1": central_tx["paid_fee_on_l1"],
    }


def _sierra_version_to_executable(sierra_version_tuple: List[Any]) -> str:
    """
    Central serializes `SierraVersion` as a 3-tuple of `0x`-prefixed hex strings
    (major, minor, patch — see `into_string_tuple` in cende's central_objects).
    The executable `SierraVersion(semver::Version)` deserializes from a single
    semver string, not a 3-tuple, so we join them: `"2.11.4"`.
    """
    major, minor, patch = (int(str(v), 16) for v in sierra_version_tuple[:3])
    return f"{major}.{minor}.{patch}"


def _resource_bounds_central_to_executable(central_rb: JsonObject) -> JsonObject:
    """
    Central uses `L1_GAS / L2_GAS / L1_DATA_GAS`; the executable
    `ValidResourceBounds`'s serialized map uses `Resource` keys
    `L1_GAS / L2_GAS / L1_DATA` (see `crates/starknet_api/src/transaction/fields.rs`,
    `pub enum Resource` with `#[serde(rename = ...)]`).
    """
    return {
        "L1_GAS": central_rb["L1_GAS"],
        "L2_GAS": central_rb["L2_GAS"],
        "L1_DATA": central_rb["L1_DATA_GAS"],
    }


def _da_mode_to_str(value: Any) -> str:
    """Central tx encodes DataAvailabilityMode as u32 (0/1); executable expects 'L1'/'L2'."""
    if isinstance(value, str):
        if value in ("L1", "L2"):
            return value
        raise OsInputBuildError(f"unexpected DA mode string: {value!r}")
    as_int = int(value)
    if as_int == 0:
        return "L1"
    if as_int == 1:
        return "L2"
    raise OsInputBuildError(f"unexpected DA mode value: {value!r}")


def _compiled_classes_to_map(blob: JsonObject) -> JsonObject:
    """
    `blob["compiled_classes"]` is `Vec<(CompiledClassHash, {compiled_class: CasmContractClass})>`
    (each entry serialized as a `[hash, {"compiled_class": ...}]` JSON tuple).
    The OS expects `BTreeMap<CompiledClassHash, CasmContractClass>` — an object keyed
    by hex hash, value is the inner `CasmContractClass` (unwrap the `compiled_class` field).
    """
    out: Dict[str, JsonObject] = {}
    for entry in blob.get("compiled_classes", []):
        compiled_class_hash, wrapper = entry[0], entry[1]
        out[str(compiled_class_hash)] = wrapper["compiled_class"]
    return out


def _sierra_by_class_hash(blob: JsonObject) -> Dict[str, JsonObject]:
    """
    `blob["contract_classes"]` is `Vec<(ClassHash, {contract_class: SierraContractClass})>`
    — the Sierra classes newly declared in this block. Indexed by `class_hash` so
    DECLARE-tx conversion can assemble each tx's `ClassInfo`.
    """
    out: Dict[str, JsonObject] = {}
    for entry in blob.get("contract_classes", []) or []:
        class_hash, wrapper = entry[0], entry[1]
        out[str(class_hash)] = wrapper["contract_class"]
    return out


def _block_hash_commitments_from_payload(payload: JsonObject) -> JsonObject:
    """
    Reshape the block-hash CLI's `block-hash-commitments` response into
    `BlockHeaderCommitments`. The CLI returns flat felt strings for each
    commitment kind; concatenated_counts is a separate field.
    """
    return {
        "transaction_commitment": str(payload["transaction_commitment"]),
        "event_commitment": str(payload["event_commitment"]),
        "receipt_commitment": str(payload["receipt_commitment"]),
        "state_diff_commitment": str(payload["state_diff_commitment"]),
        "concatenated_counts": str(payload["concatenated_counts"]),
    }


def _old_block_number_and_hash(blob: JsonObject, current_block_number: int) -> Optional[List[Any]]:
    """
    The OS uses `Option<(BlockNumber, BlockHash)>` for the
    `(current - STORED_BLOCK_HASH_BUFFER)` block hash. The cende blob carries
    `recent_block_hashes: Vec<{block_hash, block_number}>` — pick the oldest entry
    (the buffer-back entry, by `N_BLOCK_HASHES_BACK_IN_BLOB = STORED_BLOCK_HASH_BUFFER`),
    or `None` if the list is empty.

    The serde shape of a Rust 2-tuple is a JSON array `[num, hash]`.
    """
    recent: List[JsonObject] = list(blob.get("recent_block_hashes", []))
    if not recent:
        return None
    oldest = min(recent, key=lambda e: int(e["block_number"]))
    return [int(oldest["block_number"]), str(oldest["block_hash"])]


def _class_hashes_to_migrate(blob: JsonObject) -> List[List[str]]:
    """
    `blob["compiled_class_hashes_for_migration"]` is `Vec<(CompiledClassHash, CompiledClassHash)>`
    (V2, V1 pair per the central type). The OS expects
    `Vec<(ClassHash, CompiledClassHash)>` — we forward the pairs as-is. Their JSON
    shape is `[[h1, h2], ...]`; confirm the central type when validating against a
    real blob (TODO: verify the pair order matches the OS's `(class_hash, compiled_class_hash)`).
    """
    pairs = blob.get("compiled_class_hashes_for_migration", []) or []
    return [[str(a), str(b)] for a, b in pairs]


def _declared_class_hash_to_component_hashes(blob: JsonObject) -> JsonObject:
    """
    Map `{class_hash: ContractClassComponentHashes}` derived from each Sierra class in
    `blob["contract_classes"]`. Uses cairo-lang's `py_hash_class_components` — the same
    Poseidon-over-class-structure derivation as the Rust
    `sierra_class.get_component_hashes()`. Cairo-lang is imported lazily and only when
    there is at least one declared class, so echonet's module load does not require it
    on blocks (or local dev runs) without declares.
    """
    entries = blob.get("contract_classes") or []
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
        out[str(class_hash)] = {
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


def _initial_reads_from_blob(blob: JsonObject) -> JsonObject:
    """
    Pass `blob["initial_reads"]` through as `StateMaps` JSON.

    The blob writer (`apollo_batcher::batcher::decision_reached` →
    `cended_state::get_os_initial_reads`) populates this with the OS's pre-state reads
    (back-filled with the trie leaves for accessed contracts and classes), then trims
    to `accessed_keys` and clears `declared_contracts`. The serializer is blockifier's
    `transaction_serde` — the same one `starknet_os` deserializes with — so the JSON
    shape matches the OS expectation exactly:
      - `storage` is nested `{addr: {key: value}}` (custom serializer).
      - `nonces`, `class_hashes`, `compiled_class_hashes` are flat maps.
      - `declared_contracts` is empty (OS `update_cache` asserts on this).
    """
    initial_reads = blob.get("initial_reads")
    if initial_reads is None:
        raise OsInputBuildError(
            "blob is missing `initial_reads`; sequencer image must be built with "
            "the `os_input` feature (added in PR #14593)"
        )
    return initial_reads


def _chain_id_to_hex(chain_id: str) -> str:
    """
    `ChainId` is serialized as a `0x`-prefixed UTF-8 hex of the chain string
    (e.g. 'SN_MAIN' → '0x534e5f4d41494e'). If the caller already provided a
    hex-form chain id, pass it through.
    """
    if chain_id.startswith("0x"):
        return chain_id
    return "0x" + chain_id.encode("utf-8").hex()


def _gas_price_to_hex16(value: Any) -> str:
    """
    `GasPrice` is `u128` serialized as a `0x`-prefixed 16-byte hex (32 hex chars,
    zero-padded). Central `NonzeroGasPrice` is already a hex string; we just
    normalize zero-padding/width.
    """
    if isinstance(value, int):
        as_int = value
    else:
        s = str(value)
        as_int = int(s, 16) if s.startswith("0x") else int(s)
    if as_int < 0:
        raise OsInputBuildError(f"negative gas price: {value!r}")
    return "0x" + format(as_int, "032x")
