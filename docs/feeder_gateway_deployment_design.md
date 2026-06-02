# Design: Feeder Gateway — System Integration, Pods, Replicas & Multi-core

**Status:** Draft / for review
**Author:** andrew.l
**Date:** 2026-06-02
**Companion doc:** `feeder_gateway_migration_design.md` (endpoints, data sources, wire format)

This doc answers four operational questions for the new sequencer feeder-gateway (FG)
component: (1) how it fits into the system, (2) how it runs in its own pod, (3) how it
scales to many replicas, and (4) whether/how it uses multiple cores.

## 0. TL;DR

- **Multi-core: yes, natively.** The node runs a multi-threaded tokio runtime (worker
  threads = #cores). As an *active* axum HTTP component, every request is an independent
  task scheduled across all cores, with no concurrency semaphore. CPU-heavy work
  (`call_contract`, traces) is offloaded via `spawn_blocking`. **One FG pod already uses
  the whole machine.**
- **Own pod: yes, first-class.** Define a new `NodeService` in the distributed/hybrid
  deployment topology where the FG active component is `Enabled`; other services disable
  it. Standard CDK8s service + k8s DNS service discovery.
- **Replicas: scale *compute*, never *storage*.** Hard constraint (from review): **no
  additional persistent-storage replicas**; in-memory caching is acceptable. That rules
  out per-replica MDBX (Pattern B) and any new replicated storage tier. The chosen shape:
  **stateless, multi-core FG replicas (k8s Deployment + HPA, no PersistentVolume) that
  cache aggressively in memory and read from the storage source already in the
  deployment.** This works because finalized FG data is **immutable** (§4.1), so cache hit
  rates are near-perfect and the single storage source only serves cold misses + the tip.
  Multi-core further means hundreds of Python replicas collapse to far fewer pods (§1).

## 1. Why the Python replica count doesn't transfer

The Python FG runs hundreds of replicas in production. Two properties of that system
make that necessary — and **neither holds in the sequencer**:

| Property | Python FG | Sequencer FG |
|---|---|---|
| Per-process CPU | ~1 core (GIL) → need many processes for throughput | all cores per process (tokio multi-thread) |
| Storage backend | shared / networked store, readable by N processes | **local embedded MDBX, `exclusive: true`** (`apollo_storage/src/db/mod.rs:244`) — one process per DB file |
| Replica = | a thin stateless reader over shared storage | depends on data-access choice (§4) — not free |

**Implication 1 (throughput):** a single Rust FG pod on an M-core machine does the work
of roughly M Python replicas. Hundreds of Python replicas plausibly collapse to *tens or
fewer* multi-core pods. The real target is "enough pods to serve peak QPS + HA headroom,"
recomputed from QPS, not the legacy count.

**Implication 2 (data access):** you cannot point hundreds of pods at one MDBX file. How
each replica gets its data is *the* design decision, and it has real cost (§4). This is
the crux the rest of the doc addresses.

## 2. How the FG fits into the system

### 2.1 Component shape (from companion doc)
The FG is an **active** component: a `WrapperServer<FeederGatewayComponent>` running an
axum server (the pattern `apollo_http_server` uses — `apollo_http_server/src/communication.rs:5`,
`empty_component_server.rs`). It is *not* a reactive request/response component, because
read-only HTTP serving gains nothing from the infra request-enum.

It needs read access to chain data. Two sourcing strategies (the §4 fork):
- **Direct storage**: open `StorageReader` in-process (like `apollo_rpc`, which is handed
  a `StorageReader` and serves on port 8090 — `apollo_state_sync/src/runner/mod.rs:515`).
  Requires the FG pod to *be* a storage-holding node (own MDBX).
- **Remote data tier**: hold a `RemoteStateSyncClient`
  (`apollo_state_sync_types/src/communication.rs:118`) and/or talk to the
  `StorageReaderServer` HTTP endpoint (`apollo_storage/src/storage_reader_server.rs`,
  `/storage/query`, ~45 query variants), so the FG pod is stateless.

### 2.2 Active vs reactive in the infra model
Active components use `ActiveComponentExecutionMode` (`Enabled`/`Disabled`,
`apollo_node_config/src/component_execution_config.rs`), not the reactive `Remote` mode.
So "FG in its own pod" = a service where `components.feeder_gateway` is the active
`Enabled` component, plus the *clients* it needs to reach its data tier (those clients use
the reactive `Remote` mode pointing at the state-sync/storage service). This mirrors how
`apollo_http_server` (active) holds a `SharedGatewayClient` to a possibly-remote gateway.

## 3. Multi-core support (detailed)

- **Runtime:** `apollo_node/src/main.rs:36` uses `#[tokio::main]` with no override →
  multi-threaded runtime, worker threads = CPU cores. Work-stealing across all cores.
- **Active HTTP component:** `axum::serve` (`apollo_http_server/src/http_server.rs:99`)
  spawns each connection/request as its own task; **no semaphore cap**. Throughput scales
  with cores until a shared resource (storage, allocator) saturates.
- **CPU-bound endpoints:** `call_contract` and trace re-execution must run under
  `tokio::task::spawn_blocking` (as `apollo_rpc_execution` / batcher already do, e.g.
  `apollo_batcher/src/batcher.rs` call path) so blockifier execution doesn't starve the
  async reactor. Tokio's blocking pool (default 512 threads) absorbs these.
- **Contrast (not used here):** `ConcurrentLocalComponentServer`
  (`apollo_infra/src/component_server/local_component_server.rs:275`, default
  `max_concurrency = 128`) is the concurrency mechanism for *reactive* components and
  requires `Clone`. The FG, being active, doesn't use it — axum provides the concurrency.

**Conclusion:** no special work needed for multi-core; the requirement is just (a) keep
the data-read path non-blocking and (b) `spawn_blocking` the execution endpoints.

## 4. Replicas — stateless cached compute over a single storage source

**Constraint (from review):** do not add persistent-storage replicas. In-memory caching
is fine. This decides the architecture: FG replicas are **stateless compute** that cache
in memory and read from the storage source the deployment already has. We add FG pods, we
never add MDBX copies.

### 4.1 Why this works: finalized FG data is immutable
A block, its transactions, receipts, state diff, and declared classes at height N **never
change** once finalized. Only the chain *tip* (latest height, pending/preconfirmed data,
`latest`-tagged queries) moves. Therefore:
- Per-replica in-memory caches keyed by (height / block-hash / tx-hash / class-hash) have
  **near-100% hit rates** for historical reads — and historical reads dominate FG traffic
  (explorers/indexers backfilling, wallets fetching old txs).
- Cache entries for finalized heights need **no invalidation** (immutable) — only tip and
  pending entries get short TTLs. This mirrors the Python FG's existing cache tiers
  (long-lived concrete-block cache vs. 15 s pending TTL — migration doc §3 / D5), which we
  now treat as **load-bearing**, not optional.
- Net effect: the single storage source sees only cold misses + tip refreshes, so it does
  **not** need to be replicated to serve a large stateless FG fleet.

### 4.2 Topology (recommended)
```
                ┌──────────── stateless, HPA-scaled, NO PersistentVolume ───────────┐
  clients ─►[LB]─►  FG pod 1        FG pod 2        ...        FG pod N
                    [in-mem cache]  [in-mem cache]             [in-mem cache]
                        │ miss          │ miss                     │ miss
                        └───────────────┴────── HTTP/2 ────────────┘
                                          ▼
                          existing storage source (state-sync node, MDBX)
                                   — NOT a new/replicated tier —
```
- **FG pods:** k8s **Deployment + HPA**, `ScalePolicy::AutoScaled`
  (`scale_policy.rs:14`, `idle_connections = 0`), **no StatefulSet, no PersistentVolume**.
  Multi-core (§3). Hold a `RemoteStateSyncClient`
  (`apollo_state_sync_types/src/communication.rs:118`) and/or talk to the existing
  `StorageReaderServer` (`/storage/query`, `apollo_storage/src/storage_reader_server.rs`).
- **Storage source:** whatever node already holds the synced MDBX in the target
  deployment topology (consolidated/hybrid/distributed). FG reuses it; it is not
  duplicated. (If that deployment already runs >1 such node for the sequencer's own HA,
  FG can fan reads across them — but FG itself never *adds* one.)

### 4.3 Honest residual risks (no new storage allowed, so these are accepted/mitigated)
- **Single storage source = throughput ceiling + SPOF for cold/tip traffic.** Caching
  removes the historical load but not the tip/cold-miss load. Mitigations within the
  constraint: (a) cache the tip with a short TTL and coalesce concurrent misses
  (single-flight) so N replicas cause ≤1 upstream read per key per TTL; (b) negative-cache
  not-founds; (c) optionally a tiny shared cache (e.g. one Redis) if per-pod memory caches
  prove insufficient — *cache, not persistent storage*, so within the constraint. If the
  source still saturates, the only escape is more storage-holding nodes, which is
  explicitly out of scope — escalate to a storage-layer decision rather than work around it.
- **Coverage gap (D-A):** `StateSyncClient` is limited
  (`get_block`/`get_storage_at`/`get_nonce_at`/`get_class_hash_at`/latest —
  `communication.rs:128`); it does **not** serve receipts, traces, or full class defs.
  For full FG parity over the remote path, use the richer `StorageReaderServer` variants
  (~45) or extend `StateSyncClient`. Decide before building the remote read path.
- **Network hop:** ~2–5 ms per upstream read; an FG request doing many small reads
  multiplies it. Caching hides this for hits; for misses, prefer coarse-grained upstream
  calls (`get_block` returns the whole block) over many fine-grained reads.

### 4.4 Rejected (recorded so the constraint is explicit)
- **Per-replica full node (own MDBX):** adds persistent storage per replica → violates the
  constraint. Rejected despite its lower latency / no-SPOF appeal.
- **New replicated state-sync/StorageReaderServer tier:** also multiplies persistent
  storage → rejected. The FG fleet must share the *existing* source.
- **Shared networked MDBX volume:** unsupported anyway (MDBX is exclusive-locked and not
  network-FS safe).

## 5. Running in its own pod — concrete mechanism

1. **Component config.** Add `feeder_gateway` (active) to `ComponentConfig`
   (`apollo_node_config/src/component_config.rs`) and wire create/serve in
   `apollo_node/src/{components,servers}.rs` as a `WrapperServer` (active), per companion §6.3.
2. **Define the service.** Add a variant to the deployment topology enums in
   `crates/apollo_deployments/` — e.g. `DistributedNodeServiceName::FeederGateway`
   (`deployments/distributed.rs:24`) — implementing the service trait with
   `get_components_in_service()` = FG active component `Enabled`, plus its state-sync /
   storage read client set to `Remote` (pointing at the existing storage source); and
   `get_scale_policy()` = `AutoScaled`.
3. **Scale policy.** `ScalePolicy::AutoScaled` (stateless, `idle_connections = 0`,
   `scale_policy.rs:14`).
4. **Service YAML / CDK8s.** Add a layout service file (cf.
   `deployments/sequencer/configs/layouts/.../services/<svc>.yaml`): `replicas`,
   `service.type: ClusterIP`, `hpa.{enabled,minReplicas,maxReplicas,targetCPUUtilizationPercentage}`,
   and crucially **`statefulSet.enabled: false`, `persistentVolume.enabled: false`** (FG is
   stateless — no per-replica storage, per the constraint). CDK8s
   (`deployments/sequencer/src/constructs/{deployment,hpa,service}.py`) emits the manifests.
5. **Service discovery.** Remote clients use a URL placeholder
   (`REMOTE_SERVICE_URL_PLACEHOLDER`, `service.rs`) substituted at deploy time with the
   k8s service DNS name + infra-assigned port (`replacers.rs`). FG pods reach the data
   tier by its ClusterIP DNS name.
6. **Ports / probes.** FG listens on its own HTTP port (FG wire API; legacy 9713 for drop-in
   parity if behind the same LB). Add liveness/readiness probes (readiness should reflect
   data-tier reachability for Pattern A, or sync-height freshness for Pattern B).

## 6. Open decisions

- **D-1 (replica driver) — OPEN, needs ops input:** Is the production figure driven by
  **throughput**, **concurrent-connection count**, or **HA/blast-radius**? Currently
  *unknown* (to be checked against prod metrics / infra team). Sets the HPA min/max and the
  real target pod count; the architecture (§4) holds regardless, but the numbers don't.
- **D-2 (data tier) — RESOLVED:** Stateless cached FG replicas over the *existing* storage
  source; **no new persistent storage, no per-replica MDBX**. In-memory caching only.
- **D-A (coverage):** Use `StorageReaderServer`'s richer variant set for the remote read
  path, or extend `StateSyncClient` (which lacks receipts/traces/full-class) for full FG
  parity? Decide before building the read path.
- **D-3 (staleness SLO):** Stateless replicas + caching mean different pods can briefly
  serve slightly different tips. What read-staleness is acceptable for tip/`latest`
  queries, and what TTL does that imply? (Finalized data is exact regardless — §4.1.)
- **D-4 (port/LB):** Reuse legacy port 9713 + existing LB for drop-in external parity, or
  expose a new ingress?
- **D-5 (validate `StorageReaderServer`):** Confirm production-grade (it has a dynamic
  enable flag) and benchmark per-query network-hop overhead, including the single-flight
  miss-coalescing (§4.3) that protects the single source.
- **D-6 (shared cache, if needed):** If per-pod memory caches prove insufficient for tip/
  cold-miss load, is a single small shared cache (e.g. Redis) acceptable? It's a *cache*,
  not persistent storage, so within the constraint — but confirm before relying on it.
```
