# Design: Feeder Gateway as a Sequencer Component

**Status:** Draft / for review
**Author:** andrew.l
**Date:** 2026-06-02

## 1. Goal

Stand up a StarkNet **feeder gateway** as a native component in the Rust sequencer
(`starkware-libs/sequencer`) that re-serves chain data in the legacy feeder-gateway
wire format, as the **first step** toward eventually removing the feeder gateway from
the Python monorepo (`starkware-industries/starkware`,
`src/starkware/starknet/services/feeder_gateway/`).

This is explicitly **step 1 of a multi-project effort**, not the removal itself. The
scoping principle for this step is narrow and concrete:

> **Externalize what is already present in the sequencer's sync storage**, in the
> feeder-gateway wire format. Do not build new data producers, and do not remove or
> change anything in the existing system.

Constraints that follow from that principle:

1. **Wire-compatible.** Responses must be byte-for-byte compatible with the Python
   feeder gateway, because **external clients** (wallets, block explorers) — not just
   the sequencer's own `apollo_starknet_client` — must keep working unchanged. Full
   external parity is required.
2. **Runs alongside the Python FG.** The new component is additive. The Python feeder
   gateway stays in place and remains the upstream source of truth for now (it is what
   populates sync storage with, e.g., block signatures and pending data). Nothing is
   decommissioned in this step.
3. **No new producers in this step.** If a datum is not already in sync storage, it is
   out of scope for step 1 (or served only where a cheap read/compute path already
   exists) — see the revised boundary analysis in §5.

The reframed question for every endpoint is therefore **"is this datum already in the
sequencer's sync storage?"** — if yes, re-serve it; if no, it is a later-project concern.

## 2. Background: how the two repos relate today

| | `starkware/` (legacy) | `sequencer/` (target) |
|---|---|---|
| Org / lang | `starkware-industries/starkware`, Python (aiohttp) | `starkware-libs/sequencer`, Rust (tokio/axum) |
| Feeder gateway | **Server lives here** (`services/feeder_gateway/`, port 9713) | Does not exist yet |

**Critical inversion to be aware of.** The sequencer is currently a *client* of the
Python feeder gateway, not just a peer:

- `crates/apollo_starknet_client/src/reader/` is a feeder-gateway **reader**.
- `crates/apollo_central_sync` uses it to sync chain data — blocks, state updates,
  **block signatures**, and **pending data** — *from* a StarkNet node's feeder gateway
  (`apollo_central_sync/src/sources/central.rs:330`, `apollo_central_sync/src/lib.rs:399`).

So the current data flow is `Python feeder gateway → (sync) → sequencer storage`. For
**step 1 this flow is preserved, not inverted**: the Python FG remains the upstream
producer, sync keeps populating sequencer storage, and the new component simply adds a
*second read surface* over that already-synced data, in the FG wire format. Because the
data the new FG serves arrived *from* the Python FG, wire-compatibility is naturally
attainable for everything already in storage. The eventual inversion — making the
sequencer the *origin* of this data and retiring the Python FG — is later-project work,
not this step. See the boundary analysis (§5).

## 3. What the Python feeder gateway is

A **read-only HTTP API** (aiohttp, port 9713) serving chain data. It mostly returns
pre-computed data written by upstream services (Exporter, block-hash calculator,
batcher), heavily cached, with on-demand fallback computation.

Route set (full parity target):

- **Blocks / state:** `get_block`, `get_state_update`, `get_block_traces`,
  `get_preconfirmed_block`
- **Contracts / classes:** `get_code`, `get_full_contract`, `get_class_by_hash`,
  `get_class_hash_at`, `get_compiled_class_by_class_hash`, `get_storage_at`, `get_nonce`
- **Transactions:** `get_transaction`, `get_transaction_status`,
  `get_transaction_receipt`, `get_transaction_trace`
- **Read-only execution:** `call_contract`
- **Metadata / misc:** `get_contract_addresses`, `get_signature`, `get_public_key`,
  `get_number_of_transactions_in_backlog`, `get_oldest_transaction_age`
- **Internal id↔hash mappings:** `get_block_hash_by_id`, `get_block_id_by_hash`,
  `get_transaction_hash_by_id`, `get_transaction_id_by_hash`

Reference: `starkware/src/starkware/starknet/services/feeder_gateway/feeder_gateway.py:159`
(routes), `feeder_gateway_impl.py` (assembly logic).

## 4. Key assets already in the sequencer

The migration is far less green-field than it looks, because two existing crates
already cover most of the work:

### 4.1 `apollo_starknet_client` — the wire format, already in Rust

Because this crate *deserializes* the Python feeder gateway's JSON, it already
contains Rust structs locked to the exact wire format (`#[serde(rename …)]`,
version-aware optional fields). For a wire-compatible *server* we can **serialize the
same structs**, which is the single biggest de-risker for requirement #1.

Already modeled (`crates/apollo_starknet_client/src/reader/objects/`):

| FG response | Rust struct | File |
|---|---|---|
| Block (full) | `Block` / `BlockPostV0_13_1` | `objects/block.rs:48,108` |
| Block status | `BlockStatus` | `objects/block.rs:446` |
| Block signature | `BlockSignatureData` / `BlockSignatureMessage` | `objects/block.rs:475,491` |
| State update | `StateUpdate` / `StateDiff` | `objects/state.rs:21,29` |
| Pending data | `PendingData` / `PendingBlockOrDeprecated` | `objects/pending_data.rs:18,25` |
| Transactions (all types/versions) | `Transaction` enum | `objects/transaction.rs:59` |
| Transaction receipt | `TransactionReceipt` | `objects/transaction.rs:769` |
| Contract class (Cairo 0/1) | `GenericContractClass` | `reader/mod.rs:490` |

**Not** modeled here (no client need today) — must be built new for the server:
transaction *traces*, `call_contract`, `get_transaction` / `get_transaction_receipt`
as standalone lookups, `get_class_hash_at` / `get_nonce` / `get_storage_at`,
`get_code` / `get_full_contract`, `get_contract_addresses`.

> Decision needed (D1): whether to lift these structs into a shared `_types` crate or
> depend on `apollo_starknet_client` directly. They are currently partly `#[cfg(test)]`-gated.

### 4.2 `apollo_rpc` — the response-assembly logic, already in Rust

`apollo_rpc` (JSON-RPC 0.8) is a near-complete functional analog: it already turns
`StorageReader` + pending data into block/state/tx/class/event/call/trace responses.
It is **not** a registered node component — only `apollo_state_sync` and integration
tests depend on it; it runs as a standalone server (`apollo_rpc/src/lib.rs:212`,
`run_server`). It is the reference implementation for data→response mapping.

Reusable, high-confidence (`apollo_rpc/src/v0_8/api/api_impl.rs`):

| Capability | Location |
|---|---|
| Block w/ txs + receipts assembly | `:228,268` |
| State update assembly | `:485`, diff enrichment `:1686` |
| Tx by hash / receipt / status | `:387,537,529` |
| Class / class-at / class-hash-at | `:597,660,670` |
| Nonce / storage-at | `:681,340` |
| Events (with pagination) | `:713` |
| `call` (read-only exec) | `:892` → `apollo_rpc_execution::execute_call` |
| Traces (tx + block) | `:1143,1297` → `apollo_rpc_execution::simulate_transactions` |

### 4.3 Underlying data sources (in-process)

- **`apollo_storage`** — headers, bodies (txs/receipts), state diffs, classes, **block
  signatures** (`HeaderStorageReader::get_block_signature`), tx-hash↔index. Direct, fast.
- **`apollo_state_sync`** (`StateSyncClient`) — higher-level block/state/nonce/class queries.
- **`apollo_class_manager`** — sierra/casm class fetch + compilation.
- **`apollo_batcher`** (`BatcherClient`) — `call_contract`, `get_height`, `get_block_hash`,
  proposal content (in-flight block data).
- **`apollo_rpc_execution`** — blockifier wrapper for `call` and trace re-execution.

## 5. Boundary analysis — what is in sync storage vs. not

Under the step-1 principle (§1), the test for each endpoint is simply: **is the data
already in the sequencer's sync storage?** Endpoints split cleanly into three buckets.

### Bucket A — already in sync storage: serve directly (the bulk of the work)
These re-serve synced data using the §4.1 wire structs and §4.2 assembly logic. No new
producer needed.

- `get_block`, `get_state_update`, `get_transaction` / `_status` / `_receipt`
- `get_class_by_hash`, `get_compiled_class_by_class_hash`, `get_class_hash_at`,
  `get_nonce`, `get_storage_at`, `get_code`, `get_full_contract`
- **`get_signature` / `get_public_key`** — signatures are synced from the Python FG into
  storage (`apollo_storage/.../header.rs:152`, written at
  `apollo_central_sync/src/lib.rs:399`). Since the Python FG stays the upstream producer
  in step 1, we just re-serve what's stored. The earlier "who signs locally?" concern is
  **deferred to a later project**, not a step-1 blocker. (Open item: confirm where the
  *public key* is read from for `get_public_key` — synced signature data vs. config.)
