[↑ Index](../README.md) | [← Prev: 06 — Block Production](06-block-production.md)

---

# 07 — Integration Tests (Deep Dive)

How the integration test suite works: the two test harnesses, how nodes and resources are provisioned, how deployment configurations are selected, and how individual tests are written.

---

## Overview

The integration tests live in `crates/apollo_integration_tests/`. They verify end-to-end behaviour — transactions flowing from HTTP ingestion through to committed blocks — across multiple running sequencer nodes.

There are **two distinct test harnesses** that serve different goals:

| Harness | Entry point | Node execution | Deployment coverage |
|---------|-------------|---------------|---------------------|
| **FlowTestSetup** | `tests/*.rs` (cargo nextest) | In-process (same runtime) | Consolidated only (2 nodes) |
| **IntegrationTestManager** | `src/bin/sequencer_node_end_to_end_integration_tests/` (binary) | Out-of-process (spawned subprocesses) | Consolidated + Distributed + Hybrid |

The flow tests are fast, run as ordinary `cargo nextest` test cases, and cover transaction-type scenarios. The integration tests exercise real process boundaries and are run as compiled binaries (typically in CI with `scripts/run_tests.py --command integration`).

---

## Source Layout

```
crates/apollo_integration_tests/
├── src/
│   ├── lib.rs                       Module exports
│   ├── utils.rs                     Config builders, test scenarios, end_to_end_flow()
│   ├── flow_test_setup.rs           FlowTestSetup: 2-node in-process harness
│   ├── integration_test_manager.rs  IntegrationTestManager: multi-deployment harness
│   ├── node_component_configs.rs    Deployment-variant config factories
│   ├── executable_setup.rs          ExecutableSetup: per-service config + monitoring
│   ├── state_reader.rs              Storage initialization, account/contract funding
│   ├── storage.rs                   Path management + CustomPaths
│   ├── monitoring_utils.rs          Metrics polling helpers (await_block, verify_txs)
│   ├── sequencer_simulator_utils.rs Test client for simulator mode
│   └── bin/
│       ├── sequencer_node_setup.rs
│       ├── sequencer_simulator.rs
│       ├── dummy_eth_to_strk_oracle.rs
│       ├── dummy_recorder.rs
│       └── sequencer_node_end_to_end_integration_tests/
│           ├── integration_test_positive_flow.rs
│           ├── integration_test_restart_flow.rs
│           ├── integration_test_restart_service_*.rs
│           ├── integration_test_revert_flow.rs
│           └── integration_test_central_and_p2p_sync_flow.rs
└── tests/
    ├── bootstrap_declare.rs
    ├── declare_tx_flow_test.rs
    ├── deploy_account_and_invoke_flow_test.rs
    ├── funding_txs_flow_test.rs
    ├── l1_to_l2_message_flow_test.rs
    ├── multiple_account_txs_flow_test.rs
    ├── reverted_l1_handler_tx_flow_test.rs
    ├── test_custom_cairo0_txs.rs
    ├── test_custom_cairo1_txs.rs
    └── test_many.rs
```

---

## Harness 1: FlowTestSetup (In-Process)

### What it is

`FlowTestSetup` spins up **two sequencer nodes inside the same Tokio runtime**. All nodes communicate via in-memory channels — no TCP sockets, no process boundaries. This makes tests fast and deterministic.

```
┌──────────────────────────────────────────────────────────┐
│  tokio multi_thread runtime (3 threads)                  │
│                                                          │
│   sequencer_0  ─┐                                        │
│   sequencer_1  ─┴─── in-memory channels ──► consensus   │
│                       (gossipsub topics)                 │
│                                                          │
│   Anvil (L1 base layer, embedded)                        │
└──────────────────────────────────────────────────────────┘
```

Both nodes are full proposing nodes: each has a Gateway, HTTP server, Mempool, and can propose blocks. A validation-only node is not yet present in the flow test setup (tracked as a TODO in the codebase).

### Construction sequence

`FlowTestSetup::new_from_tx_generator()` takes `instance_indices: [u16; 3]` — one shared index plus one per sequencer — and proceeds in this order:

