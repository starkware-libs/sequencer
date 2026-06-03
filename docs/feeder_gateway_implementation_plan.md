# Runbook: Implement the Feeder Gateway as a Sequencer Component

> Step-by-step execution runbook. Each "PR" is self-contained: exact files, exact code,
> exact commands. Do PRs in order; do not combine. `/validate` after every PR.
> Authoritative copy lives at `docs/feeder_gateway_implementation_plan.md`.

## Context (why)

The StarkNet feeder gateway (FG) is a read-only HTTP API that today lives only in the legacy
Python monorepo (`starkware-industries/starkware`,
`src/starkware/starknet/services/feeder_gateway/`, port 9713). It serves chain data to
wallets, explorers, and the sequencer's own `apollo_starknet_client`; the sequencer currently
*consumes* it via `apollo_central_sync`. We add a native `apollo_feeder_gateway` component
that re-serves data **already in the sequencer's sync storage** in the **legacy FG JSON wire
format** (semantically equal — Reference B), running **alongside** the Python FG. Step 1 of an
eventual multi-project removal; nothing legacy is removed here.

## Global rules (EVERY PR)
- Create each PR with `/create-pr`; `/validate` after each (commitlint, fmt, clippy, tests,
  unused-deps) + `unset CI && scripts/rust_fmt.sh`. Fix until green.
- Commit first line `scope: subject` (no type prefix, ≤100 chars). Scopes:
  `workspace`, `apollo_feeder_gateway`, `apollo_feeder_gateway_config`, `apollo_node`,
  `apollo_state_sync`, `apollo_dashboard`, `deployment` (added to `commitlint.config.js` in PR1/2).
- One concern per PR. New crates land EMPTY. Config fields added ONLY in the PR that first
  reads them.
- Regen gates (run + commit JSON in the SAME PR): `ComponentConfig`/`SequencerNodeConfig`
  change → `cargo run --bin update_apollo_node_config_schema` AND `cargo run --bin
  deployment_generator`; new metric → `cargo run --bin sequencer_dashboard_generator`; new
  deployment service → `cargo run --bin deployment_generator`.
- "ANCHOR" = existing line to locate; "ADD AFTER" = insert right after it. Keep alphabetical
  order in lists. Template: `crates/apollo_http_server` (+`_config`).

---

# Reference A — StateSyncClient reads: exist vs add

FG depends on `SharedStateSyncClient` (local/remote per config) + `SharedClassManagerClient`.
**`SyncBlock` (`state_sync_types.rs:17`) holds only tx HASHES**, not txs/outputs. Each MISSING
read gets an "(a) extension PR" adding a `StateSyncRequest`/`StateSyncResponse` variant + trait
method + handler arm + storage-reading impl. The verbatim pattern is in Reference C.

| FG read | Today | Action (variant) | Backing StorageReader |
|---|---|---|---|
| storage_at/nonce_at/class_hash_at | EXISTS | use | `StateReader::{get_storage_at:407, get_nonce_at:344, get_class_hash_at:298}` (`state/mod.rs`, verified 2026-06-02) |
| block_hash(n), latest number/header | EXISTS | use | header storage |
| state diff | MISSING | `GetStateDiff(BlockNumber)->ThinStateDiff` | `get_state_diff` (`state/mod.rs:142`) |
| block txs+outputs | MISSING | `GetBlockTransactionsWithOutputs(BlockNumber)->Vec<(Transaction,TransactionOutput,TransactionHash)>` | `get_block_transactions`+`get_block_transaction_outputs`+`get_block_transaction_hashes` (`body/mod.rs`) |
| tx by hash | MISSING | `GetTransactionByHash(TransactionHash)->Option<(Transaction,TransactionOutput,BlockNumber,TransactionOffsetInBlock)>` | `get_transaction_idx_by_hash`→`get_transaction`+`get_transaction_output` (`body/mod.rs:100/106`) |
| block signature | MISSING | `GetBlockSignature(BlockNumber)->Option<BlockSignature>` | `get_block_signature` (`header.rs:152`) |
| block number by hash | MISSING | `GetBlockNumberByHash(BlockHash)->Option<BlockNumber>` | `get_block_number_by_hash` (`header.rs`) |
| compiled class hash | MISSING | `GetCompiledClassHash(BlockNumber,ClassHash)->Option<CompiledClassHash>` | `CasmStorageReader::get_compiled_class_hash` (`compiled_class.rs:84`) |
| sierra/casm class | via `SharedClassManagerClient` (`get_sierra`/`get_executable`) | use | class manager |