- `get_contract_addresses` — from config / system contracts.
- id↔hash mappings (`get_block_hash_by_id`, etc.) — storage hash↔index.

### Bucket B — not in storage but cheaply computable from stored state: include if low-cost
- `call_contract` — read-only execution over stored block state via
  `apollo_rpc_execution::execute_call`. Not "stored" but a pure function of stored state;
  reuse the existing path.
- `get_transaction_trace` / `get_block_traces` — `apollo_rpc` computes these on demand via
  re-execution (`apollo_rpc_execution::simulate_transactions`); nothing is persisted.
  Reusable, but the FG-format trace objects differ from RPC trace objects and must be
  written. **Decision (D4):** include in step 1 or defer — depends on whether external
  consumers need traces from day one.

### Bucket C — NOT in sync storage: out of scope for step 1 (flag explicitly)
- **`get_preconfirmed_block` and locally-built pending blocks.** Preconfirmed blocks are
  produced during consensus and written **write-only to the Cende recorder**
  (`apollo_batcher/src/pre_confirmed_block_writer.rs:71`); they are not in sync storage and
  have no in-process read path. *Synced* pending data (the `PendingData` that sync pulled
  from the Python FG, `apollo_rpc/.../api_impl.rs:1553`) **is** available and may be
  re-served as Bucket A — but exposing the sequencer's own in-flight/preconfirmed blocks is
  a later project (it requires a new query surface over batcher/consensus state).
- **`get_number_of_transactions_in_backlog` / `get_oldest_transaction_age`** — mempool/
  gateway-derived metrics, not sync storage. Defer unless a cheap source exists.

> Net effect of the reframing: the items previously labeled blockers (signatures, synced
> pending data) are now Bucket A. The only genuine step-1 boundary is Bucket C —
> sequencer-originated preconfirmed/pending blocks and mempool metrics.

## 6. Proposed architecture

### 6.1 Component shape
Two viable shapes; recommendation follows.

- **Option A (recommended): active HTTP component, like `apollo_rpc` + `apollo_http_server`.**
  A new `apollo_feeder_gateway` crate runs an **axum** server (`WrapperServer`, an *active*
  component) holding a `StorageReader` plus `Shared*Client`s (`class_manager`, `batcher`,
  `state_sync`) and an `apollo_rpc_execution` handle. Each route assembles its response from
  those sources and serializes the §4.1 wire structs. Mirrors how `apollo_http_server`
  fronts the gateway and how `apollo_rpc` reads storage directly. Lowest boilerplate;
  read-only serving doesn't benefit from a request/response enum.

- **Option B: reactive component (`apollo_feeder_gateway` + `_types` + `_config`).**
  Full three-crate split with a `FeederGatewayRequest`/`Response` enum over local/remote
  infra, plus a thin axum front (like `apollo_http_server`) that forwards to it. More
  boilerplate, but allows running the FG out-of-process (remote) for deployment flexibility.

> Decision needed (D3): A or B. Recommend **A** unless ops needs to split the FG onto its own
> host, in which case **B**. A can be refactored into B later if needed.

### 6.2 Crate layout (Option A)
```
crates/apollo_feeder_gateway/
  src/
    lib.rs            # component + ComponentStarter
    server.rs         # axum router: route -> handler
    handlers/         # one module per endpoint group (blocks, txs, classes, exec, meta)
    objects/          # FG wire structs NOT already in apollo_starknet_client
                      #   (traces, call result, contract_addresses, ...)
    config.rs         # FeederGatewayConfig (port, caching TTLs, feature toggles)
    metrics.rs
crates/apollo_feeder_gateway_config/   # only if Option B
crates/apollo_feeder_gateway_types/    # only if Option B
```
Reuse `apollo_starknet_client::reader::objects` for the modeled wire types (subject to D1),
and `apollo_rpc` / `apollo_rpc_execution` assembly logic for blocks, state, classes, call,
and traces.

### 6.3 Node wiring
Follows the standard recipe (touch 5 files):
`apollo_node_config/src/component_config.rs` (add field) +
`apollo_node/src/{components,servers,clients,communication}.rs`. For Option A it registers
as an active/`WrapperServer` component (like `apollo_http_server`); for Option B also adds
the reactive client/channel plumbing.

## 7. Endpoint → data-source map (full parity)