1. **Allocate shared ports** via `AvailablePorts::new(test_unique_index, shared_instance_index)` — picks a port range based on the test ID so parallel tests don't collide.
2. **Create consensus network configs** — calls `create_connected_network_configs(NUM_OF_SEQUENCERS + 1)` to get a fully-connected gossipsub mesh (`NUM_OF_SEQUENCERS = 2`), plus the raw `consensus_proposals_channels` for the TxCollector.
3. **Create mempool P2P configs** — one per node; both nodes are proposing and need Mempool P2P networking.
4. **Create state sync configs** — two configs, one per node, with independent ports.
5. **Spin up Anvil** — starts an embedded Ethereum L1, then calls `make_block_history_on_anvil()` to mine 10 blocks (giving the L1 gas price scraper something to read).
6. **Spawn TxCollector task** — listens to the raw consensus proposal stream and accumulates observed transactions for assertion.
7. **Create sequencer_0 then sequencer_1 in sequence** — each gets its own `AvailablePorts` instance (derived from the test index + per-node instance index). Nodes are created sequentially to prevent port overlap.

### FlowSequencerSetup

Each sequencer is wrapped in a `FlowSequencerSetup`:

```rust
pub struct FlowSequencerSetup {
    pub node_index: usize,
    pub add_tx_http_client: HttpTestClient,
    pub storage_handles: StorageTestHandles,
    pub node_config: SequencerNodeConfig,
    pub monitoring_client: MonitoringClient,
    clients: SequencerNodeClients,  // retained to keep component channels open
}
```