Full header = `get_block_header(n)` (whose `.block_hash` is the block hash) + commitments.
⚠ TRAP (verified 2026-06-02): do NOT use the free-standing `get_block_hash(n)`
(`apollo_storage/src/block_hash.rs:36`) — it reads a SEPARATE `block_hashes` table written ONLY by
the batcher during block production, NOT by the sync write paths, so a co-located FG re-serving
SYNCED data gets `None`. Always read the hash from `get_block_header(n).block_hash` (header.rs:208-216;
written by sync's `append_header`). The row above ("block_hash(n) … header storage") means exactly this.

# Reference B — JSON parity assertion (BYTE/ORDER-EXACT, not semantic)

**Requirement (from review): backwards compatibility includes JSON key ORDER.** So parity is
asserted by **exact byte equality** against captured Python-FG output — NOT by `serde_json::Value`
equality (which normalizes away key order, map order, and whitespace — the very things we must
preserve). Byte parity has **five axes**, each a hard constraint (verified against the Python FG):

| Axis | Python FG | Rust requirement |
|---|---|---|
| **Struct key order** | marshmallow field declaration order; `sort_keys=False` everywhere | serde emits in field-declaration order → reorder Rust structs to match (PR-parity-1) |
| **Map/dict order** | **insertion order** (Py3.7 dict), NOT sorted (`storage_diffs`, `nonces`, builtins) | `IndexMap` with the SAME insertion order Python builds — `HashMap`/`BTreeMap` both wrong (PR-parity-2) |
| **Separators** | **SPACED** `json.dumps` default `", "` / `": "`, single line | serde default is **COMPACT** → need a custom single-line-spaced `Formatter` (PR-parity-3) |
| **Felt/number** | felts = `hex()` → lowercase `0x`, no leading zeros (`0x0`,`0xf`); ints = JSON numbers | `starknet_api::Felt` Serialize must match — lock with a test |
| **Null** | `remove_none_values` post-dump → None fields **omitted** (never `null`) | every optional field needs `#[serde(skip_serializing_if = "Option::is_none")]` |

**Per-endpoint test (the parity lock):**
```rust
// Capture the Python FG's literal json.dumps bytes once, commit as a fixture.
let expected = read_resource_file("parity/<endpoint>.json"); // raw Python bytes
let actual = to_python_json(&fg_struct)?;                     // FG spaced formatter (PR-parity-3)
assert_eq!(actual, expected);  // EXACT bytes — no Value round-trip, no normalization
```
The only permitted pre-processing is stripping one trailing newline if the capture tool added it
(document at the capture site). FG response handlers MUST serialize via `to_python_json`, never
`serde_json::to_string`/`to_vec` or axum `Json<T>` (those are compact → byte mismatch). Refresh
`get_block` fixtures via `/regen-snip35-block-fixture`. The byte-parity infra PRs are in Phase D
(PR-parity-1..4); they land BEFORE any JSON-emitting handler (E0).

# Reference C — Verbatim "extend StateSyncClient" pattern

Apply this for every Reference-A MISSING read. Example shown for `GetStateDiff`.
In `crates/apollo_state_sync_types/src/communication.rs`:
```rust
// 1. enum StateSyncRequest { ... add:
    GetStateDiff(BlockNumber),
// 2. enum StateSyncResponse { ... add:
    GetStateDiff(StateSyncResult<starknet_api::state::ThinStateDiff>),
// 3. trait StateSyncClient { ... add:
    async fn get_state_diff(
        &self, block_number: BlockNumber,
    ) -> StateSyncClientResult<starknet_api::state::ThinStateDiff>;
// 4. blanket impl (mirror get_block_hash; use `Direct`, or `Boxed` if the response is Box<_>):
    async fn get_state_diff(
        &self, block_number: BlockNumber,
    ) -> StateSyncClientResult<starknet_api::state::ThinStateDiff> {
        let request = StateSyncRequest::GetStateDiff(block_number);
        handle_all_response_variants!(
            self, request, StateSyncResponse, GetStateDiff,
            StateSyncClientError, StateSyncError, Direct)
    }
// 5. add the discriminant to PrioritizedRequest::priority match (Normal).
```
In `crates/apollo_state_sync/src/lib.rs`:
```rust
// handle_request match: add
    StateSyncRequest::GetStateDiff(block_number) =>
        StateSyncResponse::GetStateDiff(self.get_state_diff(block_number).await),
// impl StateSync: add (mirror existing get_block at lib.rs:162)
    async fn get_state_diff(&self, block_number: BlockNumber)
        -> StateSyncResult<starknet_api::state::ThinStateDiff> {
        let txn = self.storage_reader.clone().begin_ro_txn()?;
        txn.get_state_diff(block_number)?.ok_or(StateSyncError::BlockNotFound(block_number))
    }
```
Note: every type carried by a request/response variant must be `Serialize + Deserialize`
(all listed `starknet_api` types are). Add a test under `apollo_state_sync` for the new method.

---

# Reference D — Performance & scaling architecture (READ BACKEND)

Core of the design: serve **very high RPS** with **true parallel MDBX reads**, correct in
same-pod / different-pod / different-node deployments, caching only at the very top and NOT
required to hit throughput targets.

## D.1 The `ChainDataReader` trait — one abstraction, two backends
FG handlers depend on a `ChainDataReader` trait (`apollo_feeder_gateway/src/reader.rs`), never
on a concrete client. Two impls, chosen by node config:
1. **`ColocatedStorageReader` (co-located with state-sync, same process — highest perf).** Holds
   an `apollo_storage::StorageReader` (`Clone + Send + Sync`; internally `Arc<Environment>`,
   so cloning shares the single MDBX env — no second handle, no extra storage). EVERY read is
   dispatched to the bounded `ReadExecutor` (D.11) — shown below as raw `spawn_blocking` ONLY for
   illustration; the real path uses `ReadExecutor::run` (NOT raw `spawn_blocking`, NOT the 512 pool):
   ```rust
   let reader = self.storage_reader.clone();
   self.executor.run(move || {                    // bounded read pool (D.11), reactor stays free
       let txn = reader.begin_ro_txn()?;          // MDBX read txn (MVCC, lock-free reader)
       txn.get_block_header(block_number)         // mmap read
   }).await?
   ```
   ("spawn_blocking" throughout D.1–D.3 means dispatch via this bounded `ReadExecutor`.)
   This mirrors `apollo_rpc` (`run_server` is handed a `StorageReader` directly and is spawned
   co-located inside `StateSyncRunner`).
2. **`RemoteChainDataReader` (different pod / node).** Holds a `SharedStateSyncClient` (and/or
   the existing `StorageReaderServer` HTTP/2 `/storage/query` for the richer query set). Each
   read is an HTTP/2 request to the state-sync/storage pod, which itself parallelizes (D.5).

## D.2 Why spawn_blocking is mandatory for PARALLEL (not just concurrent) reads
MDBX reads are synchronous, memory-mapped, CPU/page-cache-bound. Running `begin_ro_txn()`
**inline** in an `async fn` blocks a tokio **worker** thread (only `#cores` of them) → caps
parallelism at `#cores` and starves the reactor. `spawn_blocking` moves each read to tokio's
**blocking pool** (default 512 threads); MDBX MVCC permits many simultaneous read txns across
threads (up to `max_readers`, an apollo_storage config well above the blocking-pool size). →
**hundreds of MDBX reads truly in parallel.** Applies to both backends (local: spawn_blocking
here; remote: spawn_blocking in the state-sync read handlers — note today's
`StateSync::get_block` reads INLINE, which the remote-path PRs must change).

## D.3 Why co-located reads do NOT go through the local StateSyncClient
The local `SharedStateSyncClient` path adds per read: mpsc send + per-request oneshot +
component clone + `ConcurrentLocalComponentServer` task spawn + `max_concurrency` semaphore,
and the handler still reads inline. Holding `StorageReader` directly + `spawn_blocking` removes
that hop and the cap, as `apollo_rpc` does. So: **co-located → `ColocatedStorageReader` (direct);
remote → `RemoteChainDataReader`.**

## D.4 Node wiring (the storage-threading refactor)
`create_state_sync_and_runner` (`apollo_state_sync/src/lib.rs:38`) already gets `storage_reader`
from `StateSyncRunner::new` (`runner/mod.rs:190` → `(Self, StorageReader)`) but **discards** it
(returns only `(StateSync, StateSyncRunner)`). Refactor it to also return the `StorageReader`
(or expose `StateSyncResources.storage_reader`); then in
`apollo_node/src/components.rs::create_node_components`: FG Enabled + state-sync local → build
with `ColocatedStorageReader(storage_reader.clone())`; state-sync `Remote` → build with
`RemoteChainDataReader(state_sync_client)`. No behavior change to state-sync. (Supersedes the
earlier "config-only generic client" note: the generic boundary is the `ChainDataReader` trait;
the co-located impl is direct storage for max throughput.)

## D.5 Deployment scenarios — expected performance & bottleneck
| Scenario | Backend | Per-read latency | Parallelism | Bottleneck |
|---|---|---|---|---|
| Same pod (co-located) | `ColocatedStorageReader` + spawn_blocking | ~µs–low-ms (mmap/cache) | blocking pool (≈512) & `max_readers` | CPU cores, page cache, blocking pool |
| Different pod (same cluster) | `RemoteChainDataReader` → HTTP/2 | + intra-cluster RTT + serde | state-sync pod's blocking pool | state-sync read throughput + network; FG stateless, HPA-scaled |
| Different node/zone | `RemoteChainDataReader` | + higher RTT | same | network latency → prefer coarse reads (whole block in one call) + connection pooling |

## D.6 Very high RPS WITHOUT caching
Finalized data is immutable; reads are cheap mmap lookups; axum schedules each request as an
independent task across ALL cores with **no semaphore** (unlike reactive components); reads run
in parallel on the blocking pool. A co-located FG can saturate cores on reads alone and hit high
RPS with the cache OFF. Design target: meet RPS cache-off; the cache only trims hot-key/network
work.

## D.7 Caching strictly at the TOP of the stack
A `CachingReader` decorator **wraps the `ChainDataReader` trait at the outermost layer**:
`Handlers → CachingReader → (ColocatedStorageReader | RemoteChainDataReader)`. Finalized entries
(by block number/hash) cached with **no invalidation** (immutable); only `latest`/`pending` get
short TTLs. Being a trait wrapper, it helps the remote path most (elides network) and is fully
removable (config-gated) without touching handlers — see Phase G.

## D.8 Inline vs spawn_blocking — the decision (reactor safety)
`apollo_rpc` reads PLAIN data **inline** in the async handler (`api_impl.rs:340,387,191`) and
uses `spawn_blocking` only for heavy `call`/`trace`/`estimate` (`:909,1033,1103,1260`). Inline
reads on the multi-threaded runtime DO run in parallel — but only up to `#worker_threads`
(= #cores), and a slow read (page-cache miss → disk) **blocks a reactor worker**, starving HTTP
accept/scheduling. For a read-dominated FG at very high RPS, **dispatch every read via
`spawn_blocking`**: it (a) keeps the few reactor workers free, and (b) lets parallelism be
bounded by a dedicated blocking pool rather than #cores. Reads are CPU/page-cache-bound, so real
throughput is still ~#physical-cores; size the blocking pool accordingly (D.9), don't oversize.
(Inline is the simpler fallback if the POC perf test shows reads are always fast cache hits — but
spawn_blocking is the default for reactor protection.)

## D.9 Runtime configuration
`apollo_node/src/main.rs:36` uses bare `#[tokio::main]` → worker_threads = #cores,
`max_blocking_threads = 512` with an **unbounded** queue. Do NOT rely on the global 512 pool for
reads. Instead route reads through a dedicated **bounded `ReadExecutor`** (D.11) sized
`read_pool_size = 1.5 × cores` (via `std::thread::available_parallelism`; NOT 512 — an oversized pool just context-switches CPU-bound
reads, and the unbounded queue has no backpressure). `worker_threads ≈ #cores` for async/axum.
`max_readers = 8K` (`apollo_storage/src/db/mod.rs:91`) far exceeds the read pool, so the MDBX
reader table is never the limit. axum keeps **no concurrency semaphore** (like `http_server`) — the
bounded `ReadExecutor` is the read throttle/backpressure.

## D.10 Verification (adversarial — all CONFIRMED)
- **Parallel MDBX reads:** CONFIRMED. `StorageReader` is `Clone+Send+Sync` over
  `Arc<Environment>` (`lib.rs:496`), libmdbx `Transaction`/`Database` are `Send+Sync`, NO_TLS mode
  (`db/mod.rs:51`), `max_readers=8K` (`:91`) — MVCC lock-free readers; true parallelism via
  spawn_blocking, bounded by cores.
- **Local StateSyncClient is a bottleneck:** CONFIRMED. ~200–500 µs/read overhead (mpsc + oneshot
  + clone + spawn + semaphore), `max_concurrency=128` cap, inline `begin_ro_txn` blocks reactor
  (`apollo_state_sync/src/lib.rs:165`); the gateway even `block_on`s it. Direct StorageReader
  ≈ 30–50× faster at saturation → co-located FG MUST use `ColocatedStorageReader`, not the local client.
- **apollo_rpc direct co-located pattern:** CONFIRMED. `run_server` is handed `StorageReader`
  directly and spawned in `StateSyncRunner` (`runner/mod.rs:234`) — the pattern the FG copies.

## D.11 Read execution: a BOUNDED `ReadExecutor` (don't `spawn_blocking` unboundedly) + own-pod analysis
Decision record answering: *"we cannot `spawn_blocking` too much — would running FG in its own pod
(one per replica) help?"* Short answer: **bounding `spawn_blocking` and running own-pod are two
different levers** — do both, but only one of them scales reads.

**Bounding `spawn_blocking` (the real fix).** Tokio's blocking pool grows lazily up to
`max_blocking_threads` (default **512**) with an **unbounded queue and no backpressure**. Two
failure modes for our CPU/page-cache-bound MDBX reads (~5 µs each; useful parallelism ≈ #cores):
(1) **thread thrash** — letting the pool reach hundreds of threads on a ~16-core box means most just
context-switch (L3/TLB churn), degrading p99 ~3–5×; (2) **unbounded queue** (primary risk) — once all
threads are busy, every further read queues indefinitely → memory + latency blow up under a spike.
**Fix:** route ALL blocking reads through a dedicated **`ReadExecutor` with bounded concurrency ≈ 1.5×
physical cores** — NOT raw `spawn_blocking`, NOT the global 512 pool, NOT inline-on-the-reactor. This is
what D.2/D.9 mean by "spawn_blocking": through this bounded executor.
**Mechanism (important):** do NOT implement this as a second tokio runtime with capped
`max_blocking_threads` — tokio's `spawn_blocking` queue is **unbounded even when the thread count is
capped**, so backpressure never kicks in (it just queues, the exact failure mode (2) above).
Implement it as a **fixed `std::thread` worker pool of size `read_pool_size` fed by a BOUNDED channel**
(`std::sync::mpsc::sync_channel(channel_capacity)` or `tokio::sync::mpsc::channel(cap)`), with results
returned via `tokio::sync::oneshot`. **Backpressure model (decided, M5): `run()` AWAITS on a full
channel — it never rejects** (so there is no `ServiceOverloaded` error path and handlers need no
overflow branch; a saturated read queue simply slows acceptance, which is the desired natural
throttle). Use `tokio::sync::mpsc` so the send awaits. Repo precedent: `blockifier/src/concurrency/worker_pool.rs`.
```rust
pub struct ReadExecutor { /* fixed std::thread pool (read_pool_size) + bounded job channel */ }
impl ReadExecutor {
    pub fn new(read_pool_size: usize, channel_capacity: usize) -> Self { /* spawn N workers reading a bounded chan */ }
    pub async fn run<F, T>(&self, f: F) -> FgResult<T>
    where F: FnOnce() -> T + Send + 'static, T: Send + 'static {
        // bounded send(job + oneshot tx) → worker runs f() → reply via oneshot; full queue = backpressure
    }
}
// ColocatedStorageReader holds Arc<ReadExecutor>; every begin_ro_txn read goes through run(...).
```
Sizing: `read_pool_size = 1.5 × cores` (default when unset; use
`std::thread::available_parallelism()` — NOT a new `num_cpus` dep). 4c→6, 8c→12, 16c→24, 32c→48.
`channel_capacity` bounds the backlog (e.g. a small multiple of `read_pool_size`).

**Does own-pod / one-per-replica help?** It helps with the EXPENSIVE, scalable work, NOT with raw
reads:
- For a 300-tx `get_block`, the per-request cost is ≈ **5 ms JSON serialization** (Felt→hex of ~9.6k
  felts) + ≈ 2.5 ms struct conversion vs only ≈ **150 µs** of MDBX reads (~2%). So serialization
  dominates.
- In own-pod (remote) mode FG replicas do **zero local MDBX reads** — every read is an HTTP/2 hop to
  the single storage/StateSync process. So replicas scale **serialization + HTTP + reactor** linearly
  (the dominant cost moves off the storage node) — a real, recommended win — but they do **NOT** raise
  read throughput: all reads serialize through one storage process.
- **Storage-side ceiling correction:** that ceiling is NOT MDBX core count (MDBX allows ~8K concurrent
  readers); it is today the component-handler `max_concurrency` (default **128**, unrelated to cores)
  with reads run **inline** in the async handler (`apollo_state_sync/src/lib.rs:112-158`). So the
  storage side needs the SAME bounded `ReadExecutor` (sized ≈ its cores) instead of inline-gated-by-128.
- **Fleet read ceiling = Σ (per-storage-node `ReadExecutor`)**. Raising it needs MORE storage nodes
  (read replicas) — out of scope. Own-pod replica count is **not** a read-scaling knob.

**Per-topology guidance:**
- **(a) Co-located on each EXISTING storage node** — `ColocatedStorageReader` reads local MDBX via the
  `ReadExecutor` (≈1.5× that node's cores); no hop, no cross-process serialization. Highest read
  throughput; scales with the existing storage fleet. Recommended default where co-location is possible.
- **(b) Own-pod remote replicas** — scale serialization/HTTP/reactor linearly; size the **storage
  node's** `ReadExecutor` ≈ its cores and replace its inline reads. **Cheap wire codec (recommended
  here):** the remote path double-serializes today (storage JSON-encodes `SyncBlock`, FG decodes, then
  FG re-encodes to FG-JSON, ~6–9 ms extra per large block). Use a cheap internal codec (bincode/protobuf)
  storage↔FG so the heavy client-facing FG-JSON serialization runs on the scalable FG replicas and the
  storage node only pays a cheap encode.
- **(c) Raising the read ceiling** = more storage nodes (out of scope); documented escape hatch.

**Plan edits driven by D.11** (applied below): a new `ReadExecutor` PR; `ColocatedStorageReader`
(PR18) routes reads through it; `read_pool_size: Option<usize>` config (default `1.5×num_cpus`); the
remote-mode StateSync/`StorageReaderServer` read handlers use a bounded executor too (not inline); an
optional cheap-codec PR for the own-pod topology. The Reference D narrative already states replicas
scale serialization not reads (D.5/D.6) — consistent with this.

---

# Reference E — Python-FG parity checklist (behaviors the Rust impl MUST match)

Source: `starkware/.../feeder_gateway/{feeder_gateway,feeder_gateway_impl,response_objects,error_codes}.py`,
`aiohttp_utils.py`. Drive the parity-quirks PR and the per-endpoint API-diff tests (Reference F).

**Error envelope & HTTP status** (`aiohttp_utils.py:112–181`)
- [ ] Body shape `{code, message, [problems]}`. `StarknetErrorCode` → **400** by default;
  `BLOCK_NOT_FOUND`/`TRANSACTION_NOT_FOUND` → **404**; unhandled → **500**.
- [ ] Emit exact codes. **Most do NOT yet exist in `KnownStarknetErrorCode`
  (`apollo_gateway_types/src/deprecated_gateway_error.rs:25-66`)** — the PR must decide per code:
  EXISTING (reuse): `BLOCK_NOT_FOUND`, `UNDECLARED_CLASS`, `OutOfRangeClassHash` (verified present).
  MISSING (verified absent — `NO_BLOCK_HEADER` does NOT exist) — either ADD a `KnownStarknetErrorCode`
  variant `#[serde(rename="StarknetErrorCode.<NAME>")]` or emit via
  `StarknetErrorCode::UnknownErrorCode("StarknetErrorCode.<NAME>")`:
  `NO_BLOCK_HEADER`, `INVALID_TRANSACTION_HASH`, `TRANSACTION_NOT_FOUND`, `UNINITIALIZED_CONTRACT`,
  `NO_TRACE`, `NO_SIGNATURE_FOR_PENDING_BLOCK`, `OUT_OF_RANGE_CONTRACT_ADDRESS`,
  `OUT_OF_RANGE_CONTRACT_STORAGE_KEY`, `OUT_OF_RANGE_TRANSACTION_HASH`, `NO_STATE_UPDATE`. The PR
  must list which it ADDs vs emits-as-UnknownErrorCode.
- [ ] `MALFORMED_REQUEST` keeps the UPSTREAM serde string `"StarkErrorCode.MALFORMED_REQUEST"`
  (`StarkErrorCode`, not `Starknet` — `deprecated_gateway_error.rs:30`) — match verbatim, do not "fix".

**Status semantics** (`response_objects.py:98–160`, computed on demand against the active chain)
- [ ] `BlockStatus`: `PENDING`, `ACCEPTED_ON_L2` (default), `PROVEN_ON_L2` (config-gated, like
  `enable_proven_on_l2_status`), `ACCEPTED_ON_L1` (≤ base-layer marker), `ABORTED` (not on active chain).
- [ ] `FinalityStatus`: `NOT_RECEIVED`/`RECEIVED`/`ACCEPTED_ON_L2`/`PROVEN_ON_L2`/`ACCEPTED_ON_L1`.
- [ ] `ExecutionStatus`: `SUCCEEDED`/`REVERTED`/`REJECTED` (REJECTED only when in-batch but skipped validation).

**Endpoint quirks**
- [ ] `get_block?headerOnly=true` → stripped `BlockHeader`, not a full block.
- [ ] pending block: `block_hash`/`block_number`/`state_root`/`starknet_version` = `null`;
  `transaction_receipts` = `null` if ABORTED. `withFeeMarketInfo`/`withFeeProposalInfo` strip fee fields.
- [ ] `get_state_update?includeBlock=true` merges block+update; `includeSignature=true` REQUIRES
  `includeBlock=true` (else error); same fee stripping.
- [ ] `get_block_traces`: reject pending; `NO_TRACE` if not computed/cached.
- [ ] `get_transaction_status`: `{tx_status, finality_status, execution_status, [block_hash if finalized],
  [tx_failure_reason|tx_revert_reason]}`.
- [ ] `get_transaction_receipt`: omit `actual_fee`/events/messages/`execution_resources` when not executed.
- [ ] `get_code`/`get_full_contract`: `{bytecode:[], abi:{}}` for `UNINITIALIZED_CONTRACT`; Cairo1
  `sierra_program`, Cairo0 `program`.
- [ ] `get_signature`: `NO_SIGNATURE_FOR_PENDING_BLOCK` for pending.
- [ ] `get_preconfirmed_block`: delta protocol — if `knownTransactionCount` matches current
  `blockIdentifier`, return `{changed:false}`; filter trailing candidate txs (receipts=None). (Bucket C — defer.)
- [ ] id↔hash endpoints return raw values (no wrapper). `get_contract_addresses`/`call_contract`/
  `get_public_key` shapes.

**Caching-semantics differences (parity, not perf)**
- [ ] Python serves pending/preconfirmed with a **15 s TTL** (up to 15 s stale) and finalized block
  JSON with a 2 min TTL (memory bound only). Decide & document Rust's pending staleness window;
  finalized entries in the Rust top-of-stack cache can live indefinitely (immutable).

# Reference F — Benchmark & API-diff strategy

**F.1 Load-test the Rust FG (RPS).** Extend `apollo_integration_tests` `flow_test_setup.rs`
(boots a full node with HTTP + pre-seeded `get_integration_test_storage`) and drive it with
`HttpTestClient` (`apollo_http_server/src/test_utils.rs:33`, reqwest). Add a concurrency loop with
`Instant` latency capture + `Arc<AtomicU64>` counters; report RPS + p50/p95/p99. Sweep
`max_blocking_threads`, `worker_threads`, concurrency to find the knee and confirm throughput
scales with cores (validates D.2/D.9). Run `--release`. For ad-hoc saturation curves, point `oha`/
`wrk`/`vegeta` at a running FG with a fixed list of finalized block numbers/addresses.

**F.2 Diff Rust-FG vs Python-FG (API parity).** Capture a curated fixture set of real Python-FG
responses per endpoint (old finalized, recent finalized, pending, ABORTED, headerOnly, with/without
fee flags, uninitialized contract, undeclared class, out-of-range addr/key, every error). Store as
JSON — the **literal `json.dumps` bytes** (store under `resources/parity/<endpoint>.json`). Diff
harness (Reference B): for each fixture, build the Rust object from the same inputs, serialize via
`to_python_json` (PR-parity-3), and **`assert_eq!` on the EXACT bytes** — NO `serde_json::Value`
round-trip, NO key-normalization (that would mask the order/whitespace we must preserve). `expect_file!`
(regen `EXPECT_TEST=1`) drives the fixtures; `pretty_assertions`/`similar` for readable diffs. One
API-difference test module per endpoint exercising every Reference-E quirk, plus negative fixtures
asserting the byte-exact error envelope + HTTP status per `StarknetErrorCode`.

---

# PR INDEX & execution order (read this first)

Linear order to execute. **POC-critical** path is marked ★ — everything else is post-POC.
Sub-PR convention for endpoints: **`.a`** = extend StateSyncClient (Reference C, only if MISSING),
**`.b`** = `starknet_api→FG` conversion + test, **`.c`** = axum handler + byte-parity test.
Deps in (parens). REGEN = run+commit the named generator.

1. ★ **PR1** workspace: empty `_config` crate (+ root Cargo.toml members + commitlint scopes)
2. ★ **PR2** workspace: empty `apollo_feeder_gateway` crate
3. ★ **PR3** empty `FeederGatewayConfig` (→PR1)
4. ★ **PR4** `FeederGateway` struct (→PR2,PR3)
5. ★ **PR5** run-error + `ComponentStarter` (→PR4)
6. ★ **PR6** `WrapperServer` alias (→PR5)
7. ★ **PR7** config: bind ip/port (→PR3)
8. ★ **PR8** axum health routes (→PR6,PR7)
9. ★ **PR9** node `ComponentConfig.feeder_gateway` field [REGEN: config_schema + deployment_generator]
10. ★ **PR10** node `Option<FeederGatewayConfig>` (→PR9) [REGEN]
11. ★ **PR11** construct component (→PR10)
12. ★ **PR12** run wrapper server (→PR11) — node boots FG (disabled), serves health
13. ★ **PR16** `ChainDataReader` trait + `AppState` (→PR2)
14. ★ **PR17** `apollo_state_sync`: return `storage_reader` (refactor, no behavior change)
15. ★ **PR17b** bounded `ReadExecutor` (→PR16)
16. ★ **PR18** `ColocatedStorageReader` (→PR17b)
17. ★ **PR19** select backend by config + add read-pool config fields (→PR17,PR18) [REGEN]
18.   **PR20** `RemoteChainDataReader` (→PR16; default-off per B6, post-POC activation)
19. ★ **PR-parity-1** reorder reader structs to Python order (`apollo_starknet_client`)
20. ★ **PR-parity-2** HashMap→IndexMap map fields (`apollo_starknet_client`)
21. ★ **PR-parity-3** `to_python_json` spaced serializer + `fg_json` helper (→PR2)
22. ★ **PR-parity-Felt** lock Felt (+ newtype) JSON format
23. ★ **PR15** legacy error envelope (→PR-parity-3) [reuse `serialize_error`/`StarknetErrorCode`]
24. ★ **E0** get_contract_addresses (→PR-parity-3,PR16) — first JSON handler
25. ★ **E1.a** state-sync: `GetBlockTransactionsWithOutputs`+`GetBlockSignature`+`GetBlockNumberByHash`
26. ★ **E0.b/E3.b** block-status + finality-status computation (core, M7)
27. ★ **E1.b*** block-header + per-tx-family + receipt + exec-resources conversions (+tests)
28. ★ **E1.c** serve `get_block` (+ E1.d headerOnly, E1.e fee-flag stripping) — **★ POC MILESTONE**
29. ★ **PR21** POC perf test + runtime/ReadExecutor sizing — **gate on target RPS**
30.   **PR13** request/latency/error metrics · **PR14** dashboard [REGEN: dashboard_generator] (post-POC)
31.   **EXPAND:** E2 (state_update +E2.d) · E3/E4 (tx/receipt/status) · E5 (classes) · E6 (storage/nonce/class_hash)
      · E7 (code/full_contract) · E8 (id↔hash) · E9 (signature/public_key) · E10 (synced pending)
      · E11 (e2e smoke) · E12 (backlog/oldest-tx-age)
32.   **Phase F** compute (call_contract/traces; needs D3) · **Phase G** caching (optional) ·
      **Phase H** deployment · **Phase I** parity hardening + benchmark/API-diff

References: A (state-sync reads) · B (byte/order parity) · C (extend-StateSyncClient pattern) ·
D + D.11 (perf/read-execution) · E (Python parity checklist) · F (benchmark/API-diff).

---

# PHASE A — Create the two crates (empty)

## PR1 — `workspace: create empty apollo_feeder_gateway_config crate`
Create `crates/apollo_feeder_gateway_config/Cargo.toml`:
```toml
[package]
name = "apollo_feeder_gateway_config"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
description = "Configuration types for the Apollo feeder gateway."

[lints]
workspace = true

[dependencies]
apollo_config.workspace = true
serde.workspace = true
validator.workspace = true

[dev-dependencies]
rstest.workspace = true
```
Create `crates/apollo_feeder_gateway_config/src/lib.rs`:
```rust
//! Configuration types for the Apollo feeder gateway.
```
Edit root `Cargo.toml`: add `"crates/apollo_feeder_gateway"` and
`"crates/apollo_feeder_gateway_config"` to `members` (alphabetical, near other `apollo_f*`);
in `[workspace.dependencies]` add `apollo_feeder_gateway.path = "crates/apollo_feeder_gateway"`
and `apollo_feeder_gateway_config.path = "crates/apollo_feeder_gateway_config"`.
Edit `commitlint.config.js`: add `'apollo_feeder_gateway',` and `'apollo_feeder_gateway_config',`.
Validate: `cargo build -p apollo_feeder_gateway_config`. Commit:
`workspace: create empty apollo_feeder_gateway_config crate`

## PR2 — `workspace: create empty apollo_feeder_gateway crate`
Create `crates/apollo_feeder_gateway/Cargo.toml`:
```toml
[package]
name = "apollo_feeder_gateway"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
description = "Feeder gateway read API server for the Apollo sequencer."

[features]
testing = []

[lints]
workspace = true

[dependencies]
apollo_infra.workspace = true
apollo_infra_utils.workspace = true
async-trait.workspace = true
tracing.workspace = true

[dev-dependencies]
rstest.workspace = true
```
Create `crates/apollo_feeder_gateway/src/lib.rs`:
```rust
//! Feeder gateway read API server for the Apollo sequencer.
```
Validate: `cargo build -p apollo_feeder_gateway`. Commit:
`workspace: create empty apollo_feeder_gateway crate`

---

# PHASE B — Config struct + component skeleton (incremental)

## PR3 — `apollo_feeder_gateway_config: add empty FeederGatewayConfig`
Create `crates/apollo_feeder_gateway_config/src/config.rs`:
```rust
use std::collections::BTreeMap;

use apollo_config::dumping::SerializeConfig;
use apollo_config::{ParamPath, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Configuration for the feeder gateway component.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct FeederGatewayConfig {}

impl SerializeConfig for FeederGatewayConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::new()
    }
}
```
`lib.rs`: add `pub mod config;`. Commit: `apollo_feeder_gateway_config: add empty FeederGatewayConfig`

## PR4 — `apollo_feeder_gateway: add FeederGateway component struct`
`Cargo.toml`: add `apollo_feeder_gateway_config.workspace = true`.
Create `crates/apollo_feeder_gateway/src/feeder_gateway.rs`:
```rust
use apollo_feeder_gateway_config::config::FeederGatewayConfig;

pub struct FeederGateway {
    pub config: FeederGatewayConfig,
}

impl FeederGateway {
    pub fn new(config: FeederGatewayConfig) -> Self {
        Self { config }
    }
}

pub fn create_feeder_gateway(config: FeederGatewayConfig) -> FeederGateway {
    FeederGateway::new(config)
}
```
`lib.rs`: add `pub mod feeder_gateway;`. Commit: `apollo_feeder_gateway: add FeederGateway component struct`

## PR5 — `apollo_feeder_gateway: add run error and ComponentStarter`
`Cargo.toml`: add `thiserror.workspace = true`.
Create `crates/apollo_feeder_gateway/src/errors.rs`:
```rust
use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FeederGatewayRunError {
    #[error(transparent)]
    ServerStartupError(#[from] io::Error),
}
```
Append to `feeder_gateway.rs`:
```rust
use apollo_infra::component_definitions::ComponentStarter;
use apollo_infra_utils::type_name::short_type_name;
use async_trait::async_trait;
use tracing::info;

use crate::errors::FeederGatewayRunError;

impl FeederGateway {
    pub async fn run(&mut self) -> Result<(), FeederGatewayRunError> {
        info!("FeederGateway run starting.");
        Ok(())
    }
}

#[async_trait]
impl ComponentStarter for FeederGateway {
    async fn start(&mut self) {
        info!("Starting component {}.", short_type_name::<Self>());
        self.run().await.unwrap_or_else(|e| panic!("Failed to start FeederGateway: {e:?}"))
    }
}
```
`lib.rs`: add `pub mod errors;`. Commit: `apollo_feeder_gateway: add run error and ComponentStarter`

## PR6 — `apollo_feeder_gateway: add WrapperServer type alias`
Create `crates/apollo_feeder_gateway/src/communication.rs`:
```rust
use apollo_infra::component_server::WrapperServer;

use crate::feeder_gateway::FeederGateway as FeederGatewayComponent;

pub type FeederGateway = WrapperServer<FeederGatewayComponent>;
```
`lib.rs`: add `pub mod communication;`. Commit: `apollo_feeder_gateway: add WrapperServer type alias`

## PR7 — `apollo_feeder_gateway_config: add bind ip and port`
Replace `config.rs` body (fields needed by PR8):
```rust
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};

use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use validator::Validate;

const FEEDER_GATEWAY_PORT: u16 = 8082; // configurable; intentionally NOT legacy 9713.

#[derive(Clone, Debug, Serialize, Deserialize, Validate, PartialEq)]
pub struct FeederGatewayConfig {
    pub ip: IpAddr,
    pub port: u16,
}

impl Default for FeederGatewayConfig {
    fn default() -> Self {
        Self { ip: IpAddr::from(Ipv4Addr::UNSPECIFIED), port: FEEDER_GATEWAY_PORT }
    }
}

impl SerializeConfig for FeederGatewayConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param("ip", &self.ip.to_string(), "The feeder gateway ip.", ParamPrivacyInput::Public),
            ser_param("port", &self.port, "The feeder gateway port.", ParamPrivacyInput::Public),
        ])
    }
}

impl FeederGatewayConfig {
    pub fn ip_and_port(&self) -> (IpAddr, u16) {
        (self.ip, self.port)
    }
}
```
Commit: `apollo_feeder_gateway_config: add bind ip and port`

## PR8 — `apollo_feeder_gateway: serve axum app with health routes`
`Cargo.toml`: add `axum.workspace = true`, `tokio = { workspace = true, features = ["rt"] }`.
Replace `run()` + add `app()` in `feeder_gateway.rs`:
```rust
use std::net::SocketAddr;

use axum::http::StatusCode;
use axum::routing::get;
use axum::{serve, Router};
use tokio::net::TcpListener;

impl FeederGateway {
    pub async fn run(&mut self) -> Result<(), FeederGatewayRunError> {
        let (ip, port) = self.config.ip_and_port();
        let addr = SocketAddr::new(ip, port);
        let app = self.app();
        info!("FeederGateway running on {}", addr);
        let listener = TcpListener::bind(&addr).await?;
        Ok(serve(listener, app).await?)
    }

    pub fn app(&self) -> Router {
        Router::new()
            .route("/feeder_gateway/is_alive", get(|| async { (StatusCode::OK, "FeederGateway is alive") }))
            .route("/feeder_gateway/is_ready", get(|| async { (StatusCode::OK, "FeederGateway is ready") }))
    }
}
```
Add `#[cfg(test)] #[path = "feeder_gateway_test.rs"] mod feeder_gateway_test;` + a `tower::oneshot`
test that `is_alive` returns 200 (pattern: `http_server_test.rs`). Commit:
`apollo_feeder_gateway: serve axum app with health routes`

---

# PHASE C — Wire into the node

## PR9 — `apollo_node: add feeder_gateway execution-mode field`
`crates/apollo_node_config/src/component_config.rs`:
- ANCHOR `pub http_server: ActiveComponentExecutionConfig,` ADD AFTER
  `    pub feeder_gateway: ActiveComponentExecutionConfig,`
- dump ANCHOR `prepend_sub_config_name(self.http_server.dump(), "http_server"),` ADD AFTER
  `            prepend_sub_config_name(self.feeder_gateway.dump(), "feeder_gateway"),`
- disabled ANCHOR `http_server: ActiveComponentExecutionConfig::disabled(),` ADD AFTER
  `            feeder_gateway: ActiveComponentExecutionConfig::disabled(),`
- list ANCHOR `("http_server", self.http_server.is_disabled()),` ADD AFTER
  `            ("feeder_gateway", self.feeder_gateway.is_disabled()),`
REGEN: `update_apollo_node_config_schema` + `deployment_generator`. Commit:
`apollo_node: add feeder_gateway execution-mode field`

## PR10 — `apollo_node: add optional feeder_gateway_config to node config`
`apollo_node_config/Cargo.toml`: add `apollo_feeder_gateway_config.workspace = true`.
`node_config.rs`:
- import (near `apollo_http_server_config::config`): `use apollo_feeder_gateway_config::config::FeederGatewayConfig;`
- ANCHOR `pub http_server_config: Option<HttpServerConfig>,` ADD AFTER
  `    pub feeder_gateway_config: Option<FeederGatewayConfig>,`
- dump ANCHOR `ser_optional_sub_config(&self.http_server_config, "http_server_config"),` ADD AFTER
  `            ser_optional_sub_config(&self.feeder_gateway_config, "feeder_gateway_config"),`
- default ANCHOR `http_server_config: Some(HttpServerConfig::default()),` ADD AFTER
  `            feeder_gateway_config: Some(FeederGatewayConfig::default()),`
- validate ANCHOR `validate_component_config_is_set_iff_running_locally!(http_server, http_server_config);`
  ADD AFTER `        validate_component_config_is_set_iff_running_locally!(feeder_gateway, feeder_gateway_config);`
REGEN: `update_apollo_node_config_schema` + `deployment_generator`. Commit:
`apollo_node: add optional feeder_gateway_config to node config`

## PR11 — `apollo_node: construct feeder gateway component`
`apollo_node/Cargo.toml`: add `apollo_feeder_gateway.workspace = true`.
`components.rs`:
- import (near http_server import L17): `use apollo_feeder_gateway::feeder_gateway::{create_feeder_gateway, FeederGateway};`
- struct ANCHOR `pub http_server: Option<HttpServer>,` ADD AFTER `    pub feeder_gateway: Option<FeederGateway>,`
- after the `http_server` match block, ADD:
```rust
    let feeder_gateway = match config.components.feeder_gateway.execution_mode {
        ActiveComponentExecutionMode::Enabled => {
            let feeder_gateway_config = config
                .feeder_gateway_config
                .as_ref()
                .expect("Feeder gateway config should be set");
            Some(create_feeder_gateway(feeder_gateway_config.clone()))
        }
        ActiveComponentExecutionMode::Disabled => None,
    };
```
- return literal ANCHOR `http_server,` ADD AFTER `        feeder_gateway,`
Commit: `apollo_node: construct feeder gateway component`

## PR12 — `apollo_node: run feeder gateway wrapper server`
`servers.rs`:
- import (near L20): `use apollo_feeder_gateway::communication::FeederGateway;`
- struct ANCHOR `pub(crate) http_server: Option<Box<HttpServer>>,` ADD AFTER
  `    pub(crate) feeder_gateway: Option<Box<FeederGateway>>,`
- after the `http_server` wrapper block, ADD:
```rust
    let feeder_gateway = create_wrapper_server!(
        &config.components.feeder_gateway.execution_mode,
        components.feeder_gateway
    );
```
- `WrapperServers { ... }` literal ANCHOR `http_server,` ADD AFTER `        feeder_gateway,`
- run list ANCHOR `server_future_and_label(self.http_server, "Http"),` ADD AFTER
  `            server_future_and_label(self.feeder_gateway, "Feeder Gateway"),`
Commit: `apollo_node: run feeder gateway wrapper server`
> Node now boots FG (disabled by default), serving health.

---

# PHASE D — Cross-cutting prerequisites

## PR13 — `apollo_feeder_gateway: add request metric`
`Cargo.toml`: add `apollo_metrics.workspace = true`, `apollo_proc_macros.workspace = true`;
dev `apollo_metrics = { workspace = true, features = ["testing"] }`, `metrics`,
`metrics-exporter-prometheus`. Create `metrics.rs`:
```rust
use apollo_metrics::define_metrics;
use tracing::info;

#[cfg(test)]
#[path = "metrics_test.rs"]
pub mod metrics_test;

define_metrics!(
    FeederGateway => {
        MetricCounter { FEEDER_GATEWAY_REQUESTS_TOTAL, "feeder_gateway_requests_total", "Total feeder gateway requests", init = 0 },
    },
);

pub(crate) fn init_metrics() {
    info!("Initializing FeederGateway metrics");
    FEEDER_GATEWAY_REQUESTS_TOTAL.register();
}
```
`lib.rs`: `pub mod metrics;`. Call `crate::metrics::init_metrics();` at the top of `run()`.
`metrics_test.rs`: mirror `http_server/src/metrics_test.rs`. Commit:
`apollo_feeder_gateway: add request metric`

## PR14 — `apollo_dashboard: add feeder gateway dashboard row`
`dashboard_definitions.rs`: `use apollo_feeder_gateway::metrics::{FEEDER_GATEWAY_REQUESTS_TOTAL, ...};`
add a row in `get_apollo_dashboard()` (mirror existing component rows); extend the chain in
`metric_definitions_test.rs`. REGEN: `sequencer_dashboard_generator` (commit
`resources/dev_grafana.json`). Commit: `apollo_dashboard: add feeder gateway dashboard row`

## PR15 — `apollo_feeder_gateway: add legacy error envelope`
`Cargo.toml`: add `apollo_gateway_types.workspace = true`, `axum`, `http`, `serde_json`, `regex`.
In `errors.rs` add (copy `serialize_error` + regexes from `http_server/src/errors.rs`):
```rust
use apollo_gateway_types::deprecated_gateway_error::{KnownStarknetErrorCode, StarknetError, StarknetErrorCode};
use axum::response::{IntoResponse, Response};
use http::StatusCode;
use regex::Regex;

#[derive(Debug, thiserror::Error)]
pub enum FeederGatewayError {
    #[error("Block not found")]
    BlockNotFound,
    #[error("Transaction hash not found")]
    TransactionNotFound,
    #[error("Malformed request: {0}")]
    MalformedRequest(String),
    #[error("Internal error")]
    Internal,   // m8: source logged, NEVER serialized to the client
}

impl IntoResponse for FeederGatewayError {
    fn into_response(self) -> Response {
        let (code, sn) = match self {
            FeederGatewayError::BlockNotFound => (
                StatusCode::BAD_REQUEST,
                StarknetError { code: StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::BlockNotFound), message: "Block not found".into() },
            ),
            FeederGatewayError::TransactionNotFound => (
                StatusCode::BAD_REQUEST,
                StarknetError { code: StarknetErrorCode::UnknownErrorCode("StarknetErrorCode.TRANSACTION_NOT_FOUND".into()), message: "Transaction not found".into() },
            ),
            FeederGatewayError::MalformedRequest(m) => (
                StatusCode::BAD_REQUEST,
                StarknetError { code: StarknetErrorCode::KnownErrorCode(KnownStarknetErrorCode::MalformedRequest), message: m },
            ),
            // m8: generic 500, no internal detail leaked (the source was already logged at the
            // construction site via tracing::error!(error=?e); `internal()` in PR18 does this).
            FeederGatewayError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                StarknetError { code: StarknetErrorCode::UnknownErrorCode("StarknetErrorCode.INTERNAL_ERROR".into()), message: "Internal error".into() },
            ),
        };
        serialize_error(code, &sn)
    }
}

fn serialize_error(code: StatusCode, error: &StarknetError) -> Response {
    // copy sanitization from http_server/src/errors.rs::serialize_error
    let quote_re = Regex::new(r#"["`]"#).unwrap();
    let sanitize_re = Regex::new(r#"[^a-zA-Z0-9 :.,\[\]\(\)\{\}'_]"#).unwrap();
    let message = sanitize_re
        .replace_all(&quote_re.replace_all(&error.message, "'"), " ")
        .to_string();
    let sanitized = StarknetError { code: error.code.clone(), message };
    // M1: error bodies MUST also be byte-parity (spaced) → to_python_json, NOT serde_json::to_vec.
    let body = to_python_json(&sanitized).expect("serializable");
    ([(http::header::CONTENT_TYPE, "application/json")], code, body).into_response()
}
```
Verify the exact `KnownStarknetErrorCode` variant names against `apollo_gateway_types`. Unit test:
`FeederGatewayError::BlockNotFound.into_response()` body byte-matches a captured Python error fixture
(spaced). `Internal` logs the source via `tracing::error!` and returns a generic 500 (no leak — m8).
Commit: `apollo_feeder_gateway: add legacy error envelope`
> Depends on `to_python_json` (PR-parity-3) — sequence PR15 after PR-parity-3, or stub then switch.

> **ORDERING (POC-first):** PR13 (metric) + PR14 (dashboard) are observability, NOT
> POC-critical — do them AFTER the POC milestone (after E1 + the perf test). The POC-critical
> prerequisites are PR15 (error envelope) and PR16–PR20 below (the `ChainDataReader` stack).
> The POC milestone = E0 + E1 served via the co-located backend (see the marker after E1).

## PR16 — `apollo_feeder_gateway: add ChainDataReader trait + AppState`
The data seam every handler depends on (per Reference D.1). `Cargo.toml`: add
`starknet_api.workspace = true`, `apollo_starknet_client.workspace = true`; dev-dep `mockall`,
add `mockall` to `testing`. Create `reader.rs`:
```rust
use std::sync::Arc;

use async_trait::async_trait;
use starknet_api::block::{BlockHeader, BlockNumber};

use crate::errors::FeederGatewayError;

pub type FgResult<T> = Result<T, FeederGatewayError>;

/// The FG's read backend. Impls: `ColocatedStorageReader` (direct StorageReader, via bounded ReadExecutor per D.11)
/// and `RemoteChainDataReader` (SharedStateSyncClient). One method per FG read primitive; widened
/// one endpoint at a time.
#[cfg_attr(any(test, feature = "testing"), mockall::automock)]
#[async_trait]
pub trait ChainDataReader: Send + Sync + 'static {
    async fn latest_block_header(&self) -> FgResult<Option<BlockHeader>>;
    // + get_block_header, get_block_transactions_with_outputs, get_state_diff, ... (added per endpoint)
}

#[derive(Clone)]
pub struct AppState {
    pub reader: Arc<dyn ChainDataReader>,
    pub config: apollo_feeder_gateway_config::config::FeederGatewayConfig,
}
```
In `feeder_gateway.rs`, give `FeederGateway` an `app_state: AppState`, build it in `new`, and wire
`.layer(Extension(self.app_state.clone()))` into `app()` (mirror `http_server.rs`). Unit-test a
handler against `MockChainDataReader`. Commit:
`apollo_feeder_gateway: add ChainDataReader trait and AppState`

## PR17 — `apollo_state_sync: expose storage_reader from create_state_sync_and_runner`
Pure refactor, no behavior change (Reference D.4). `create_state_sync_and_runner`
(`apollo_state_sync/src/lib.rs:38`) already gets `storage_reader` from `StateSyncRunner::new`
(`runner/mod.rs:190` → `(Self, StorageReader)`) but discards it. Change its return to
`(StateSync, StateSyncRunner, StorageReader)` and update the single call site in
`apollo_node/src/components.rs:565`. **In THIS PR the new element has no consumer yet (PR18/PR19 are
the first readers), and CI clippy is deny-level → bind it `_storage_reader` here, then rename to
`storage_reader` in PR19.** Verified (2026-06-02): exactly one call site; `StateSyncRunner::new`
already returns the reader and `create_state_sync_and_runner` already binds it only to feed
`StateSync::new`, so this is a pure forward of an existing value (no behavior change); state-sync unit
tests build `StateSync` via a struct literal, so the arity change breaks no tests. Commit:
`apollo_state_sync: return storage_reader from create_state_sync_and_runner`

## PR17b — `apollo_feeder_gateway: add bounded ReadExecutor` (per D.11)
Create `reader/executor.rs`: a bounded blocking-read executor (per D.11). **NOT** a tokio runtime
with capped `max_blocking_threads` (its queue is unbounded → no backpressure). Use a **fixed
`std::thread` worker pool fed by a BOUNDED channel** (mirror `blockifier/src/concurrency/worker_pool.rs`):
```rust
pub struct ReadExecutor { job_tx: tokio::sync::mpsc::Sender<Job> /* N workers drain a bounded chan */ }
impl ReadExecutor {
    pub fn new(read_pool_size: usize, channel_capacity: usize) -> Self {
        // spawn read_pool_size std::threads, each looping on a bounded job receiver
    }
    pub async fn run<F, T>(&self, f: F) -> FgResult<T>
    where F: FnOnce() -> T + Send + 'static, T: Send + 'static {
        // job_tx.send((boxed f, oneshot tx)).await  -> worker runs f() -> reply via oneshot;
        // bounded channel applies backpressure when full (NOT unbounded queueing)
    }
}
```
`ReadExecutor::new(read_pool_size, channel_capacity)` takes its sizing as params (default
`read_pool_size = 1.5 × std::thread::available_parallelism()` — do NOT add a `num_cpus` crate). The
**config fields are NOT added here** — they're added in PR19 where the executor is first constructed
(config-when-needed). **Backpressure semantics (M5): `run()` AWAITS on a full channel (never rejects)**
— a full read queue applies natural backpressure, no `ServiceOverloaded` error. Unit test: spawn
concurrency > `read_pool_size`, assert in-flight never exceeds the bound AND all calls eventually
complete (no rejection). Commit: `apollo_feeder_gateway: add bounded ReadExecutor`

## PR18 — `apollo_feeder_gateway: add ColocatedStorageReader backend`
`Cargo.toml`: add `apollo_storage.workspace = true`, `tokio` (already), `starknet_api`. Create
`reader/colocated.rs`. Hold `Arc<ReadExecutor>` and route EVERY read through it (NOT raw
`spawn_blocking` — D.11):
```rust
use std::sync::Arc;
use apollo_storage::StorageReader;
use apollo_storage::header::HeaderStorageReader;
use async_trait::async_trait;
use starknet_api::block::BlockHeader;

use crate::reader::{ChainDataReader, FgResult};
use crate::reader::executor::ReadExecutor;

pub struct ColocatedStorageReader {
    storage_reader: StorageReader,
    executor: Arc<ReadExecutor>,   // bounded ≈1.5× cores (D.11)
}

impl ColocatedStorageReader {
    pub fn new(storage_reader: StorageReader, executor: Arc<ReadExecutor>) -> Self {
        Self { storage_reader, executor }
    }
}

#[async_trait]
impl ChainDataReader for ColocatedStorageReader {
    async fn latest_block_header(&self) -> FgResult<Option<BlockHeader>> {
        let reader = self.storage_reader.clone();          // Arc<Environment> clone, ~free
        self.executor.run(move || {                        // bounded parallelism, reactor free (D.2/D.11)
            let txn = reader.begin_ro_txn().map_err(internal)?;
            let marker = txn.get_header_marker().map_err(internal)?;
            let Some(latest) = marker.prev() else { return Ok(None) };
            txn.get_block_header(latest).map_err(internal)
        }).await?
    }
}

fn internal<E: std::fmt::Display>(e: E) -> crate::errors::FeederGatewayError {
    // m8: log the source HERE (the only place it's seen) — `FeederGatewayError::Internal` deliberately
    // carries no detail so nothing leaks to the client. Do NOT discard `e`.
    tracing::error!(error = %e, "feeder gateway internal read error");
    crate::errors::FeederGatewayError::Internal
}
```
(Mirrors `apollo_rpc` direct-StorageReader access, but through the bounded `ReadExecutor` instead
of raw `spawn_blocking`.) Add a `FeederGatewayError::Internal` variant. Construct the
`ReadExecutor` once in `create_feeder_gateway` (PR19) and share it. Unit test via
`get_test_storage()`. Commit: `apollo_feeder_gateway: add ColocatedStorageReader backend`

## PR19 — `apollo_node: select FG read backend by topology`
`apollo_feeder_gateway_config`: add `read_backend: ReadBackend { Colocated, Remote }` field
(default `Colocated`). **Also add the read-pool config fields HERE (moved from PR17b per
config-when-needed — this is their first reader):** `read_pool_size: Option<usize>` (default
`1.5 × available_parallelism`) and `read_channel_capacity: Option<usize>` to `FeederGatewayConfig`.
In `components.rs` FG block, construct the `ReadExecutor` ONCE and thread it into the colocated
backend (B1 — PR18's `ColocatedStorageReader::new` takes `(StorageReader, Arc<ReadExecutor>)`):
```rust
let executor = Arc::new(ReadExecutor::new(
    feeder_gateway_config.read_pool_size(),       // default 1.5 × available_parallelism
    feeder_gateway_config.read_channel_capacity(),
));
let reader: Arc<dyn ChainDataReader> = match feeder_gateway_config.read_backend {
    ReadBackend::Colocated => Arc::new(ColocatedStorageReader::new(
        storage_reader.clone(),   // from PR17; requires state-sync local in this process
        executor.clone(),         // bounded read executor (PR17b)
    )),
    ReadBackend::Remote => Arc::new(RemoteChainDataReader::new(
        clients.get_state_sync_shared_client().expect("state sync client"),
    )),
};
Some(create_feeder_gateway(feeder_gateway_config.clone(), reader))
```
Thread `reader` into `create_feeder_gateway`/`FeederGateway::new`. **REGEN (config fields added
here): `update_apollo_node_config_schema` + `deployment_generator`, commit the regenerated JSON.**
Commit: `apollo_node: select feeder gateway read backend by topology`

## PR20 — `apollo_feeder_gateway: add RemoteChainDataReader backend`
For different-pod/node (Reference D.1 impl 2). `Cargo.toml`: add
`apollo_state_sync_types.workspace = true`. Create `reader/remote.rs`:
```rust
use apollo_state_sync_types::communication::SharedStateSyncClient;
// impl ChainDataReader by delegating to the StateSyncClient methods (existing or added per
// Reference C). Each method maps StateSyncClientError -> FeederGatewayError.
pub struct RemoteChainDataReader { client: SharedStateSyncClient }
```
Implements the same trait methods as `ColocatedStorageReader`, one per endpoint as they land
(reads not yet on `StateSyncClient` get a Reference-C extension PR). Commit:
`apollo_feeder_gateway: add RemoteChainDataReader backend`

> **Storage-side read bounding (D.11) — separate PR `apollo_state_sync: bound read handlers`.**
> The remote path concentrates ALL reads on the single state-sync process, whose read handlers
> today run `begin_ro_txn` **inline**, gated only by `max_concurrency=128` (unrelated to cores).
> Route those reads through a bounded `ReadExecutor` sized ≈ that node's cores (the real read
> bound), so remote-mode reads parallelize correctly and don't block the state-sync reactor.

> **Optional (own-pod topology) PR `apollo_feeder_gateway: cheap storage↔FG codec`.** The remote
> path double-serializes (storage JSON-encodes `SyncBlock` → FG decodes → FG re-encodes to FG-JSON,
> ~6–9 ms extra/large block). A cheap internal codec (bincode/protobuf) on the state-sync↔FG hop
> keeps the heavy client-facing FG-JSON serialization on the scalable FG replicas and the storage
> node on a cheap encode. Gated on the remote topology; not needed co-located.

---

# PHASE D-parity — JSON byte/order parity infrastructure (per Reference B)

Backwards-compat requires byte/order-exact JSON. These land BEFORE any JSON-emitting handler
(E0). PR-parity-1/2 are in `apollo_starknet_client` (scope `apollo_starknet_client`), -3/-Felt in
the FG crate.

## PR-parity-1 — `apollo_starknet_client: reorder reader-object fields to Python order`
Reorder field DECLARATIONS (no type/logic changes — "moves only", trivially reviewable) so serde's
declaration-order output matches the Python FG key order. **The order reference is
`resources/reader/block_post_0_14_3.json` (and state-update fixtures).** NOTE (verified 2026-06-02):
that fixture is **indented/pretty-printed**, so it is the source of truth for KEY ORDER ONLY — it is
NOT the byte-parity fixture (byte parity uses single-line spaced captures under `resources/parity/`,
Reference B). Confirm its order against a freshly-captured Python `json.dumps` before relying on it.
- **`BlockPostV0_13_1` (block.rs) — the current struct declaration order does NOT match the fixture
  (verified 2026-06-02).** Reorder the fields to EXACTLY this top-level order (from
  `block_post_0_14_3.json`):
  `block_hash, parent_block_hash, block_number, state_root, transaction_commitment, event_commitment,
  receipt_commitment, state_diff_commitment, state_diff_length, status, l1_da_mode, l1_gas_price,
  l1_data_gas_price, l2_gas_price, transactions, timestamp, sequencer_address, transaction_receipts,
  starknet_version, l2_gas_consumed, next_l2_gas_price, fee_proposal_fri`.
- `IntermediateInvokeTransaction` + `TransactionReceipt` (transaction.rs), `StateDiff` (state.rs) —
  per the field-order delta (e.g. `transaction_hash`/`version` to the front of invoke;
  `execution_status` first in receipt; `nonces` first in `StateDiff` — verify each against the
  fixtures, do not assume). `ExecutionResources` and `StateUpdate` are claimed to already match —
  VERIFY against a fixture, then leave untouched if so.
- Safe: these structs are deserialize-only on the client side, not hashed/signed/snapshotted; the
  existing tests are object-equality/round-trip (order-agnostic) and must stay green with ZERO edits
  (if one fails it was secretly asserting order → convert it to the byte-fixture strategy, don't
  revert). Add a `// PARITY: field order must match Python FG; do not reorder (Reference B)` comment
  atop each struct. Commit: `apollo_starknet_client: reorder reader-object fields to Python order`

## PR-parity-2 — `apollo_starknet_client: make map fields deterministic (HashMap→IndexMap)`
`ExecutionResources.builtin_instance_counter: HashMap<Builtin,u64>` (transaction.rs:660) and
`ContractClass.entry_points_by_type: HashMap<..>` (state.rs:66) serialize in **random** order →
parity bug. Change both to `IndexMap` (+ mirror in `test_utils.rs:133/167`). `storage_diffs`/`nonces`
are already `IndexMap` — keep. Document (code comment citing the Python construction site) that the
FG producer must INSERT in Python's order (Python does not sort these — we must not either). Test:
build an `ExecutionResources` with ≥2 builtins; assert stable, Python-matching key order. Commit:
`apollo_starknet_client: make map fields deterministic`

## PR-parity-3 — `apollo_feeder_gateway: Python-style spaced JSON serializer`
serde default is COMPACT; Python `json.dumps` default is SPACED (`", "`/`": "`, single line). Create
`serialization.rs` with a `serde_json::ser::Formatter` emitting `", "` between elements and `": "`
after keys, no indent/newline, plus `pub fn to_python_json<T: Serialize>(v: &T) -> FgResult<String>`.
Test: `to_python_json(&json!({"a":1,"b":[1,2]}))` == `{"a": 1, "b": [1, 2]}` exact bytes; `[]`/`{}`
edge cases. ALL FG handlers serialize via this (never axum `Json<T>`/`to_string`).
**`ensure_ascii` — IMPLEMENT IT HERE, do not defer (verified 2026-06-02):** Python `json.dumps`
defaults `ensure_ascii=True` (non-ASCII → `\uXXXX`); serde emits raw UTF-8. The `get_block` path CAN
carry free text — `revert_error` is an `Option<String>` (transaction.rs:786) on reverted-tx receipts —
so add a `write_string_fragment` branch to the `Formatter` that escapes bytes > 0x7F as `\uXXXX`
(~5-10 lines, same file). Cheap insurance; removes a latent POC-path parity break. Test with a
non-ASCII string. Commit: `apollo_feeder_gateway: Python-style spaced JSON serializer`
> **Ground-truth fixtures need NO live Python FG / SSH / bazel (verified 2026-06-02):** the committed
> indented `crates/apollo_starknet_client/resources/reader/block_post_0_14_3.json` round-trips
> byte-stably and has zero floats, so `python -c "import json;print(json.dumps(json.load(open(F))))"`
> reproduces the exact default-separator single-line bytes. Each `(c)` PR generates its
> `resources/parity/<endpoint>.json` from the committed fixture this way (for `get_block` it already
> exists), and the A1/B4 round-trip lock (deserialize → `to_python_json` → assert original bytes) needs
> no capture at all. A real captured Python response is still worth obtaining eventually to confirm the
> committed fixture itself faithfully matches today's Python wire output — but it does NOT gate the POC.

## PR-parity-Felt — `apollo_feeder_gateway: lock Felt JSON format`
One tiny test asserting `serde_json::to_string(&Felt)` yields lowercase `0x`, no leading zeros
(`"0x0"`, `"0xf"`). Pins the assumption the whole parity effort rests on; fails loudly if
`starknet_api` ever changes Felt Serialize. Commit: `apollo_feeder_gateway: lock Felt JSON format`

> **PR-parity-4 is folded into Phase E:** each endpoint's (c) handler PR serializes via
> `to_python_json` and adds a **byte-equality** test vs a captured Python fixture
> (`resources/parity/<endpoint>.json`) per Reference B — that test is what actually proves
> parity-1/2/3 worked for that response. Optional fields use `#[serde(skip_serializing_if=...)]`.

---

# PHASE E — Read endpoints

Per endpoint, in order: **(b) conversion** — `starknet_api -> FG` converter in
`crates/apollo_feeder_gateway/src/conversions/<family>.rs` (`pub(crate) fn`) + Reference-B test;
**(local) ColocatedStorageReader method** — add the trait method + a `spawn_blocking + begin_ro_txn`
impl that reads storage and applies the converter; **(remote) RemoteChainDataReader method** —
implement the same trait method via `StateSyncClient` (add a Reference-C extension PR first if the
read is MISSING per Reference A); **(c) handler** — axum route calling `state.reader.<method>()`.
Each of these is its own small PR. The conversion code blocks below are unchanged; they now feed
the `ColocatedStorageReader` method (co-located, the POC/primary path) and are reused by the remote
path.

> **SERIALIZATION (Reference B):** handlers must NOT return axum `Json<T>` (compact, breaks byte
> parity). Build the FG wire struct, serialize with `to_python_json` (PR-parity-3), and return a
> raw `Response` with `content-type: application/json` and those exact bytes. Each (c) PR adds a
> **byte-equality** test vs `resources/parity/<endpoint>.json`. Helper:
> ```rust
> fn fg_json<T: Serialize>(v: &T) -> Response {
>     ([(header::CONTENT_TYPE, "application/json")], to_python_json(v)).into_response()
> }
> ```

Handler param parsing helper (add once, in E0):
```rust
// block id from query: ?blockNumber=<u64> | ?blockHash=0x.. | latest | pending
fn parse_block_id(params: &HashMap<String, String>) -> Result<BlockId, FeederGatewayError> { /* never panic */ }
```

### E0. get_contract_addresses (pipeline-prover — FIRST)
(c) PR `apollo_feeder_gateway: serve get_contract_addresses`. Add config (convention 4):
```rust
// apollo_feeder_gateway_config/src/config.rs
use starknet_api::core::ContractAddress; // add starknet_api dep to the config crate
#[derive(Clone, Debug, Default, Serialize, Deserialize, Validate, PartialEq)]
pub struct FeederGatewayContractAddresses {
    #[serde(rename = "Starknet")]
    pub starknet: ContractAddress,
    #[serde(rename = "GpsStatementVerifier")]
    pub gps_statement_verifier: ContractAddress,
}
// add to FeederGatewayConfig: pub contract_addresses: FeederGatewayContractAddresses,
// add its dump() entries.
```
Handler + route:
```rust
async fn get_contract_addresses(Extension(state): Extension<AppState>) -> Response {
    fg_json(&state.config.contract_addresses)   // spaced serializer, NOT Json<T> (Reference B)
}
// .route("/feeder_gateway/get_contract_addresses", get(get_contract_addresses))
```
Test (Reference B) vs a hand-written `contract_addresses.json`.

### E1. get_block (anchor — uses block_post_*.json)
Target: `BlockPostV0_13_1` (`block.rs:46-96`). Invert
`to_starknet_api_block_and_version()` (`block.rs:309-363`).

(a) PR `apollo_state_sync: expose block transactions with outputs` — Reference C with:
```rust
GetBlockTransactionsWithOutputs(BlockNumber),
// response:
GetBlockTransactionsWithOutputs(StateSyncResult<Vec<(starknet_api::transaction::Transaction, starknet_api::transaction::TransactionOutput, starknet_api::transaction::TransactionHash)>>),
// impl:
async fn get_block_transactions_with_outputs(&self, block_number: BlockNumber)
    -> StateSyncResult<Vec<(Transaction, TransactionOutput, TransactionHash)>> {
    let txn = self.storage_reader.clone().begin_ro_txn()?;
    let txs = txn.get_block_transactions(block_number)?.ok_or(StateSyncError::BlockNotFound(block_number))?;
    let outs = txn.get_block_transaction_outputs(block_number)?.ok_or(StateSyncError::BlockNotFound(block_number))?;
    let hashes = txn.get_block_transaction_hashes(block_number)?.ok_or(StateSyncError::BlockNotFound(block_number))?;
    Ok(itertools::izip!(txs, outs, hashes).collect())
}
```
Also (a) `apollo_state_sync: expose get_block_signature`, `get_block_number_by_hash` (Reference C),
needed by E1.c/E8/E9.

(b) PR `apollo_feeder_gateway: add block-header conversion`. `conversions/block.rs`:
```rust
use apollo_starknet_client::reader::objects::block::{BlockPostV0_13_1, BlockStatus};
use apollo_starknet_client::reader::objects::transaction::{Transaction, TransactionReceipt};
use starknet_api::block::BlockHeader;

pub(crate) fn block_to_fg(
    header: &BlockHeader,
    status: BlockStatus,
    transactions: Vec<Transaction>,
    transaction_receipts: Vec<TransactionReceipt>,
) -> BlockPostV0_13_1 {
    let h = &header.block_header_without_hash;
    BlockPostV0_13_1 {
        block_hash: header.block_hash,
        block_number: h.block_number,
        parent_block_hash: h.parent_hash,
        sequencer_address: h.sequencer,
        state_root: h.state_root,
        status,
        timestamp: h.timestamp,
        transactions,
        transaction_receipts,
        starknet_version: h.starknet_version,
        l1_da_mode: h.l1_da_mode,
        l1_gas_price: h.l1_gas_price,
        l1_data_gas_price: h.l1_data_gas_price,
        l2_gas_price: h.l2_gas_price,
        // OPTION MISMATCH (verified 2026-06-02): `starknet_api::block::BlockHeader` has these as
        // `Option<_>` (skip_serializing) but FG `BlockPostV0_13_1` has them NON-Option (block.rs:73-74).
        // So they MUST be unwrapped here — assigning the `Option<_>` source directly does NOT compile.
        // Finalized blocks always carry them, so the default branch is effectively unreachable.
        transaction_commitment: header.transaction_commitment.unwrap_or_default(),
        event_commitment: header.event_commitment.unwrap_or_default(),
        state_diff_commitment: header.state_diff_commitment,
        receipt_commitment: header.receipt_commitment,
        state_diff_length: header.state_diff_length,
        l2_gas_consumed: h.l2_gas_consumed,
        next_l2_gas_price: h.next_l2_gas_price,
        fee_proposal_fri: h.fee_proposal_fri,
    }
    // verify each field's exact type/Option-ness against block.rs:46-96 & :309-363.
}
```
+ Reference-B test building `header` from the fixture and asserting header subset.

(b) PR `apollo_feeder_gateway: add block-status mapping`:
```rust
pub(crate) fn block_status_to_fg(/* committed?, base_layer_marker, block_number */) -> BlockStatus {
    // ACCEPTED_ON_L1 if block_number <= base_layer_marker else ACCEPTED_ON_L2; Pending for pending.
}
```

(b) FIVE PRs — `apollo_feeder_gateway: add <invoke|declare|deploy_account|deploy|l1_handler>-tx conversion`.
`conversions/transaction.rs`. Invert the `TryFrom`/`From` at `transaction.rs:557-652` (invoke),
`:256-347` (declare), `:426-496` (deploy_account), `:361` (deploy), `:157` (l1_handler). Invoke
example (full):
```rust
use apollo_starknet_client::reader::objects::transaction::{IntermediateInvokeTransaction, Transaction as FgTx};
use starknet_api::transaction::{InvokeTransaction, Transaction as ApiTx};

pub(crate) fn invoke_to_fg(tx: &InvokeTransaction, tx_hash: TransactionHash) -> FgTx {
    let i = match tx {
        InvokeTransaction::V0(t) => IntermediateInvokeTransaction {
            max_fee: Some(t.max_fee), version: TransactionVersion::ZERO,
            signature: t.signature.clone(), entry_point_selector: Some(t.entry_point_selector),
            calldata: t.calldata.clone(), sender_address: t.contract_address,
            nonce: None, resource_bounds: None, tip: None,
            nonce_data_availability_mode: None, fee_data_availability_mode: None,
            paymaster_data: None, account_deployment_data: None, proof_facts: None, transaction_hash: tx_hash,
        },
        InvokeTransaction::V1(t) => IntermediateInvokeTransaction {
            max_fee: Some(t.max_fee), version: TransactionVersion::ONE, signature: t.signature.clone(),
            entry_point_selector: None, calldata: t.calldata.clone(), sender_address: t.sender_address,
            nonce: Some(t.nonce), resource_bounds: None, tip: None,
            nonce_data_availability_mode: None, fee_data_availability_mode: None,
            paymaster_data: None, account_deployment_data: None, proof_facts: None, transaction_hash: tx_hash,
        },
        InvokeTransaction::V3(t) => IntermediateInvokeTransaction {
            max_fee: None, version: TransactionVersion::THREE, signature: t.signature.clone(),
            entry_point_selector: None, calldata: t.calldata.clone(), sender_address: t.sender_address,
            nonce: Some(t.nonce), resource_bounds: Some(t.resource_bounds), tip: Some(t.tip),
            nonce_data_availability_mode: Some(t.nonce_data_availability_mode.into()),
            fee_data_availability_mode: Some(t.fee_data_availability_mode.into()),
            paymaster_data: Some(t.paymaster_data.clone()),
            account_deployment_data: Some(t.account_deployment_data.clone()),
            proof_facts: Some(t.proof_facts.clone()), transaction_hash: tx_hash,
        },
    };
    FgTx::Invoke(i)
}

pub(crate) fn transaction_to_fg(tx: &ApiTx, tx_hash: TransactionHash) -> FgTx {
    match tx {
        ApiTx::Invoke(t) => invoke_to_fg(t, tx_hash),
        ApiTx::Declare(t) => declare_to_fg(t, tx_hash),
        ApiTx::DeployAccount(t) => deploy_account_to_fg(t, tx_hash),
        ApiTx::Deploy(t) => deploy_to_fg(t, tx_hash),
        ApiTx::L1Handler(t) => l1_handler_to_fg(t, tx_hash),
    }
}
```
Declare/deploy_account follow the same V0V1/V2/V3 shape (set `compiled_class_hash` for V2/V3,
`class_hash`, `sender_address`). Deploy/l1_handler are infallible field copies (see `:361`/`:157`).
Each PR: the one function + a Reference-B test using a tx pulled from the block fixture.

(b) PR `apollo_feeder_gateway: add exec-resources conversion`:
```rust
pub(crate) fn exec_resources_to_fg(r: &starknet_api::execution_resources::ExecutionResources)
    -> apollo_starknet_client::reader::objects::transaction::ExecutionResources {
    // n_steps: r.steps, n_memory_holes: r.memory_holes,  (field names differ: steps/memory_holes
    //   on starknet_api ExecutionResources vs n_steps/n_memory_holes on FG — execution_resources.rs:232).
    // builtin_instance_counter: map starknet_api::Builtin -> FG Builtin variant-by-variant by ENUM
    //   variant (serde strings differ: "range_check_builtin_applications" vs "range_check_builtin";
    //   see transaction.rs:705-765 for the inverse). Forward maps only variants that exist on both;
    //   FG `Builtin::Output` has no starknet_api source so it is simply never produced.
    // data_availability: Some(r.da_gas_consumed.into()), total_gas_consumed: Some(r.gas_consumed.into()).
}
```

(b) PR `apollo_feeder_gateway: add receipt conversion`. Invert
`into_starknet_api_transaction_output()` (`transaction.rs:800-871`):
```rust
pub(crate) fn output_to_fg_receipt(
    output: &starknet_api::transaction::TransactionOutput,
    tx_hash: TransactionHash,
    index: TransactionOffsetInBlock,
) -> TransactionReceipt {
    let (exec_status, revert_error) = match output.execution_status() {
        TransactionExecutionStatus::Succeeded => (FgExecStatus::Succeeded, None),
        TransactionExecutionStatus::Reverted(r) => (FgExecStatus::Reverted, Some(r.revert_reason.clone())),
    };
    TransactionReceipt {
        transaction_index: index,
        transaction_hash: tx_hash,
        l1_to_l2_consumed_message: Default::default(), // set for L1Handler from the tx
        l2_to_l1_messages: output.messages_sent().iter().map(message_to_l1_to_fg).collect(),
        events: output.events().to_vec(),
        execution_resources: exec_resources_to_fg(output.execution_resources()),
        actual_fee: output.actual_fee(),
        execution_status: exec_status,
        revert_error,
    }
    // verify TransactionOutput accessor method names against starknet_api/transaction.rs.
}
```

(c) PR `apollo_feeder_gateway: serve get_block`. Widen `ChainDataReader` (both backends):
```rust
async fn block(&self, block_number: BlockNumber) -> FgResult<BlockPostV0_13_1>;
// ColocatedStorageReader::block: spawn_blocking { begin_ro_txn → get_block_header (use header.block_hash; NOT block_hash.rs::get_block_hash) +
//   get_block_transactions + get_block_transaction_outputs + status } assemble via
//   block_to_fg + per-tx + receipts. RemoteChainDataReader::block: via StateSyncClient
//   (get_block_hash + GetBlockTransactionsWithOutputs from PR E1.a) then the same converters.
```
Handler:
```rust
async fn get_block(Extension(state): Extension<AppState>, Query(p): Query<HashMap<String,String>>)
    -> Result<Response, FeederGatewayError> {
    let block_number = resolve_block_number(&state, parse_block_id(&p)?).await?;
    let block = state.reader.block(block_number).await?;
    Ok(fg_json(&block))   // spaced serializer (Reference B); byte-equality test vs parity/get_block.json
}
// .route("/feeder_gateway/get_block", get(get_block))
```
Reference-B test vs `block_post_0_14_3.json`.

(c) PR `apollo_feeder_gateway: support get_block headerOnly`. Add
`crate::objects::FeederGatewayBlockHeader` (the `BlockPostV0_13_1` fields minus the two vecs,
same names/serde); return it when `?headerOnly=true`. Test vs a header fixture.

---
## ★ POC MILESTONE — reached here
After E0 + E1(c), the FG serves **real `get_block` data over HTTP via the co-located
`ColocatedStorageReader` (parallel MDBX reads through the bounded `ReadExecutor`, D.11)** end-to-end.
This is the POC. Validate it, then run the POC perf test below BEFORE expanding to more endpoints.

## PR21 (POC perf test) — `apollo_feeder_gateway: get_block load test + runtime config`
(Unnumbered-in-spirit milestone PR; sits between PR20 and the post-POC PR13/PR14.) Implements
Reference F.1 and validates the `ReadExecutor` sizing (D.9/D.11).
- Add an integration test under `apollo_integration_tests` (or `apollo_feeder_gateway` with
  `tokio::test(flavor="multi_thread")`) that boots the FG over `get_integration_test_storage`
  seeded with N blocks, then drives `get_block` with `HttpTestClient`
  (`apollo_http_server/src/test_utils.rs:33`) in a `tokio` fan-out loop: `Arc<AtomicU64>` counters,
  `Instant` latencies, report RPS + p50/p95/p99 (`--release`).
- Confirm `worker_threads ≈ #cores` for axum; the read concurrency is the bounded `ReadExecutor`
  (PR17b), **starting from `read_pool_size = 1.5 × available_parallelism` (D.9/D.11)** — the sweep
  VALIDATES this baseline rather than picking an arbitrary `max_blocking_threads`.
- Sweep `read_pool_size` + `read_channel_capacity` + client concurrency; record the RPS knee.
  **Gate on a target RPS** before
  expanding. Also point `oha`/`wrk` at a locally-run FG for ad-hoc saturation curves.
Commit: `apollo_feeder_gateway: get_block load test and runtime config`

## PRs (post-POC observability) — PR13 metric + PR14 dashboard
Now do the deferred PR13 (request metric) and PR14 (dashboard row) from Phase D — not
POC-critical, but land them before broadening the surface so new endpoints get metered.

---
> Everything below is the **EXPAND** phase (remaining endpoints), then compute (Phase F),
> caching (Phase G, nice-to-have), deployment (Phase H), benchmarks/parity (Phase I).

### E2. get_state_update
(a) `GetStateDiff` (Reference C, shown there). (b) `conversions/state.rs` — invert
`From<ClientStateDiff>` (`apollo_rpc/src/v0_8/state.rs:53-93`):
```rust
use apollo_starknet_client::reader::objects::state::{StateDiff, StateUpdate, StorageEntry, DeployedContract, DeclaredClassHashEntry, ReplacedClass};
use indexmap::IndexMap;
pub(crate) fn thin_state_diff_to_fg(thin: &starknet_api::state::ThinStateDiff) -> StateDiff {
    StateDiff {
        storage_diffs: thin.storage_diffs.iter().map(|(a, m)|
            (*a, m.iter().map(|(k, v)| StorageEntry { key: *k, value: *v }).collect())).collect(),
        deployed_contracts: thin.deployed_contracts.iter().map(|(address, class_hash)|
            DeployedContract { address: *address, class_hash: *class_hash }).collect(),
        // SOURCE FIELD: ThinStateDiff has `class_hash_to_compiled_class_hash`, NOT `declared_classes`
        // (starknet_api/src/state.rs:68-76).
        declared_classes: thin.class_hash_to_compiled_class_hash.iter().map(|(class_hash, compiled_class_hash)|
            DeclaredClassHashEntry { class_hash: *class_hash, compiled_class_hash: *compiled_class_hash }).collect(),
        old_declared_contracts: thin.deprecated_declared_classes.clone(), // Vec<ClassHash> 1:1
        nonces: thin.nonces.clone(),
        // ⚠ PARITY GAP (verified 2026-06-02, see Open decision D4): `starknet_api::state::ThinStateDiff`
        // has NO `replaced_classes` field (only `deployed_contracts`, which merges deploys+replacements).
        // The apollo_rpc `From<ClientStateDiff> for ThinStateDiff` (v0_8/state.rs:53-92) populates
        // replaced_classes ONLY because its *source* (ClientStateDiff) carries it — that converter is a
        // DIFFERENT `ThinStateDiff` type and is NOT a usable inverse here. Emitting `vec![]`
        // unconditionally DIVERGES from Python FG (which splits deployed vs replaced) and is visible to
        // external clients. Reconstructing it requires per-address "did this address have a class at the
        // previous block?" reads. Do NOT ship `vec![]` silently — resolve D4 first.
        replaced_classes: vec![], // placeholder — see D4; parity-incorrect as written
        migrated_compiled_classes: vec![],
    }
    // verify ThinStateDiff field names (`deployed_contracts` vs `class_hash_to_compiled_class_hash`)
    // against starknet_api/state.rs; the legacy ThinStateDiff variant differs from the v0.8 one.
}
pub(crate) fn state_update_to_fg(block_hash, new_root, old_root, diff) -> StateUpdate { /* wrap */ }
```
(c) handler `get_state_update` (`?includeBlock`). Needs `new_root`(=this header state_root) +
`old_root`(=prev header state_root via another header read).

### E3. get_transaction (+ status)
(a) `GetTransactionByHash` (Reference C) returning
`Option<(Transaction, TransactionOutput, BlockNumber, TransactionOffsetInBlock)>` (impl:
`get_transaction_idx_by_hash` → `get_transaction` + `get_transaction_output`). (b) reuse
`transaction_to_fg` + a finality/status mapping fn (`NOT_RECEIVED`|`ACCEPTED_ON_L2`|
`ACCEPTED_ON_L1` from base-layer marker; exec `SUCCEEDED`/`REVERTED` from output). (c) handlers
`get_transaction`, `get_transaction_status` (status subset). Unknown hash → `NOT_RECEIVED`.

### E4. get_transaction_receipt
(b) reuse `output_to_fg_receipt` + add the FG receipt wrapper fields (`block_hash`,
`block_number`, `status`). (c) handler `get_transaction_receipt`.

### E5. classes
pre-PR `apollo_feeder_gateway: add class-manager client dependency` (add
`SharedClassManagerClient` field + thread `clients.get_class_manager_shared_client()`).
(b) `conversions/class.rs`:
```rust
use apollo_starknet_client::reader::GenericContractClass;
pub(crate) fn sierra_to_fg(c: starknet_api::state::SierraContractClass) -> GenericContractClass {
    GenericContractClass::Cairo1ContractClass(c.into()) // verify ContractClass field map (state.rs:64-69)
}
// Cairo0 path: GenericContractClass::Cairo0ContractClass(deprecated_class)
```
(c) handlers `get_class_by_hash` (`ClassManagerClient::get_sierra`/deprecated),
`get_compiled_class_by_class_hash` (`get_executable` → CASM).

### E6. get_storage_at / get_nonce / get_class_hash_at (one PR, handlers only)
All three exist on `StateSyncClient`. Three thin handlers returning hex `Felt`/`Nonce`/
`ClassHash`; params `contractAddress`(+`key`)+block id. Use `fg_json` (NOT axum `Json<T>`) like every
other handler — a bare scalar has no separators so the bytes happen to match either way, but staying
on `fg_json` keeps the "never `Json<T>`" rule (Reference B) exceptionless:
```rust
async fn get_storage_at(Extension(s): Extension<AppState>, Query(p): Query<HashMap<String,String>>)
    -> Result<Response, FeederGatewayError> {
    let bn = resolve_block_number(&s, parse_block_id(&p)?).await?;
    let addr = parse_contract_address(&p)?; let key = parse_storage_key(&p)?;
    Ok(fg_json(&s.reader.storage_at(bn, addr, key).await?))
}
```

### E7. get_code / get_full_contract
(b) `get_full_contract` reuses E5 conversion; `get_code` returns legacy `{ bytecode, abi }`
built from the class. (c) two handlers.

### E8. id↔hash mappings
(a) `GetBlockNumberByHash` (Reference C) + tx id↔hash if needed. (c) four thin handlers
(`get_block_hash_by_id`, `get_block_id_by_hash`, `get_transaction_hash_by_id`,
`get_transaction_id_by_hash`).

### E9. get_signature / get_public_key
(a) `GetBlockSignature` (Reference C, backed by `get_block_signature`). (b) convert
`starknet_api::block::BlockSignature` (a newtype `BlockSignature(pub Signature)` where
`Signature { r: Felt, s: Felt }`) → `BlockSignatureData`. NOTE (verified 2026-06-02):
`BlockSignatureData` (`block.rs:475`) is an **enum**, not a struct — variants `Deprecated { signature:
[Felt;2], signature_input: BlockSignatureMessage }` and a non-deprecated `{ block_hash, signature:
[Felt;2] }`. Build the correct variant (`signature = [sig.0.r, sig.0.s]`); pick deprecated-vs-not to
match what Python FG emits for the target version. (c) handlers.
`get_public_key`: **D2** — add a `sequencer_public_key` config field here OR read from synced
signature data; confirm source with the team.

### E10. synced pending data
PR `apollo_starknet_client: derive Serialize on PendingData` (add `Serialize` to `PendingData`
+ nested `PendingBlockOrDeprecated`/pending-state types in `reader/objects/pending_data.rs`).
Then (a)/(b)/(c) serve the pending branch of `get_block`/`get_state_update` from the synced
`PendingData` (NOT sequencer-built preconfirmed blocks — Bucket C, out of scope).

### E11. integration smoke test
PR `apollo_feeder_gateway: e2e route test` — boot `WrapperServer` over `MockChainDataReader`
(or `get_test_storage()`); hit routes via `reqwest` (dev-dep); assert Reference-B JSON + parity
error envelopes; cover empty/missing/boundary/not-found (no panics on request-derived values).

---

# PHASE F — Compute endpoints (server-side execution)

`execute_call` signature (`apollo_rpc_execution/src/lib.rs:226`):
`execute_call(storage_reader, maybe_pending_data, &chain_id, state_number, block_context_number,
&contract_address, entry_point_selector, calldata, &execution_config, override_kzg_da_to_false,
class_manager_handle) -> ExecutionResult<CallExecution>`. Run under `spawn_blocking`. Execution
needs the `StorageReader`, so extend `apollo_state_sync` (mirror `apollo_rpc` api_impl.rs:892).

1. **call_contract**
   - (a) PR `apollo_state_sync: expose call_contract` — Reference C with request
     `CallContract { block_number: BlockNumber, contract_address: ContractAddress,
     entry_point_selector: EntryPointSelector, calldata: Calldata }` → `Vec<Felt>`. Impl:
     ```rust
     async fn call_contract(&self, req: CallContractInput) -> StateSyncResult<Vec<Felt>> {
         let storage_reader = self.storage_reader.clone();
         let chain_id = self.chain_id.clone(); let exec_config = self.execution_config.clone();
         let class_manager = self.class_manager_handle.clone();
         tokio::task::spawn_blocking(move || {
             let state_number = StateNumber::right_before_block(req.block_number);
             execute_call(storage_reader, None, &chain_id, state_number, req.block_number,
                 &req.contract_address, req.entry_point_selector, req.calldata,
                 &exec_config, false, class_manager)
                 .map(|call| call.execution.retdata.0)
         }).await.map_err(/* join error -> StateSyncError */)?
            .map_err(/* execution error -> StateSyncError */)
     }
     ```
     **PREREQUISITE (blocker — these fields do NOT exist on `StateSync` today):** `StateSync`
     (`apollo_state_sync/src/lib.rs:57-63`) holds only `storage_reader`/`new_block_sender`/
     `starknet_client`/`config_manager_client`/`dynamic_config`. `execute_call` requires
     `chain_id: ChainId`, `execution_config: ExecutionConfig`, and
     `class_manager_handle: Option<(SharedClassManagerClient, Handle)>`. Pick ONE before coding,
     and update this PR accordingly: **(A)** add the three fields to `StateSync` + thread them via
     `StateSync::new` from `apollo_node/components.rs`; or **(B)** inject them into the FG read
     backends instead (co-located: `ColocatedStorageReader` runs `execute_call` directly; remote:
     a dedicated call client) so `StateSync` is untouched — (B) also keeps execution off the single
     state-sync process. `apollo_batcher::call_contract` exists but only for latest/batcher state.
   - (b) PR — FG call objects: parse legacy `CallFunction { contract_address,
     entry_point_selector, calldata, signature }`; respond `{ "result": [felt…] }`.
   - (c) PR — `POST /feeder_gateway/call_contract` handler (block id in query).
2. **get_transaction_trace**
   - (a) PR `apollo_state_sync: expose trace_transaction` — wrap
     `apollo_rpc_execution::simulate_transactions` (`lib.rs:981`) for one tx hash.
   - (b) PR — define `crate::objects::trace` FG structs (`TransactionTrace`,
     `FunctionInvocation`, validate/execute/fee-transfer invocations, events, messages, state
     diff) matching the Python FG JSON; capture a Python fixture; Reference-B test.
   - (c) PR — `GET /feeder_gateway/get_transaction_trace` handler.
3. **get_block_traces**
   - (c) PR — `GET /feeder_gateway/get_block_traces`; returns `{ traces: [{ transaction_hash,
     trace_root }] }` using the per-block simulate + the E-F2 trace objects.

---

# PHASE G — Caching (NICE-TO-HAVE, top of stack, OFF by default)

Per Reference D.6/D.7 the system must hit target RPS with caching **OFF** (validated by the POC
perf test); caching only trims hot-key/network cost. Implement it at the **very top of the
stack** — a decorator over `Arc<dyn ChainDataReader>` (so it wraps BOTH backends and is
config-gated off without touching handlers). Cache the **already-serialized JSON response** keyed
by `(endpoint, params)` for **finalized blocks only**, explicitly excluding `latest`/`pending`/
preconfirmed (Reference E: Python uses a 15 s TTL for those). `lru = "0.12"` and `cached = "0.44"`
are workspace deps. 3 PRs, each behavior-preserving with tests:
1. `apollo_feeder_gateway: top-of-stack response cache for finalized reads`:
   ```rust
   pub struct CachingReader {
       inner: Arc<dyn ChainDataReader>,
       responses: tokio::sync::Mutex<LruCache<ResponseKey, Arc<[u8]>>>, // tokio Mutex (never hold std guard across .await); serialized JSON, finalized only
   }
   // wrap at construction: if cache enabled, reader = Arc::new(CachingReader::new(reader, cfg)).
   // Only insert when the block id resolves to a finalized height; never cache latest/pending.
   ```
   Add a `cache: { enabled: bool (default false), max_entries }` config field HERE.
2. `apollo_feeder_gateway: short-ttl cache for tip/pending` — optional TTL tier for
   `latest`/`pending` (`cached::TimedCache` or manual `Instant`); document the staleness window
   (match or diverge from Python's 15 s — Reference E). TTL config field HERE.
3. `apollo_feeder_gateway: single-flight miss coalescing` — per-key
   `Mutex<HashMap<Key, Weak<Shared<Future>>>>` (or `tokio::sync::OnceCell`) so N concurrent misses
   cause ≤1 upstream read. Test with concurrent calls asserting one upstream hit.

---

# PHASE H — Deployment (own pod)

Stateless compute reading the EXISTING storage source (no per-replica storage). Topology is
config-only: consolidated = FG `Enabled`, state-sync client local; distributed = FG own service,
clients `Remote`.

## PR `deployment: add FeederGateway service`
`crates/apollo_deployments/`:
- Add `FeederGateway` to `DistributedNodeServiceName` (`distributed.rs:23`) (and hybrid/
  consolidated as desired) and to `ComponentConfigInService` (`deployment_definitions.rs`).
- In `get_component_configs` add `let feeder_gateway = Self::FeederGateway.component_config_pair(
  infra_port_allocator.next());` and a match arm:
  ```rust
  Self::FeederGateway => get_feeder_gateway_component_config(state_sync.remote(), class_manager.remote()),
  ```
  Write `get_feeder_gateway_component_config(...)` building a `ComponentConfig` with
  `feeder_gateway: ActiveComponentExecutionConfig::enabled()` + the two `Remote` clients.
- Bump `DISTRIBUTED_NODE_REQUIRED_PORTS_NUM` **11 → 12** (`distributed.rs:18`); add the
  `FeederGateway` variant to BOTH `DistributedNodeServiceName` (`distributed.rs:24-36`) and
  `ComponentConfigInService` (`deployment_definitions.rs:17`). Implement `get_scale_policy` =
  `ScalePolicy::AutoScaled`, `get_retries` per pattern, `get_components_in_service`.
REGEN: `cargo run --bin deployment_generator` (commit `resources/services/...` +
`resources/app_configs/...`). Commit: `deployment: add FeederGateway service`

## PR `deployment: feeder gateway k8s service yaml`
Add `deployments/sequencer/configs/layouts/<topology>/services/feeder-gateway.yaml` (mirror
`mempool.yaml`):
```yaml
name: feeder-gateway
replicas: 2
config:
  configList: crates/apollo_deployments/resources/services/distributed/replacer_deployment_feeder_gateway.json
service:
  enabled: true
  type: "ClusterIP"
statefulSet:
  enabled: false        # stateless — no per-replica storage
persistentVolume:
  enabled: false
hpa:
  enabled: true
  minReplicas: 2        # D1: tune from prod metrics
  maxReplicas: 10
  targetCPUUtilizationPercentage: 70
```
Add liveness/readiness probes → `/feeder_gateway/is_alive` / `/feeder_gateway/is_ready`
(readiness should also reflect remote state-sync reachability). Commit:
`deployment: feeder gateway k8s service yaml`

---

# PHASE I — Parity hardening + benchmark/API-diff (Reference E & F)

## PR — `apollo_feeder_gateway: parity quirks pass`
Drive the Reference-E checklist: error envelope + `StarknetErrorCode` strings + HTTP status
mapping (400/404/500); status-semantics computation (ACCEPTED_ON_L2 / PROVEN_ON_L2 (config gate
`enable_proven_on_l2_status`) / ACCEPTED_ON_L1 via base-layer marker / ABORTED); pending-null
stripping; `withFeeMarketInfo`/`withFeeProposalInfo` stripping; `includeBlock`+`includeSignature`
coupling; `NO_TRACE`/`NO_SIGNATURE_FOR_PENDING_BLOCK`; `UNINITIALIZED_CONTRACT` empty
code/contract. Unit tests per quirk. (Split into a few PRs if large.)

## PR — `apollo_feeder_gateway: API-diff harness vs Python FG`
Implement Reference F.2: capture curated Python-FG response fixtures (old/recent finalized,
pending, ABORTED, headerOnly, fee-flag variants, uninitialized contract, undeclared class,
out-of-range, every error) under `crates/apollo_feeder_gateway/resources/parity/`; a test that
calls the Rust FG, normalizes JSON (`to_normalized_json`), and `expect_file!`-asserts against the
captured Python JSON (regen `EXPECT_TEST=1`); one API-difference module per endpoint covering the
Reference-E quirks + negative error fixtures (envelope + HTTP status). Use `pretty_assertions`/
`similar` for readable diffs.

## PR — `apollo_feeder_gateway: RPS load-test suite`
Generalize the POC perf test (F.1) across endpoints (get_block full/header, get_state_update,
get_storage_at, get_nonce, get_class_hash_at): RPS + p50/p95/p99, cache-on vs cache-off, and the
three deployment backends (co-located vs remote same-cluster vs remote cross-node) to confirm the
Reference-D.5 expectations. Document results.

---

# Verification (whole feature)
- Per PR: `/validate`; `cargo build -p <touched>`; `SEED=0 cargo test -p <touched>`;
  `unset CI && scripts/rust_fmt.sh`; `python scripts/run_tests.py --command clippy
  --changes_only --commit_id HEAD`. After config/dashboard/deployment changes run the matching
  regen bin and confirm zero diff.
- Wire-compat: Reference B — **byte-exact** equality of `to_python_json(struct)` vs captured Python
  `resources/parity/*.json` (key order + map order + spaced separators + Felt hex + null-omission);
  NOT `serde_json::Value` equality.
- E2E (from E11): boot `WrapperServer` over `get_test_storage()`/mock, hit routes, assert
  byte-exact parity JSON + error envelopes (via `to_python_json`).
- Parallel-run (post-merge): run new FG beside Python FG, diff live/replayed traffic before
  re-pointing consumers.

# Open decisions
- **D1** HPA replica bounds — needs prod metrics (throughput vs connections vs HA).
- **D2** `get_public_key` source — config field vs synced signature data. **Recommended (confirm
  with team): read from the synced `BlockSignature` data already in storage (no new key/config);
  fall back to a config field only if signatures aren't reliably present.** Blocks E9 until confirmed.
- **D3 (was B5)** `call_contract`/traces (Phase F) need `chain_id`/`execution_config`/
  `class_manager_handle` not on `StateSync`. **Recommended: inject them into the FG read backends
  (Option B) — `ColocatedStorageReader` runs `execute_call` directly, keeping `StateSync` untouched
  and execution off the single state-sync process.** Add as a prerequisite PR before F.1.
- **D4 (new, verified 2026-06-02)** `get_state_update` `replaced_classes` parity (blocks E2.c
  parity). `starknet_api::state::ThinStateDiff` has NO `replaced_classes` (its `deployed_contracts`
  merges deploys + replacements), so the FG cannot split them from stored state alone — Python FG
  reports them separately and external clients may depend on it. Options: **(A)** reconstruct by
  reading, for each entry in `deployed_contracts`, whether the address had a class at the previous
  block (extra reads per state-update; correct); **(B)** check whether sync storage retains the
  original deployed/replaced split elsewhere (e.g. the raw synced state diff before thinning) and
  re-serve that; **(C)** accept divergence and document it (only if a stakeholder confirms no client
  reads `replaced_classes`). **Option A is NOT open research — copy the in-repo reference: `apollo_rpc`
  `convert_thin_state_diff` (`v0_8/api/api_impl.rs:1686-1710`) reconstructs `replaced_classes` from the
  same lossy `ThinStateDiff` by iterating `deployed_contracts` and calling `is_deployed(prev_block, addr)`
  — present-at-prev-block ⇒ replaced, else deploy.** So E2.c can resolve this in-PR without a team
  ruling (only Option C needs sign-off). Decide before E2.c ships; do NOT silently emit `vec![]`.
- New wire structs to author (not in `apollo_starknet_client`): `FeederGatewayBlockHeader`
  (headerOnly) and the trace objects (Phase F) — build against Python fixtures.

# Issue-hunt resolutions (applied 2026-06-02)
A 10-agent adversarial issue-hunt confirmed the items below (false-positives dropped: all file:line
refs accurate; `testing=[]` correct; config-when-needed honored except PR17b→PR19 fix). **Applied
in place:** B1 (PR19 constructs+passes `ReadExecutor`), M1 (error body via `to_python_json`), M2/M3
(read-pool config fields + REGEN moved to PR19), M5 (`ReadExecutor::run` AWAITS, never rejects), m8
(`Internal` error logs source, returns generic 500). **Recorded as plan amendments (fold into the
named PRs):**
- **M6 — add Phase E12** (missing endpoints): E12.a `serve get_number_of_transactions_in_backlog`,
  E12.b `serve get_oldest_transaction_age` (mempool/pending-batch reads; Bucket-C-adjacent — confirm
  a cheap source exists, else mark deferred).
- **M7 — promote status/finality to Phase E** (not Phase I edge cases): **E0.b** block-status
  computation (PENDING/ACCEPTED_ON_L2/PROVEN_ON_L2(config-gated)/ACCEPTED_ON_L1 via base-layer
  marker/ABORTED) — needed by E1; **E3.b** finality-status (NOT_RECEIVED/RECEIVED/…) — needed by E3.
- **M8 — fee-flag/coupling handlers in Phase E**: **E1.e** `withFeeMarketInfo`/`withFeeProposalInfo`
  stripping in `get_block`; **E2.d** same for `get_state_update` + enforce `includeSignature` requires
  `includeBlock` (else error).
- **M9 — pending-block parity errors as concrete PRs**: F.3 emits `NO_TRACE` (pending/uncomputed);
  E9 emits `NO_SIGNATURE_FOR_PENDING_BLOCK` — each adds/chooses the error code + HTTP status (per the
  Reference-E MISSING-codes list).
- **M10 — map insertion-order parity**: PR-parity-2 must add a test building a `StateDiff`/
  `ExecutionResources` with ≥3 entries and asserting JSON key order == Python's; where source
  iteration order is not guaranteed to match Python, apply an explicit sort key in the conversion.
- **M11 — nested-struct field-order audit**: extend PR-parity-1 to also lock the field order of
  NESTED structs (`ResourceBounds`, message objects, `StorageEntry`, per-tx-variant inner structs)
  against the Python fixtures, not just the top-level structs.
- **m1** extend the Felt lock test to the newtypes FG emits (`ClassHash`/`ContractAddress`/
  `TransactionHash`/`StorageKey`/`Nonce`/`BlockHash`). **m2** document `to_python_json` only changes
  spacing (delegates string-escaping to serde) + add a non-ASCII test (confirm `ensure_ascii` parity
  need). **m3** each variant (headerOnly/pending) byte-tests its own `resources/parity/*.json`. **m4**
  define `FeederGatewayBlockHeader` (small infra PR) and the trace objects (Phase F.0) as explicit
  structs. **m5** give the Reference-C state-sync extensions stable labels (PR-A.1…); **m6** E1(a) is
  ONE PR adding `GetBlockTransactionsWithOutputs`+`GetBlockSignature`+`GetBlockNumberByHash`, E8(a)
  adds only tx id↔hash. **m7** add `starknet_api` dep in PR16 only (PR18 adds only `apollo_storage`).
  **m9** specify graceful shutdown: drop the `ReadExecutor` job-sender to stop+join workers, wire
  axum `with_graceful_shutdown`.
- **Hardening (completeness critic):** param parsers (E0/E1.c/E6) must cap numeric params + bound
  hex-string lengths + return 400 on parse failure (never panic on request input); `POST
  /call_contract` (Phase F) needs a configurable request-body-size limit → 413; document that pending
  responses are speculative (may change between calls).
- **Nits:** n1 define `fg_json`/`to_python_json` once in PR-parity-3 (E0 just calls it); n2 the full
  POC-critical path is PR1–20 + PR-parity-1/2/3/Felt + E0 + E1 → milestone → PR21 → PR13/14 → E2+;
  n3 E0 restates its "requires PR-parity-3" dependency.

# Plan improvements (folded in 2026-06-02)
From an impact/effort review. **Applied above:** the PR Index + linear execution order (A5) and the
`.a/.b/.c` endpoint convention (A6). **Fold into the named PRs:**
- **A1 — parity lock test (PR-parity-1/3, E1.c):** replace the bare "do not reorder" comment with a
  `// PARITY LOCK` comment, and add a round-trip regression test
  `let f=read_parity_fixture(...); assert_eq!(to_python_json(&serde_json::from_slice::<T>(&f)?)?, f);`
  per anchor struct — locks field/byte order; lands once `to_python_json` (PR-parity-3) exists.
- **A2 — PR15 reuse pins:** copy `serialize_error` sanitization verbatim from
  `apollo_http_server/src/errors.rs:127-140`; reuse `StarknetErrorCode`/`KnownStarknetErrorCode`
  (`apollo_gateway_types/src/deprecated_gateway_error.rs:25-66`); map code+status per
  `errors.rs:95-114`. Do NOT re-derive.
- **A3 — PR21/Reference F reuse pins:** `HttpTestClient` (`apollo_http_server/src/test_utils.rs:33`),
  `get_integration_test_storage` (`apollo_integration_tests/src/storage.rs`), `expect_test` for
  fixtures. No hand-rolled harness.
- **A4 — observability (PR13 + Phase E):** expand to `FEEDER_GATEWAY_REQUESTS_TOTAL{endpoint}`,
  `..._FAILED{endpoint,error_code}`, `..._REQUEST_LATENCY_SECS` (histogram); record via ONE axum
  middleware layer (not per-handler copy). Add `ReadExecutor` `queue_depth()`/`max_capacity()`
  accessors → a saturation gauge. (Post-POC; PR13 already deferred.)
- **B1 — PR17b simplification:** implement `ReadExecutor` first as `Arc<Semaphore>(read_pool_size)` +
  `spawn_blocking` (~10 lines); adopt the dedicated thread pool (`blockifier/.../worker_pool.rs`)
  only if PR21 perf shows tokio lazy-spawn overhead matters. Keep queue accessors either way.
- **B2 — conversions:** `apollo_rpc/src/v0_8/transaction.rs:557-652` is a *reference only* for
  V0/V1/V3 field extraction; reuse the FG wire structs directly (don't redeclare); converters stay
  hand-written (formats differ).
- **B3 — Phase I tests:** one `rstest`-parameterized byte-parity test over `(endpoint, fixture,
  builder)` rows instead of N copies.
- **B4 — PR-parity-3:** add a deserialize→`to_python_json`→assert-original-bytes round-trip test
  (catches `skip_serializing_if`/missing-field bugs one-way tests miss).
- **B5 — readiness:** add `async fn health_check()` to `ChainDataReader` (PR16; impl in PR18/PR20);
  `is_ready` → 503 on backend-unreachable or read-pool >~80% saturated (PR8). `is_alive` stays 200.
- **B6 — POC leanness:** keep PR20 implemented but default node config to co-located + FG disabled;
  remote activation is a Phase H decision after the PR21 baseline.
- **B7 — `docs/feeder_gateway_configuration.md`:** config field reference table (ip/port/read_backend/
  read_pool_size/read_channel_capacity/cache.*) + topology notes (cache rows optional/off).
- **B8 — Phase H.3 cutover playbook:** replace the vague "parallel-run" line with staged shadow →
  canary (5/10/25/50/100%) → full cutover → rollback; splitter/canary are deploy-time, out-of-process.
- **Rejected (don't build):** auto fixture-corpus generator (use the existing `/regen-snip35-block-fixture`
  skill), nightly live-Python-diff CI job, in-process shadow-diff per handler + metric, dynamic
  kill-switch, compile-time field-count/no-HashMap macro guards (the byte round-trip test + IndexMap
  already enforce this), a shared `*_conversions` crate / apollo_rpc pending extraction / batched
  `GetBatch` RPC — all off the POC path; A1/B2 mitigate the drift risk they targeted at lower cost.

# Readiness assessment (2026-06-02, round 3) — VERDICT: READY TO START
A 5-dimension readiness review (POC data feasibility, the PR17 storage refactor, parity
achievability, open-decision sequencing, mechanical/CI executability) + adversarial verification of
every claimed blocker concluded: **no blocker requires resolution before coding starts — begin at PR1.**
Key confirmations: (1) every datum `get_block` needs IS in sync storage; the three body vectors
(`get_block_transactions`/`_outputs`/`_hashes`) are guaranteed aligned (same table + cursor), so
`izip!` is sound, and central sync writes header+signature+body atomically per finalized block.
(2) PR17 is a clean one-call-site forward of an already-bound reader; `StorageReader` clone shares the
single `Arc<Environment>` (no MDBX re-open) — and `exclusive:true` is exactly WHY clone-and-share is
mandatory. (3) Parity infra is buildable today with NO live Python FG (offline fixture generation +
A1/B4 round-trip lock). (4) D1→deploy-time, D2→E9, D3→Phase F, D4→E2.c — all post-POC, each with a
recommended resolution (D4 has an in-repo reference impl). (5) all regen bins/template files/macros/
config anchors exist (config-schema bin `cargo check` RC=0). **De-risking edits applied from this pass:**
the `get_block_hash` prose trap (Reference A + sketch — use `header.block_hash`, not the batcher-only
`block_hashes` table), state-reader line citations, the PR17 `_storage_reader` lint note, the D4
reference-impl pointer, and PR-parity-3 `ensure_ascii` (implement now — `revert_error` reaches the POC
path) + the offline-fixture note. **Soft follow-ups (NOT blocking):** obtain one real captured Python
response eventually to confirm the committed fixture matches today's wire output; settle the state_update
felt-padding question at E2.

# Codebase verification pass (2026-06-02, round 2)
Re-verified the plan's concrete claims (field names, Option-ness, signatures, enum variants, anchors)
against the live tree via 5 parallel readers + direct reads. **Bugs FIXED in this doc:**
- **`block_to_fg` commitments (E1.b): COMPILE BUG.** `starknet_api BlockHeader.transaction_commitment`/
  `event_commitment` are `Option<_>`, but FG `BlockPostV0_13_1` has them NON-Option → must
  `.unwrap_or_default()` (the prior "no unwrap" comment was backwards — it checked the dest, not the
  source). Fixed.
- **PR-parity-1:** embedded the verified `BlockPostV0_13_1` top-level order from
  `block_post_0_14_3.json` (struct decl order does NOT match it); flagged that fixture is indented
  (key-order reference only, not the byte-parity source).
- **E2 `replaced_classes`: PARITY GAP** elevated to **D4** — `starknet_api ThinStateDiff` lacks
  `replaced_classes`; emitting `vec![]` diverges from Python FG.
- **E9:** `BlockSignatureData` is an **enum** (not a struct); corrected the conversion note.

**Claims CONFIRMED accurate (no longer "verify" — treat as settled):** `execute_call` signature
(apollo_rpc_execution/src/lib.rs:226) matches EXACTLY incl. `class_manager_handle: Option<(SharedClassManagerClient, Handle)>`;
`simulate_transactions:981`; apollo_rpc inline-reads + spawn_blocking-for-execute pattern + direct
`storage_reader` field (api_impl.rs); `HttpTestClient` (test_utils.rs:33), `get_integration_test_storage`
(apollo_integration_tests/src/storage.rs), `flow_test_setup.rs` boots a full HTTP node,
`blockifier/src/concurrency/worker_pool.rs` (std::thread + mpsc) — all exist. All PR9–12 anchors exist
(`component_config.rs` L58/76/102/140; `node_config.rs` L260/304/343/528; `servers.rs` L97/671-674/704/718,
http_server label = `"Http"`; `components.rs` L268-285, import L17). `ActiveComponentExecutionMode`
{Enabled,Disabled} in `apollo_node_config::component_execution_config`. `KnownStarknetErrorCode` has
`BlockNotFound`/`MalformedRequest`/`UndeclaredClass`/`OutOfRangeClassHash`; `MALFORMED_REQUEST` serde
string is literally `"StarkErrorCode.MALFORMED_REQUEST"` (the `StarkErrorCode` typo is real — match it);
`StarknetError{code,message}`, `StarknetErrorCode::{KnownErrorCode,UnknownErrorCode(String)}`;
`serialize_error` at http_server/errors.rs:127-140. `max_concurrency` default 128 =
`apollo_infra/.../local_component_server.rs:36`. `DISTRIBUTED_NODE_REQUIRED_PORTS_NUM = 11` (bump to 12).
FG `Builtin` enum carries every `starknet_api` builtin variant + FG-only `Output` → **forward
starknet_api→FG map is TOTAL (no silent drops)**. `TransactionOutput` exposes accessor METHODS
(`execution_status()/messages_sent()/events()/execution_resources()/actual_fee()`). `ThinStateDiff` has
`class_hash_to_compiled_class_hash` (not `declared_classes`), `deployed_contracts`, `nonces`,
`deprecated_declared_classes`, `storage_diffs` (storage_diffs/nonces already `IndexMap`). FG
`ExecutionResources.builtin_instance_counter` + `ContractClass.entry_points_by_type` are `HashMap`
(PR-parity-2 targets — confirmed). `starknet_api InvokeTransactionV3.proof_facts` exists (non-Option) →
the `Some(t.proof_facts.clone())` inverse is correct. `PendingData` does NOT derive `Serialize`
(E10 must add it). `StateNumber::right_before_block` exists.
**Still NOT independently verified (keep the inline "verify" hedges):** the `StateDiff`/state-update
fixture field order and nested-struct order (PR-parity-1/M11 — no state-update fixture inspected this
round); FG `ExecutionResources`/`StateUpdate` "already matches Python" claim (needs a fixture).

# Critical files
- Template: `crates/apollo_http_server/src/{communication,http_server,errors,metrics}.rs`,
  `apollo_http_server_config/src/config.rs`.
- Node: `apollo_node/src/{components,servers}.rs`; config
  `apollo_node_config/src/{component_config,node_config}.rs`; bin
  `apollo_node/src/bin/update_apollo_node_config_schema.rs`.
- Wire structs/fixtures: `apollo_starknet_client/src/reader/objects/{block,state,transaction,
  pending_data}.rs` (+ `block_test.rs`, `resources/reader/block_post_*.json`).
- State sync: `apollo_state_sync_types/src/communication.rs` (Reference C),
  `apollo_state_sync/src/lib.rs` (handler `:113`, impl `:162`).
- Execution: `apollo_rpc_execution/src/lib.rs` (`execute_call:226`, `simulate_transactions:981`);
  reference `apollo_rpc/src/v0_8/api/api_impl.rs`.
- Deployment: `apollo_deployments/src/deployments/*`, `deployment_definitions.rs`,
  `bin/deployment_generator.rs`; `deployments/sequencer/...`. Scopes: `commitlint.config.js`.

---

# Implementation progress & findings (2026-06-03)

Bottom-up Graphite stack built and validated (every PR compiles; `116` tests pass across the
touched crates). **Implemented (in stack order):**
PR1, PR2 (empty crates) · PR3–PR8 (config + component skeleton + axum health routes) ·
PR9–PR12 (node wiring; FG boots disabled, serves health; REGEN clean) ·
PR16 (ChainDataReader + AppState + mock) · PR17 (state-sync returns `storage_reader`) ·
PR17b (bounded ReadExecutor) · PR18 (ColocatedStorageReader) · PR20 (RemoteChainDataReader) ·
PR19 (select backend by topology + read-pool config; REGEN) ·
PR-parity-1 (reorder `BlockPostV0_13_1`) · PR-parity-2 (`builtin_instance_counter` → IndexMap) ·
PR-parity-3 (`to_python_json` spaced serializer + ensure_ascii + `fg_json`) ·
PR-parity-Felt (Felt + newtype JSON lock) · PR15 (legacy error envelope, byte-parity) ·
E0 (`get_contract_addresses`, byte-parity tested end-to-end) ·
E8-partial (`get_block_hash_by_id`, both backends, 404 on miss) ·
PR13 (`FEEDER_GATEWAY_REQUESTS_TOTAL` metric + `MetricScope::FeederGateway`) ·
PR14 (dashboard row + dedup-test entry; `dev_grafana.json` regenerated) ·
B7 (`docs/feeder_gateway_configuration.md` config reference).

## NEW blockers found (corrections to this plan — resolve before E1.c et al.)
1. **Transaction byte-parity is BLOCKED by serde tag position.** `reader::objects::transaction::
   Transaction` is `#[serde(tag = "type")]`, which serde serializes **tag-first**
   (`{"type": "INVOKE_FUNCTION", ...}`). The Python fixture puts `"type"` **last** (verified:
   invoke keys end with `type`). Field reordering (PR-parity-1/M11) CANNOT move the tag — it needs
   a **custom `Serialize` for `Transaction`** (emit variant fields then `type`) or restructuring to
   an untagged enum with an explicit trailing `type` field. This is a wire-format change in the
   shared `apollo_starknet_client` crate, so it needs a deliberate decision/review. **Gates every
   tx-carrying response: E1.c (get_block), E3 (get_transaction), E4 (get_transaction_receipt).**
2. **Fixture coverage gap.** `block_post_0_14_3.json` contains only invoke-v3 txs, so the
   declare/deploy/deploy_account/l1_handler converters (E1.b) and the L1-handler receipt-message
   path cannot be round-trip/byte verified from the committed fixture. Capture additional Python-FG
   fixtures (or `/regen-snip35-block-fixture` variants) before landing those converters, per the
   repo verification mandate (don't ship unverified conversions).
3. **Receipt optional-message parity.** FG `TransactionReceipt.l1_to_l2_consumed_message` has no
   `skip_serializing_if`, so it always serializes, but Python omits it for non-L1Handler receipts —
   another byte-parity gap for the receipt converter/struct (M11 area).

## Intentional deviations from the written plan (applied in the stack)
- **PR1/PR2:** each crate's `members` + `[workspace.dependencies]` + commitlint entries land in the
  PR that *creates* that crate (the plan front-loaded both crates into PR1, but `members` is eager
  so a member dir that doesn't exist yet fails `cargo build`).
- **PR9:** `feeder_gateway` is deliberately NOT added to `validate_tx_ingestion_components_disabled`
  — it is a read-only component, not tx-ingestion; adding it would wrongly force FG off on
  validation-only nodes (where re-serving reads is desirable). Field/dump/`disabled()` entries added.
- **PR17b ReadExecutor:** implemented as the B1 simplification — a bounded `tokio::sync::Semaphore`
  (size `read_pool_size`) gating `spawn_blocking`; `run()` awaits a permit and never rejects (M5).
  Dropped the separate `read_channel_capacity` config (the semaphore subsumes the bound). Kept
  `max_concurrency()`/`in_flight()` saturation accessors. Swap to the `std::thread` pool only if the
  PR21 perf test shows tokio lazy-spawn overhead matters.
- **PR19:** FG component construction relocated to AFTER state-sync in `components.rs` (the colocated
  backend needs state-sync's `storage_reader`); the state-sync match now yields
  `Option<StorageReader>`. Config gained `read_backend` (default `Colocated`) + `read_pool_size`.
- **PR-parity-1:** reordered only the top-level `BlockPostV0_13_1` (verified against the fixture).
  Nested tx/receipt reordering deferred — tx byte-parity is blocked by finding 1 regardless, so
  reordering them now buys no verified parity and risks churn.
- **PR-parity-2:** `builtin_instance_counter` → IndexMap now (get_block path). Deferred
  `ContractClass.entry_points_by_type` → IndexMap to E5 (it ripples into `starknet_api`'s
  `from_hash_map` and is on the get_class path, not get_block).
- **PR15:** BLOCK_NOT_FOUND / TRANSACTION_NOT_FOUND map to HTTP **404** per Reference E (the PR15
  code snippet's 400 contradicted Reference E; followed Reference E). Error body uses `to_python_json`
  (M1). `fg_json` defined in `serialization.rs` (needs `FeederGatewayError: IntoResponse`, so it
  lands with PR15, not PR-parity-3).
- **PR16:** added only `starknet_api` (per m7); a minimal `FeederGatewayError::Internal` was added in
  PR16 so `FgResult` has an error type, then expanded into the full envelope in PR15.

## Remaining work (not yet implemented)
- **Decision-gated (finding 1):** E1.b/E1.c (get_block conversions + handler), E1.d/E1.e, E3, E4 —
  all carry transactions, so they need the custom `Transaction` `Serialize` decision first.
- **Needs a live Python FG / more fixtures to verify edge cases:** E6
  (`get_storage_at`/`get_nonce`/`get_class_hash_at`). The happy path is byte-trivial (bare felt), but
  the undeployed-contract behavior diverges between backends today — the co-located `StateReader`
  returns `0x0` for storage of an undeployed contract while the state-sync client errors
  `ContractNotFound` — and which one matches Python is unverified here. Resolve (and make the backends
  consistent) against the Python FG before shipping.
- **Phase H — deployment (own pod), well-scoped but large/mechanical:** add a `FeederGateway` variant
  to `DistributedNodeServiceName` AND `ComponentConfigInService`; because `get_components_in_service`
  matches `ComponentConfigInService` exhaustively in every per-service arm, a new variant requires an
  added arm in each (~12 services); also `get_scale_policy` (AutoScaled), `get_retries`,
  `get_component_configs` (a `get_feeder_gateway_component_config(state_sync.remote(),
  class_manager.remote())` builder), bump `DISTRIBUTED_NODE_REQUIRED_PORTS_NUM` 11→12, add the k8s
  service yaml, and regen `deployment_generator`. Deferred because the Remote topology it serves is
  default-off (B6) and the wide exhaustive-match edit is error-prone; do it as a focused PR.
- **Also remaining:** the per-endpoint `RemoteChainDataReader` methods + Reference-C state-sync
  extensions (e.g. `GetBlockTransactionsWithOutputs`, `GetBlockNumberByHash`), E2/E5/E7/E9–E12, PR21
  (perf test), the observability middleware that records `FEEDER_GATEWAY_REQUESTS_TOTAL` (A4), and
  Phases F (compute), G (caching), I (parity hardening + benchmark/API-diff).

---

# Live feeder gateway verification (2026-06-03)

The Python feeder gateway is reachable for ground-truth byte/behavior checks at
`https://feeder.<network>.starknet.io/feeder_gateway/<endpoint>` (e.g.
`feeder.alpha-sepolia.starknet.io`, `feeder.alpha-mainnet.starknet.io`) — note the `feeder.`
subdomain. This is the canonical way to verify parity; **guesses (and even this plan's "Reference E")
were wrong in several places**, caught only by hitting the live service. Findings so far:

- **Error HTTP status is 400, not 404 (FIXED).** `get_block?blockNumber=999999999` →
  `HTTP 400 {"code": "StarknetErrorCode.BLOCK_NOT_FOUND", "message": "Block number 999999999 was not
  found."}`. Reference E claimed 404 for BLOCK_NOT_FOUND/TRANSACTION_NOT_FOUND; the live service
  returns 400 (a `StarknetErrorCode` body is 400 by default). The error-envelope PR was corrected to
  map both to 400. **Follow-up:** the live message embeds the block number, so full message parity
  needs the number threaded into `FeederGatewayError::BlockNotFound`.
- **`get_block_hash_by_id` parameter is `blockId`, not `blockNumber` (FIXED).** Verified
  `...get_block_hash_by_id?blockId=1` → `"0x78b6..."` (bare quoted hash, HTTP 200); `blockNumber` is
  rejected. The handler now reads `blockId` (numeric form; `latest`/`pending`/hash forms are a
  follow-up).
- **`get_contract_addresses` is wrong, and is a NETWORK-VARIABLE MAP (DOCUMENTED, not yet fixed).**
  The response is not a fixed-shape struct: **mainnet returns 4 fields**
  (`Starknet`, `GpsStatementVerifier`, `strk_l2_token_address`, `eth_l2_token_address`) while
  **sepolia returns 8** (adds `MemoryPageFactRegistry`, `MerkleStatementContract`,
  `FriStatementContract`, `HybridGpsFactAdapter`) in a DIFFERENT key order. So the L1-contract set and
  its order are network-specific. The `Starknet`/`GpsStatementVerifier`/etc. values are **EIP-55
  checksummed L1 (Ethereum) addresses** (e.g. `0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4`), NOT
  felts; `strk_l2_token_address`/`eth_l2_token_address` are felts. The current
  `FeederGatewayContractAddresses` (two `ContractAddress` felt fields) is therefore byte-wrong on
  count, type, AND shape. **Correct model:** a config-driven ORDERED map (e.g. `IndexMap<String,
  EthAddress>`, preserving the per-network insertion order) of L1 contracts serialized with EIP-55
  checksum (sha3/Keccak256 is a workspace dep; `EthAddress`/`H160` default `Serialize` is lowercase,
  so a checksum serializer is needed), plus the two L2 token-address felt fields. This is a design
  decision (ordered-map config schema + EIP-55), not a quick struct edit — verify the exact per-network
  field set + order against the live service.
- **Transaction `type` is LAST (CONFIRMED).** A real DECLARE tx (sepolia block 1) serializes its keys
  ending in `type`, confirming the serde `#[serde(tag="type")]` tag-first blocker is real and that
  `get_block`/`get_transaction`/`get_transaction_receipt` need the custom `Transaction` `Serialize`
  (emit fields then `type`) before they can be byte-parity. Sepolia block 1 is also a ready DECLARE
  fixture; iterate block numbers to capture invoke/deploy/l1_handler fixtures for the other converters.

**Recommended process going forward:** for each endpoint, fetch the live response first, diff against
the Rust output byte-for-byte, and only then ship. The live service makes every parity claim testable;
do not rely on the plan's prose for status codes, field sets, param names, or formats.