| Endpoint | Backing source | Wire struct | New work |
|---|---|---|---|
| `get_block` | storage headers+bodies / `apollo_rpc` assembly | `Block` (4.1) | low |
| `get_state_update` | storage state diff / `apollo_rpc` `:485` | `StateUpdate` (4.1) | low |
| `get_transaction` / `_status` / `_receipt` | storage body+output / `apollo_rpc` `:387,529,537` | `Transaction`,`TransactionReceipt` (4.1) | low–med |
| `get_class_by_hash` / `get_compiled_class_by_class_hash` | class_manager / storage classes | `GenericContractClass` (4.1) | low |
| `get_class_hash_at` / `get_nonce` / `get_storage_at` | state reader / `apollo_rpc` `:670,681,340` | scalars | low |
| `get_code` / `get_full_contract` | class + state reader | new objects | med |
| `get_contract_addresses` | config / system contracts | new object | low |
| `call_contract` | `apollo_rpc_execution::execute_call` | new result object | med |
| `get_transaction_trace` / `get_block_traces` | `apollo_rpc_execution::simulate_transactions` | new FG trace objects | Bucket B — med–high; D4 |
| `get_signature` / `get_public_key` | storage signature (synced) + pubkey source | `BlockSignatureData` (4.1) | Bucket A — low |
| `get_preconfirmed_block` + sequencer-built pending | batcher / consensus (no read path) | `PendingData` (4.1) | **Bucket C — deferred to later project** |
| synced pending block / state (from sync's `PendingData`) | sync `PendingData` | `PendingData` (4.1) | Bucket A — med |
| backlog / oldest-tx-age | mempool/gateway metrics (not in storage) | scalars | Bucket C — defer |
| id↔hash mappings | storage hash↔index | scalars | Bucket A — low |

## 8. Suggested phasing

Scoped to step 1 — everything runs alongside the Python FG; nothing is removed.

1. **Phase 0 — scaffolding & Bucket A reads.** New crate + axum server + node wiring;
   implement the in-storage endpoints: `get_block`, `get_state_update`, tx queries,
   class/nonce/storage/code queries, `get_signature` / `get_public_key`,
   `get_contract_addresses`, id↔hash. Validate wire-compat against the Python server with
   golden-file/diff tests (reuse `apollo_starknet_client` to round-trip).
2. **Phase 1 — synced pending + execution.** Re-serve sync's `PendingData` (Bucket A);
   add `call_contract`, then optionally `get_transaction_trace` / `get_block_traces`
   (Bucket B) via `apollo_rpc_execution` per D4.
3. **Phase 2 — parallel-run / diffing.** Run the new FG beside the Python FG on live
   traffic, diff responses, harden parity. Still no removal.

**Out of scope for step 1 (later projects):** sequencer-originated preconfirmed/pending
blocks (Bucket C, needs a batcher/consensus read path), local block-signature production,
mempool-metric endpoints, and the eventual decommission of the Python FG.

## 9. Wire-compatibility verification strategy

- **Golden fixtures.** Capture real Python feeder-gateway JSON responses; assert the new
  server reproduces them byte-for-byte (modulo documented, intentional differences).
  Note the existing `regen-snip35-block-fixture` flow already regenerates a `get_block`
  fixture from the Python repo — extend that pattern per endpoint.
- **Round-trip via `apollo_starknet_client`.** The reader must successfully deserialize
  the new server's output (it already encodes the canonical schema + version handling).
- **Shadow/diff in cutover (Phase 4).** Run new and old side by side; diff responses on
  live traffic before decommissioning.

## 10. Open decisions

- **D1:** Lift `apollo_starknet_client` reader objects into a shared `_types` crate, or
  depend on the crate directly? (Some are `#[cfg(test)]`-gated.)
- **D2:** For `get_public_key`, where is the sequencer public key read from in step 1 —
  the synced `BlockSignatureData`, or a config value? (Local block-*signature production*
  is explicitly out of scope for step 1 — the Python FG remains the signature source.)
- **D3:** Component shape — Option A (active HTTP, recommended) vs B (reactive + remote-capable)?
- **D4:** Include execution endpoints (`call_contract`, traces — Bucket B) in step 1, or
  defer until after the in-storage endpoints ship? Depends on external-consumer need.
- **D5:** Caching parity — the Python server has extensive TTL/LRU caching. Match it, or
  rely on storage speed + lighter caching initially?
- **D6:** Confirm the full external-consumer route inventory (wallets/explorers) so we know
  the exact parity surface required, and whether any Bucket C routes are load-bearing for
  external clients (which would change their priority).
```