Construction calls `create_node_config()` (see [Config assembly](#config-assembly)) then `create_node_modules()` (from `apollo_node`), which wires up all component servers in-process and returns the client handles. The `clients` field is kept alive to prevent dropping the component communication channels, which would crash the in-process servers.

### TxCollector

A background task that subscribes to the raw `consensus_proposals_channels` stream. For every `ProposalPart::Transactions` message it sees, it decodes the transaction hashes and records them in `AccumulatedTransactions`. Tests use this to assert that the right transactions were batched into blocks, independent of which node proposed them.

`AccumulatedTransactions` is round-aware: when a new round starts at the same height, it discards the previous round's hashes and starts fresh. When a new height starts, it promotes the current round's hashes into `accumulated_tx_hashes`.

---

## Harness 2: IntegrationTestManager (Out-of-Process)

### What it is

`IntegrationTestManager` orchestrates **multiple sequencer nodes running as separate OS processes**. It handles mixed deployments — some nodes consolidated (single process), some distributed (one process per service), some hybrid.

```
┌──────────────────────────────────────────────────────────────┐
│  IntegrationTestManager                                      │
│                                                              │
│  idle_nodes: HashMap<usize, NodeSetup>                       │
│  running_nodes: HashMap<usize, RunningNode>                  │
│  tx_generator: MultiAccountTransactionGenerator              │
│  anvil_base_layer: AnvilBaseLayer                            │
│                                                              │
│  Node 0 (Consolidated) ── one process                        │
│  Node 1 (Distributed)  ── N processes (one per service)      │
│  Node 2 (Hybrid)       ── M processes (some co-located)      │
│       ...                                                    │
└──────────────────────────────────────────────────────────────┘
```

### Lifecycle

```
new() ──► get_sequencer_setup_configs()  [build NodeSetup for each node]
      ──► AnvilBaseLayer::new()          [start L1, mine history blocks]
      ──► idle_nodes map populated

run_nodes(indices) ──► NodeSetup::run()  [spawn_run_node() per service]
                  ──► await_alive()      [poll monitoring until healthy]

[test body]

shutdown_nodes(indices) ──► AbortOnDropHandle::abort() per service
                        ──► node moved back to idle_nodes
```

`run_nodes()` calls `spawn_run_node()` for each `ExecutableSetup` in the node's service map. This launches the compiled sequencer binary with the pre-written config file path as an argument. Each service gets an `AbortOnDropHandle<()>` — when the handle is dropped or aborted, the subprocess is killed.

`await_alive()` polls every service's `/monitoring/alive` endpoint at 100 ms intervals, up to 2500 attempts (250 seconds total), before declaring the node healthy.

`shutdown_nodes()` moves the node back to `idle_nodes` after stopping it — nodes can be restarted in the same test.

### NodeSetup and RunningNode

```rust
pub struct NodeSetup {
    node_type: NodeType,                          // Consolidated | Distributed | Hybrid
    executables: HashMap<NodeService, ExecutableSetup>,
    pub add_tx_http_client: HttpTestClient,       // points to the HttpServer service
    storage_handles: StorageTestHandles,          // keeps TempDirs alive
}

pub struct RunningNode {
    node_setup: NodeSetup,
    executable_handles: HashMap<NodeService, AbortOnDropHandle<()>>,
}
```

`NodeSetup::new()` resolves which `ExecutableSetup` owns the `HttpServer` component (using `node_type.get_services_of_components(ComponentConfigInService::HttpServer)`) and uses its config to build the `add_tx_http_client`.

`RunningNode` exposes `shutdown_service()` and `run_service()` for fine-grained service-level lifecycle control — used by restart and failover tests.

---

## Deployment Configurations

Three deployment variants are defined in `node_component_configs.rs`. The configuration source of truth lives in `apollo_deployments`, which knows how each `NodeType` splits components across services.

### Consolidated

```rust
pub fn create_consolidated_component_configs() -> NodeComponentConfigs {
    NodeType::Consolidated.get_component_configs(None)
}
```

All components run in a **single process** (`NodeService::Consolidated`). No port allocation is needed for inter-component communication — everything uses in-process channels. Used by both FlowTestSetup (where it runs in-process) and IntegrationTestManager (where it runs as a single subprocess).

### Distributed

```rust
pub fn create_distributed_component_configs(
    available_ports_generator: &mut AvailablePortsGenerator,
) -> NodeComponentConfigs {
    let ports = available_ports.get_next_ports(DISTRIBUTED_NODE_REQUIRED_PORTS_NUM);
    let mut configs = NodeType::Distributed.get_component_configs(Some(ports));
    set_urls_to_localhost(configs.values_mut());
    configs
}
```

Each major component runs in its **own process**. `NodeType::Distributed.get_component_configs()` returns a `HashMap<NodeService, ComponentConfig>` with one entry per service (Batcher, StateSync, ConsensusManager, Gateway/HttpServer, etc.). `set_urls_to_localhost()` rewrites all inter-service URLs to `127.0.0.1` so that local test processes can reach each other. `DISTRIBUTED_NODE_REQUIRED_PORTS_NUM` ports are drawn from the shared pool to avoid conflicts.

### Hybrid

```rust
pub fn create_hybrid_component_configs(
    available_ports_generator: &mut AvailablePortsGenerator,
) -> NodeComponentConfigs {
    let ports = available_ports.get_next_ports(HYBRID_NODE_REQUIRED_PORTS_NUM);
    let mut configs = NodeType::Hybrid.get_component_configs(Some(ports));
    set_urls_to_localhost(configs.values_mut());
    configs
}
```

Some components are co-located in shared processes; others are separate. The exact grouping is defined in `apollo_deployments::deployments::hybrid`. Fewer ports are needed than Distributed (`HYBRID_NODE_REQUIRED_PORTS_NUM < DISTRIBUTED_NODE_REQUIRED_PORTS_NUM`).

### Choosing a deployment in a test

End-to-end tests specify counts at construction time:

```rust
let mut manager = IntegrationTestManager::new(
    3,     // num_of_consolidated_nodes
    1,     // num_of_distributed_nodes
    1,     // num_of_hybrid_nodes
    None,
    TestIdentifier::PositiveFlowIntegrationTest,
).await;
```

The manager assigns sequential indices across all nodes: consolidated nodes get indices 0..2, distributed node gets index 3, hybrid node gets index 4. Tests can run all of them or a subset.

---

## Resource Allocation

### Port allocation

Port allocation uses `AvailablePorts` and `AvailablePortsGenerator` from `apollo_infra_utils`. Each test gets a globally unique base offset derived from its `TestIdentifier` (cast to `u16`), preventing port collisions when tests run in parallel.

```
TestIdentifier → u16 test_unique_index
  └──► AvailablePorts::new(test_unique_index, instance_index)
         └──► deterministic, non-overlapping port ranges per (test, instance) pair
```

In FlowTestSetup, three `AvailablePorts` instances are created — one shared (for consensus/mempool/state-sync mesh ports) and one per sequencer (for that node's individual service ports). The three instance indices come from `EndToEndFlowArgs::instance_indices`, which defaults to `[0, 1, 2]` and can be overridden per test to avoid collisions between tests sharing a `TestIdentifier`.

### Storage paths

Each node gets independent storage for every database it owns. Paths follow this layout:

```
<temp_dir>/
└── node_<index>/
    ├── batcher/          MDBX (StateOnly scope)
    ├── state_sync/       MDBX (FullArchive scope)
    ├── class_manager/
    │   ├── class_hash_storage/   MDBX
    │   └── classes/              filesystem (Sierra/CASM files)
    ├── consensus/        MDBX (StateOnly scope)
    ├── proof_manager/    filesystem
    └── committer/        RocksDB (Patricia Merkle Tree)
```

`TempDir` handles are kept alive in `StorageTestHandles` for the lifetime of the test setup object. When the test ends and the setup is dropped, all temp directories are deleted automatically.

`CustomPaths` allows overriding the database root (used in Docker-based system tests where paths are mounted into containers).

### Storage initialization

`StorageTestSetup::new()` pre-populates every node's storage before any sequencer process starts:

1. Writes test account contracts and ERC20 fee token contracts to MDBX.
2. Deposits initial balances so accounts can pay fees.
3. Stores Cairo0 deprecated classes and Cairo1 Sierra+CASM classes in the class manager.
4. Sets the genesis state (nonces, balances, class hashes) in both batcher and state sync storage.

This avoids a bootstrapping problem: tests submit transactions immediately on startup without waiting for a genesis block from L1 or another node.

### Mock services

Two mock HTTP servers are spawned locally for every test setup:

| Service | Function | Why |
|---------|----------|-----|
| **Dummy ETH→STRK oracle** | Returns a fixed exchange rate (`DEFAULT_ETH_TO_FRI_RATE`) for any timestamp query | Tests don't need a real price feed |
| **Dummy CENDE recorder** | Returns `200 OK` for all blob/pre-confirmed write and latest-block queries | Tests don't write to a real L1 data availability layer |

`spawn_local_success_recorder(port)` returns a `(Url, JoinHandle)`. `spawn_local_eth_to_strk_oracle(port)` returns a `(UrlAndHeaders, JoinHandle)` — the `UrlAndHeaders` wrapper is needed because the L1 gas price provider can attach extra HTTP headers to oracle requests.

---

## Config Assembly

All per-node configuration is assembled by `create_node_config()` in `utils.rs`. It takes:

- `AvailablePorts` — draws ports for the HTTP server, storage reader server, class manager, etc.
- `ChainInfo` — chain ID, fee token addresses
- `StorageTestConfig` — pre-populated paths for each database
- `StateSyncConfig`, `ConsensusManagerConfig`, `MempoolP2pConfig` — pre-wired network configs
- `ComponentConfig` — which components are enabled/local vs disabled/remote
- `block_max_capacity_gas`, `validator_id`, `allow_bootstrap_txs` flags

Key decisions made inside `create_node_config()`:

- **`allow_bootstrap_txs = true`** sets `validate_non_zero_resource_bounds = false` in the Gateway and Mempool configs, allowing zero-fee bootstrap transactions.
- A macro `wrap_if_component_config_expected!` checks each component's `ComponentConfig` — if the component is running locally it wraps the config in `Some(...)`, otherwise the config field is `None`. This ensures config fields are only present in the service that actually runs that component.
- **`validation_only`** is currently hardcoded to `false` in the assembled `SequencerNodeConfig`. A validation-only node configuration is planned but not yet implemented in the integration test harness.
- **Config pointers** (`ConfigPointersMap`) propagate shared values (`chain_id`, `eth_fee_token_address`, `strk_fee_token_address`, `validator_id`, `recorder_url`, `starknet_url`) to all config fields that reference them. This is the mechanism by which a single canonical value is written once and reflected everywhere.

For distributed/hybrid nodes, the config is serialized to a JSON file (`node_integration_test_config_changes.json`) inside a `TempDir` (or a custom path). The subprocess is launched with this file path as its argument and reads its configuration from it.

---

## Consensus and Network Wiring

### Consensus

`create_consensus_manager_configs_and_channels()` calls `create_connected_network_configs(n + 1)` (one extra slot for the TxCollector observer), then `network_config_into_broadcast_channels()` to produce:
- `Vec<ConsensusManagerConfig>` — one per node, pre-connected
- `consensus_proposals_channels` — the raw stream used by TxCollector

The staking/committee configuration is embedded in each `ConsensusManagerConfig` via `create_consensus_manager_configs_from_network_configs()`:
- All `n_composed_nodes` validators get addresses `DEFAULT_VALIDATOR_ID + i`
- All validators have `can_propose: true` and equal staking weight of 1
- Consensus timeouts scaled by 2× relative to production defaults (more slack for test environments)

### Mempool P2P

`create_mempool_p2p_configs()` generates one config per node. In the flow test setup, both nodes are proposing nodes and both get Mempool P2P network configs.

### State Sync

`create_state_sync_configs()` allocates three separate port groups per node (P2P sync, RPC, storage reader server).

---

## Writing a Flow Test

Flow tests live in `tests/` and use `end_to_end_flow()`. Every flow test follows the same pattern:

```rust
// 3 threads = 2 sequencer threads + 1 test thread
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn my_test() {
    end_to_end_flow(
        EndToEndFlowArgs::new(
            TestIdentifier::MyTestId,       // unique ID → port base
            create_my_scenario(),           // what transactions to send
            GasAmount(30_000_000),          // block capacity (sierra and proving gas)
        )
        // Optional modifiers:
        .instance_indices([0, 1, 2])           // override port instance offsets
        .allow_bootstrap_txs()                 // allow zero-fee bootstrap txs
        .expecting_full_blocks()               // assert at least one block fills up
        .expecting_reverted_transactions()     // assert at least one tx reverted
    )
    .await
}
```

**3 worker threads**: one per sequencer node, plus one for the test's own logic.

### EndToEndTestScenario

```rust
pub struct EndToEndTestScenario {
    pub create_rpc_txs_fn: CreateRpcTxsFn,
    // fn(&mut MultiAccountTransactionGenerator) -> Vec<RpcTransaction>

    pub create_l1_to_l2_messages_args_fn: CreateL1ToL2MessagesArgsFn,
    // fn(&mut MultiAccountTransactionGenerator) -> Vec<L1HandlerTransaction>

    pub test_tx_hashes_fn: TestTxHashesFn,
    // fn(&[TransactionHash]) -> Vec<TransactionHash>
}
```

- `create_rpc_txs_fn` generates the transactions to submit to the Gateway.
- `create_l1_to_l2_messages_args_fn` generates L1→L2 messages to inject via Anvil.
- `test_tx_hashes_fn` filters/selects which returned tx hashes to assert on (e.g., `test_single_tx` asserts exactly one tx; `validate_tx_count` asserts a specific count).

### TestScenario trait

For `IntegrationTestManager`-based tests, scenarios implement `TestScenario`:

```rust
pub trait TestScenario {
    fn create_txs(
        &self,
        tx_generator: &mut MultiAccountTransactionGenerator,
        account_id: AccountId,
    ) -> (Vec<RpcTransaction>, Vec<L1HandlerTransaction>);

    fn n_txs(&self) -> usize;
}
```

Built-in implementations: `ConsensusTxs` (invoke + L1 handler), `DeclareTx`, `DeployAndInvokeTxs`.

### Transaction send rate

`end_to_end_flow()` sends transactions at `TPS = 3` (3 per second) with `sleep(Duration::from_millis(1000 / TPS))` between sends. At this rate with the default Tendermint timeouts, each block contains roughly 15 transactions.

All RPC transactions are sent to `sequencer_0`'s Gateway. The Mempool P2P layer propagates them to `sequencer_1`, which exercises that propagation path.

### Synchronization / assertions

`end_to_end_flow()` waits for transactions to be batched using an in-process `PrometheusHandle`:

```rust
// Both nodes run in the same process, so both increment the same metric registry.
total_expected_batched_txs_count = NUM_OF_SEQUENCERS * expected_batched_tx_hashes.len();

tokio::time::timeout(TEST_SCENARIO_TIMEOUT, async {
    loop {
        current_batched_txs_count = get_total_batched_txs_count(&global_recorder_handle);
        if current_batched_txs_count == total_expected_batched_txs_count { break; }
        sleep(TIME_BETWEEN_CHECKS).await;
    }
}).await
```

Because both sequencers run in the same process and share the same Prometheus registry, each block execution increments `BATCHED_TRANSACTIONS` once per node. The expected count is therefore `NUM_OF_SEQUENCERS × n_txs_in_scenario`.

`verify_block_hash_flow()` waits until both nodes' `GlobalRoot` marker has advanced past the target height (proving state commitment is complete), then fetches the block hash at that height from both `StorageReaderServer` endpoints and asserts they match.

The `monitoring_utils.rs` polling helpers (`await_batcher_block`, `await_sync_block`, `await_block`, etc.) are used by `IntegrationTestManager`-based tests rather than flow tests, since out-of-process nodes don't share a metric registry:

| Function | What it polls | Metric |
|----------|--------------|--------|
| `await_batcher_block(cond)` | Batcher's current building height | `BUILDING_HEIGHT` |
| `await_sync_block(cond)` | Min of all StateSync markers | `STATE_SYNC_*_MARKER` |
| `await_block(node, expected)` | Both batcher + sync reach `expected` | Combined |
| `await_txs_accepted(node, n)` | StateSync processed ≥ n txs | `STATE_SYNC_PROCESSED_TRANSACTIONS` |
| `verify_txs_accepted(node, n)` | Same, fires only once (no retry loop) | Same |
| `assert_no_reverted_txs(node)` | Reverted tx count = 0 | `REVERTED_TRANSACTIONS` |
| `get_consensus_decisions_reached(node)` | Decisions counter | `CONSENSUS_DECISIONS_REACHED_BY_CONSENSUS` |

Polling parameters: 100 ms interval, 2500 attempts (250 second ceiling) for block awaits; 1000 attempts for tx verification.

---

## Writing an Integration Test (IntegrationTestManager)

Integration tests are compiled as binaries in `src/bin/sequencer_node_end_to_end_integration_tests/`. A typical test:

```rust
#[tokio::main]
async fn main() {
    integration_test_setup("test_name").await;  // configure tracing

    let mut manager = IntegrationTestManager::new(
        3,   // consolidated
        1,   // distributed
        1,   // hybrid
        None,
        TestIdentifier::MyIntegrationTest,
    ).await;

    let all_nodes = manager.get_node_indices();
    manager.run_nodes(all_nodes.clone()).await;

    // Send transactions and verify blocks.
    manager.send_txs_and_verify(50, 2, BlockNumber(15)).await;

    // Verify all nodes agree on block hashes.
    manager.verify_block_hash_across_all_running_nodes(Some(BlockNumber(16))).await;

    manager.shutdown_nodes(all_nodes);
}
```

For restart/failover tests, `RunningNode::shutdown_service()` and `run_service()` let individual services within a node be stopped and restarted independently. `modify_config_idle_nodes()` and `modify_config_pointers_idle_nodes()` allow editing a node's config between a shutdown and the next `run_nodes()` call, enabling revert and reconfiguration tests.

---

## Check Your Understanding

> Relevant file: `architecture/deep-dives/07-integration-tests.md`

1. A flow test and an integration test both use `TestIdentifier::EndToEndFlowTest`. Why could this cause a port collision, and what mechanism prevents it in practice?
2. `end_to_end_flow()` asserts `total_expected_batched_txs_count == NUM_OF_SEQUENCERS * n_txs`. Why is it multiplied by `NUM_OF_SEQUENCERS` rather than just `n_txs`? Would this formula still be correct if the two nodes ran in separate processes?
3. When `create_node_config()` sees a component with `ComponentConfig` set to remote (not running locally), it omits that component's config (`None`). Why is this important for a distributed deployment where, say, the Batcher runs in a separate process from the HttpServer?
4. `await_block()` polls two metrics: `BUILDING_HEIGHT` from the Batcher and the minimum of several `STATE_SYNC_*_MARKER` values from StateSync. What does it mean if the Batcher metric is ahead but the StateSync metric is behind — what part of the pipeline is the bottleneck?

---

[↑ Index](../README.md) | [← Prev: 06 — Block Production](06-block-production.md)
