[↑ Index](README.md) | [← Prev: 02 — The Component Model](02-component-model.md) | [→ Next: 04 — Key Data Flows](04-data-flows.md)

---

# 03 — Component Reference

A concise description of each component's responsibility, its key dependencies, and where to start reading.

---

## User-Facing Entry Points

### HTTP Server (`apollo_http_server`)

The public-facing API server. Built on [Axum](https://github.com/tokio-rs/axum), a Rust async web framework. It deserializes incoming HTTP requests and forwards them to the Gateway. It also exposes the Config Manager's API for hot-reloading config. It is an *active* component (WrapperServer) — it has its own run loop.

**Depends on:** Gateway (client), Config Manager (client)
**Start reading:** `[crates/apollo_http_server/src/http_server.rs](../crates/apollo_http_server/src/http_server.rs)`

### Gateway (`apollo_gateway`)

Validates incoming transactions: signature checks, fee checks, nonce validation, contract class validation. Acts as a gatekeeper before the Mempool. It queries State Sync for current state and Class Manager for contract class info.

A **nonce** is a per-account counter stored in the Starknet state. Every transaction must include the sender account's current nonce, and the Gateway rejects any transaction whose nonce doesn't match. This serves two purposes: it prevents replay attacks (a valid signed transaction cannot be submitted a second time, because the nonce will already have been consumed), and it enforces ordering (transactions from the same account are processed in nonce order). Because the nonce is part of the on-chain account state, State Sync — as the source of truth for that state — is the authoritative place to read it.

**Depends on:** State Sync (client), Mempool (client), Class Manager (client), Proof Manager (client)
**Start reading:** `[crates/apollo_gateway/src/gateway.rs](../crates/apollo_gateway/src/gateway.rs)`

---

## Transaction Staging

### Mempool (`apollo_mempool`)

Maintains a priority queue of pending transactions. Transactions are ordered by tip (fee). The Batcher calls `get_txs` to drain batches for a block. When a block is committed the Mempool is notified to drop those transactions.

**Depends on:** Mempool P2P Propagator (client), Config Manager (client)
**Start reading:** `[crates/apollo_mempool/src/mempool.rs](../crates/apollo_mempool/src/mempool.rs)`

### Mempool P2P (`apollo_mempool_p2p`)

Two sub-components:

- **Propagator** — receives the instruction to propagate a tx and sends it to peers over [libp2p](https://libp2p.io/), a modular peer-to-peer networking library.
- **Runner** — the libp2p event loop; receives txs from peers and injects them back through the Gateway.

**Depends on:** Gateway (client, for inbound peer txs), Class Manager (client), Proof Manager (client)
**Start reading:** `[crates/apollo_mempool_p2p/src/propagator.rs](../crates/apollo_mempool_p2p/src/propagator.rs)`, `[runner.rs](../crates/apollo_mempool_p2p/src/runner.rs)`

---

## Block Production

### Consensus Manager (`apollo_consensus_manager`)

Runs the consensus algorithm (based on [Tendermint](https://tendermint.com/), a [Byzantine Fault Tolerant](https://en.wikipedia.org/wiki/Byzantine_fault) consensus protocol). It drives the Batcher to propose blocks or validate proposals from other sequencers. It also signs votes via the Signature Manager.

This is an *active* (WrapperServer) component with its own run loop — it is the "conductor" that tells the Batcher what to do and when.

**Depends on:** Batcher (client), State Sync (client), Class Manager (client), Signature Manager (client), L1 Gas Price Provider (client), Proof Manager (client), Config Manager (client)
**Start reading:** `[crates/apollo_consensus_manager/src/consensus_manager.rs](../crates/apollo_consensus_manager/src/consensus_manager.rs)`

### Batcher (`apollo_batcher`)

The workhorse of block production. When instructed by Consensus:

1. Calls `get_txs` on Mempool to fetch pending transactions.
2. Fetches L1 events (deposits/messages) from L1 Events Provider and Gas fees from the L1 Gas Price Provider.
3. For each transaction, calls Class Manager to retrieve the compiled contract class (CASM bytecode) needed for execution.
4. Executes transactions using Blockifier (Cairo VM), producing a state diff.
5. Assembles the block body.
6. Calls `commit_block` on the Committer to compute the state root and finalise the block.
7. Notifies the Mempool to clear committed transactions.

The Batcher also supports *validating* a block proposal from another sequencer (the same execution path, but initiated by the Consensus Manager's validate flow rather than propose flow).

The dependencies split into three roles:

- **Block content sources** (provide the data that goes into a block): Mempool, L1 Events Provider, L1 Gas Price Provider, Class Manager
- **Configuration** (provide runtime parameters such as block size limits): Config Manager
- **Output recipients** (called after execution): Committer, Proof Manager, Mempool (for cleanup)

**Depends on:** Mempool (client), L1 Events Provider (client), L1 Gas Price Provider (client), Class Manager (client), Committer (client), Proof Manager (client), Config Manager (client)
**Start reading:** `[crates/apollo_batcher/src/batcher.rs](../crates/apollo_batcher/src/batcher.rs)`

### Blockifier (`blockifier`)

Not a component — it is a library (no server/client). The Batcher instantiates it directly. Blockifier runs the [Cairo VM](https://github.com/starkware-libs/cairo), applies state diffs, enforces fee rules, and handles builtin constraints. It is the EVM-equivalent execution engine for Starknet.

**No component dependencies** — it is a pure library called directly by the Batcher.
**Start reading:** `[crates/blockifier/src/transaction/](../crates/blockifier/src/transaction/)` (transaction execution), `[crates/blockifier/src/execution/](../crates/blockifier/src/execution/)` (VM execution)

### Class Manager (`apollo_class_manager`)

Stores and serves Cairo contract class definitions (both [Sierra](https://docs.starknet.io/architecture-and-concepts/smart-contracts/cairo-and-sierra/) source and compiled [CASM](https://docs.starknet.io/architecture-and-concepts/smart-contracts/cairo-and-sierra/) bytecode). Has its own [MDBX](https://libmdbx.dqdkfa.ru/)-backed storage.

**The Class Manager is the mandatory gateway to contract class bytecode.** When a declare transaction is processed, the Batcher sends the new Sierra class to the Class Manager, which calls the Sierra Compiler to produce CASM. Only once the CASM is stored and returned can the Blockifier execute any transaction that references that class. The Blockifier itself does not compile — it only executes; it always asks the Class Manager for the bytecode it needs.

**Depends on:** Sierra Compiler (client), Config Manager (client)
**Start reading:** `[crates/apollo_class_manager/src/class_manager.rs](../crates/apollo_class_manager/src/class_manager.rs)`

### Sierra Compiler (`apollo_compile_to_casm`)

Compiles [Sierra](https://docs.starknet.io/architecture-and-concepts/smart-contracts/cairo-and-sierra/) (the typed, safe IR that Cairo 1 programs compile to) into CASM (the low-level bytecode the VM executes). CPU-intensive; configured as a `ConcurrentLocalComponentServer` so multiple compilations can run in parallel.

**No component dependencies** — it is a pure computation service; it receives Sierra bytes and returns CASM bytes.
**Start reading:** `[crates/apollo_compile_to_casm/src/lib.rs](../crates/apollo_compile_to_casm/src/lib.rs)`

---

## L1 Integration

### L1 Events Provider (`apollo_l1_events`)

Stores L1→L2 messages and ETH deposits scraped from Ethereum. The Batcher queries it for events to include in blocks. The Scraper feeds it.

**Depends on:** State Sync (client, to align event history with the current chain height on startup)
**Start reading:** `[crates/apollo_l1_events/src/l1_events_provider.rs](../crates/apollo_l1_events/src/l1_events_provider.rs)`

### L1 Events Scraper (`apollo_l1_events`)

An *active* component that polls the Ethereum base layer contract for new events (deposits, messages) and writes them to the L1 Events Provider.

**Depends on:** L1 Events Provider (client)
**Start reading:** `[crates/apollo_l1_events/src/l1_scraper.rs](../crates/apollo_l1_events/src/l1_scraper.rs)`

### L1 Gas Price Provider (`apollo_l1_gas_price`)

Caches recent Ethereum gas prices. Both the Batcher and the Consensus Manager query it — the Batcher uses it when building block proposals, and the Consensus Manager uses it to set the `l1_gas_price` field in block headers.

**No component dependencies** — it is fed exclusively by the L1 Gas Price Scraper via its request-response interface.
**Start reading:** `[crates/apollo_l1_gas_price/src/l1_gas_price_provider.rs](../crates/apollo_l1_gas_price/src/l1_gas_price_provider.rs)`

### L1 Gas Price Scraper (`apollo_l1_gas_price`)

An *active* component that periodically polls Ethereum for gas prices and writes them to the L1 Gas Price Provider.

**Depends on:** L1 Gas Price Provider (client)
**Base layer contract:** `[crates/papyrus_base_layer/](../crates/papyrus_base_layer/)` — wraps the Ethereum JSON-RPC calls.
**Start reading:** `[crates/apollo_l1_gas_price/src/l1_gas_price_scraper.rs](../crates/apollo_l1_gas_price/src/l1_gas_price_scraper.rs)`

---

## Finalization

### Committer (`apollo_committer`)

Receives the state diff from the Batcher and computes the new Starknet global state root using the [Patricia Merkle Trie](https://ethereum.org/en/developers/docs/data-structures-and-encoding/patricia-merkle-trie/) (`starknet_patricia`, backed by [RocksDB](https://rocksdb.org/)). Stores the block hash. Feeds the resulting `SyncBlock` to State Sync.

**No component dependencies** — it is driven entirely by incoming `commit_block` requests from the Batcher; it does not call out to other components.
**Start reading:** `[crates/apollo_committer/src/committer.rs](../crates/apollo_committer/src/committer.rs)`

### Proof Manager (`apollo_proof_manager`)

Manages the lifecycle of ZK proofs for blocks. Coordinates with external proving infrastructure. Used by the Batcher and Consensus Manager.

**No component dependencies** — it coordinates with external proving infrastructure, not with other sequencer components.
**Start reading:** `[crates/apollo_proof_manager/src/proof_manager.rs](../crates/apollo_proof_manager/src/proof_manager.rs)`

---

## State & Sync

### State Sync (`apollo_state_sync`)

The canonical source of "current Starknet state" for components that need to read state (Gateway for nonce/balance checks, Consensus Manager for committee info). It is populated from two sources:

- **Central Sync** — syncs from Starknet's central API (Feeder Gateway) for historical blocks
- **P2P Sync** — syncs from peers in the p2p network

Also feeds new locally-produced blocks back into the state.

**Depends on:** Class Manager (client), Config Manager (client)
**Start reading:** `[crates/apollo_state_sync/src/state_sync.rs](../crates/apollo_state_sync/src/state_sync.rs)`

---

## Infrastructure / Support

### Signature Manager (`apollo_signature_manager`)

Signs consensus protocol messages (votes, proposals) with the sequencer's private key. Runs as a `ConcurrentLocalComponentServer`.

**No component dependencies** — it holds the node's private key internally and signs data passed to it directly.
**Start reading:** `[crates/apollo_signature_manager/src/signature_manager.rs](../crates/apollo_signature_manager/src/signature_manager.rs)`

### Config Manager (`apollo_config_manager`)

Holds runtime-configurable parameters (e.g., fee limits, block size limits). Components can query it for their current dynamic config. The Config Manager Runner watches for config file changes and notifies components to reload. It is local-only (no remote mode).

**No component dependencies** — it is the leaf node all other components depend on, not the other way around.
**Start reading:** `[crates/apollo_config_manager/src/config_manager.rs](../crates/apollo_config_manager/src/config_manager.rs)`

### Monitoring Endpoint (`apollo_monitoring_endpoint`)

Exposes `/monitoring/metrics` ([Prometheus](https://prometheus.io/) format) and `/monitoring/alive` (liveness). Scrapes metric values from the `apollo_metrics` registry at request time.

**Depends on:** Mempool (optional client, for queue-depth metrics), L1 Events Provider (optional client, for L1 sync metrics)
**Start reading:** `[crates/apollo_monitoring_endpoint/src/monitoring_endpoint.rs](../crates/apollo_monitoring_endpoint/src/monitoring_endpoint.rs)`

---

## Check Your Understanding

> Relevant file: `architecture/03-components.md`

1. A user deploys a new Cairo 1.0 contract. Which components are involved between "Gateway receives the tx" and "the class bytecode is ready to execute"?
2. The Batcher is about to build a block. List the three additional data sources it queries (besides the Mempool) during a block-building cycle.
3. What is the difference between the L1 Events Scraper and the L1 Events Provider?
4. The Gateway needs to validate a transaction's nonce. Which component does it call, and why does that component have that information?

---

[↑ Index](README.md) | [← Prev: 02 — The Component Model](02-component-model.md) | [→ Next: 04 — Key Data Flows](04-data-flows.md)
