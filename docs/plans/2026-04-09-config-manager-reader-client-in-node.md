# Config Manager Reader Client in Node Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace `get_config_manager_shared_client` (which returns `Arc<dyn ConfigManagerClient>`, the async RPC-style client) with `get_config_manager_reader_client` (which returns `Arc<dyn ConfigManagerReaderClient>`, the sync watch-channel reader), so that `SequencerNodeClients.config_manager_client` holds the new reader-client variant.

**Architecture:**
- The new `ConfigManagerReaderClient` trait (added in `arni/clients/reader_client_variant`) is a **sync** trait backed by a `tokio::sync::watch` channel — the config manager runner writes new values to the sender, and the reader client wraps the receiver. This avoids async overhead for config reads.
- The watch channel currently lives in `components.rs` (created at component construction time). We move it to `create_node_channels` so the receiver is available when clients are created in `create_node_clients`.
- `SequencerNodeClients` gains an `Option<SharedConfigManagerReaderClient>` field replacing the old `Client<ConfigManagerRequest, ConfigManagerResponse>` field.

**Tech Stack:** Rust, `tokio::sync::watch`, `LocalComponentReaderClient<NodeDynamicConfig>`.

---

## Context — How the Code Flows Today

```
create_node_modules (utils.rs)
  ├── create_node_channels(config) → SequencerNodeCommunication
  ├── create_node_clients(config, &mut channels) → SequencerNodeClients
  │     config_manager_client: Client<ConfigManagerRequest, ConfigManagerResponse>
  │     get_config_manager_shared_client() → Option<Arc<dyn ConfigManagerClient>>  ← old async RPC client
  ├── create_node_components(config, &clients, ...) → NodeComponents
  │     ↳ creates watch::channel(NodeDynamicConfig) internally (lines 173-174 of components.rs)
  │     ↳ calls clients.get_config_manager_shared_client() to get the RPC client for ConfigManagerRunner
  └── create_node_servers(config, &mut channels, components, &clients)
```

After this plan:
```
create_node_modules (utils.rs)
  ├── create_node_channels(config) → SequencerNodeCommunication
  │     ↳ also creates watch channel for NodeDynamicConfig, stores tx+rx
  ├── create_node_clients(config, &mut channels) → SequencerNodeClients
  │     config_manager_client: Option<SharedConfigManagerReaderClient>  ← new sync reader
  │     get_config_manager_reader_client() → Option<SharedConfigManagerReaderClient>  ← new getter
  ├── create_node_components(config, &clients, &mut channels, ...) → NodeComponents  ← channels added
  │     ↳ takes dynamic_config_tx from channels, passes to ConfigManagerRunner
  │     ↳ calls clients.get_config_manager_reader_client()  ← uses new getter
  └── create_node_servers(...)
```

---

## Task 1: Add `LocalConfigManagerReaderClient` type alias

**File:** `crates/apollo_config_manager_types/src/communication.rs`

Currently there is no `LocalConfigManagerReaderClient` type alias. The reader client trait is implemented via blanket impl for anything implementing `ComponentReaderClient<NodeDynamicConfig>`.

**Step 1:** Read the file to understand existing imports:
```
crates/apollo_config_manager_types/src/communication.rs  (lines 1–50)
```

**Step 2:** Add the import for `LocalComponentReaderClient` from infra. In the existing `use apollo_infra::component_client::{...}` block, add `LocalComponentReaderClient`.

**Step 3:** Add the type alias near the other `Local/Remote` client type aliases (around line 37):
```rust
pub type LocalConfigManagerReaderClient = LocalComponentReaderClient<NodeDynamicConfig>;
```
(Import `NodeDynamicConfig` from `apollo_node_config::node_config::NodeDynamicConfig` — it should already be imported via `apollo_node_config`.)

**Step 4:** Verify it compiles:
```bash
cargo build -p apollo_config_manager_types
```

---

## Task 2: Add the `NodeDynamicConfig` watch channel to `SequencerNodeCommunication`

**File:** `crates/apollo_node/src/communication.rs`

The `SequencerNodeCommunication` struct already holds all inter-component channels. We add the `NodeDynamicConfig` watch channel here, so both `create_node_clients` (needs rx) and `create_node_components` (needs tx) can pull from it.

**Step 1:** Read `crates/apollo_node/src/communication.rs` in full.

**Step 2:** Add imports at the top:
```rust
use apollo_node_config::node_config::NodeDynamicConfig;
use tokio::sync::watch;
```

**Step 3:** Add two fields to `SequencerNodeCommunication`:
```rust
dynamic_config_tx: Option<watch::Sender<NodeDynamicConfig>>,
dynamic_config_rx: Option<watch::Receiver<NodeDynamicConfig>>,
```

**Step 4:** Add two `take_*` methods on `SequencerNodeCommunication`:
```rust
pub fn take_dynamic_config_tx(&mut self) -> watch::Sender<NodeDynamicConfig> {
    self.dynamic_config_tx.take().expect("dynamic_config_tx already taken")
}

pub fn take_dynamic_config_rx(&mut self) -> watch::Receiver<NodeDynamicConfig> {
    self.dynamic_config_rx.take().expect("dynamic_config_rx already taken")
}
```

**Step 5:** In `create_node_channels`, create the watch channel conditioned on config manager running locally:
```rust
let (dynamic_config_tx, dynamic_config_rx) =
    match config.components.config_manager.execution_mode {
        ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled => {
            let node_dynamic_config = NodeDynamicConfig::from(config);
            let (tx, rx) = watch::channel(node_dynamic_config);
            (Some(tx), Some(rx))
        }
        _ => (None, None),
    };
```
And add `dynamic_config_tx` and `dynamic_config_rx` to the `SequencerNodeCommunication { ... }` constructor at the end of the function.

**Step 6:** Verify it compiles:
```bash
cargo build -p apollo_node
```

---

## Task 3: Change `SequencerNodeClients.config_manager_client` to the reader type

**File:** `crates/apollo_node/src/clients.rs`

This is the core change.

**Step 1:** Read the file.

**Step 2:** Replace the old imports related to config manager with the new ones:

Remove:
```rust
use apollo_config_manager_types::communication::{
    ConfigManagerRequest,
    ConfigManagerResponse,
    LocalConfigManagerClient,
    RemoteConfigManagerClient,
    SharedConfigManagerClient,
};
```

Add:
```rust
use apollo_config_manager_types::communication::{
    LocalConfigManagerReaderClient,
    SharedConfigManagerReaderClient,
};
use apollo_infra::component_client::LocalComponentReaderClient;
```

**Step 3:** Change the field in `SequencerNodeClients`:

Old:
```rust
config_manager_client: Client<ConfigManagerRequest, ConfigManagerResponse>,
```

New:
```rust
config_manager_client: Option<SharedConfigManagerReaderClient>,
```

**Step 4:** Remove `get_config_manager_shared_client` and add `get_config_manager_reader_client`:

Remove:
```rust
pub fn get_config_manager_shared_client(&self) -> Option<SharedConfigManagerClient> {
    get_shared_client!(self, config_manager_client)
}
```

Add:
```rust
pub fn get_config_manager_reader_client(&self) -> Option<SharedConfigManagerReaderClient> {
    self.config_manager_client.clone()
}
```

**Step 5:** In `create_node_clients`, replace the `create_client!` block for config_manager:

Remove:
```rust
let config_manager_client = create_client!(
    &config.components.config_manager.execution_mode,
    LocalConfigManagerClient,
    RemoteConfigManagerClient,
    channels.take_config_manager_tx(),
    &config.components.config_manager.remote_client_config,
    &config.components.config_manager.url,
    config.components.config_manager.port,
    &CONFIG_MANAGER_INFRA_METRICS.get_local_client_metrics(),
    &CONFIG_MANAGER_INFRA_METRICS.get_remote_client_metrics()
);
```

Add:
```rust
let config_manager_client = match config.components.config_manager.execution_mode {
    ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled => {
        let rx = channels.take_dynamic_config_rx();
        let reader_client = LocalConfigManagerReaderClient::new(rx);
        Some(Arc::new(reader_client) as SharedConfigManagerReaderClient)
    }
    _ => None,
};
```

Note: Add `use std::sync::Arc;` if not already present. Add the `ReactiveComponentExecutionMode` import if not already present (it is, via `apollo_node_config::component_execution_config::ReactiveComponentExecutionMode`).

**Step 6:** Update the struct constructor in `create_node_clients` — `config_manager_client` is now `Option<...>`, no change needed to the field name itself.

Also remove the `CONFIG_MANAGER_INFRA_METRICS` import since it's no longer needed:
```rust
use apollo_config_manager::metrics::CONFIG_MANAGER_INFRA_METRICS;
```

**Step 7:** Verify it compiles:
```bash
cargo build -p apollo_node
```

---

## Task 4: Pass `channels` to `create_node_components` and update it

**Files:** `crates/apollo_node/src/utils.rs` and `crates/apollo_node/src/components.rs`

**Step 1:** In `utils.rs`, update the call to `create_node_components` to pass `&mut channels`:

Old:
```rust
let components = create_node_components(config, &clients, prometheus_handle, cli_args).await;
```
New:
```rust
let components = create_node_components(config, &clients, &mut channels, prometheus_handle, cli_args).await;
```

**Step 2:** In `components.rs`, update the function signature:

Old:
```rust
pub async fn create_node_components(
    config: &SequencerNodeConfig,
    clients: &SequencerNodeClients,
    prometheus_handle: Option<PrometheusHandle>,
    cli_args: Vec<String>,
) -> NodeComponents {
```
New:
```rust
pub async fn create_node_components(
    config: &SequencerNodeConfig,
    clients: &SequencerNodeClients,
    channels: &mut SequencerNodeCommunication,
    prometheus_handle: Option<PrometheusHandle>,
    cli_args: Vec<String>,
) -> NodeComponents {
```

Add the import `use crate::communication::SequencerNodeCommunication;` to `components.rs` if not already there.

**Step 3:** In `components.rs`, inside the `LocalExecutionWithRemoteDisabled` match arm for config_manager (around line 162), replace the watch channel creation and client retrieval:

Old:
```rust
let (dynamic_config_tx, dynamic_config_rx) =
    watch::channel(node_dynamic_config.clone());
let config_manager_client = clients
    .get_config_manager_shared_client()
    .expect("Config Manager client should be available");
let config_manager_runner = ConfigManagerRunner::new(
    config_manager_config.clone(),
    config_manager_client,
    dynamic_config_tx,
    dynamic_config_rx,
    cli_args,
);
```

New:
```rust
let dynamic_config_tx = channels.take_dynamic_config_tx();
let config_manager_reader_client = clients
    .get_config_manager_reader_client()
    .expect("Config Manager reader client should be available");
let config_manager_runner = ConfigManagerRunner::new(
    config_manager_config.clone(),
    config_manager_reader_client,
    dynamic_config_tx,
    cli_args,
);
```

**Step 4:** Remove the `use tokio::sync::watch;` import from `components.rs` if it's no longer used (check first).

**Step 5:** Verify it compiles:
```bash
cargo build -p apollo_node
```

---

## Task 5: Update `ConfigManagerRunner` to remove `_dynamic_config_rx`

**File:** `crates/apollo_config_manager/src/config_manager_runner.rs`

The `_dynamic_config_rx` field was added temporarily (see the TODO comment) to keep the watch channel alive until the reader client held it. Now that `SequencerNodeClients` holds `LocalConfigManagerReaderClient` (which wraps the receiver), this field is no longer needed.

Also, the `config_manager_client` field type needs to change from `SharedConfigManagerClient` (async RPC client) to `SharedConfigManagerReaderClient` (sync watch reader). This field is used in `update_config` to call `set_node_dynamic_config`. With the new design, the runner WRITES via `dynamic_config_tx` and the clients READ via the receiver. So the runner no longer needs to call `set_node_dynamic_config` on the async client — it just sends to the watch channel.

Wait — review the TODO comment on the block that calls `config_manager_client.set_node_dynamic_config`:
```rust
// TODO(Arni): Remove this block once config_manager_client is removed from the runner.
match self.config_manager_client.set_node_dynamic_config(node_dynamic_config).await {
```

This TODO says to remove `config_manager_client` from the runner entirely. Since the runner now writes to the watch channel, the async RPC client call to `ConfigManager` is redundant (the `ConfigManager` struct is also being phased out).

**Step 1:** Read `crates/apollo_config_manager/src/config_manager_runner.rs`.

**Step 2:** Remove `config_manager_client` and `_dynamic_config_rx` from the `ConfigManagerRunner` struct:

Old:
```rust
pub struct ConfigManagerRunner {
    config_manager_config: ConfigManagerConfig,
    config_manager_client: SharedConfigManagerClient,
    dynamic_config_tx: Sender<NodeDynamicConfig>,
    _dynamic_config_rx: Receiver<NodeDynamicConfig>,
    cli_args: Vec<String>,
}
```
New:
```rust
pub struct ConfigManagerRunner {
    config_manager_config: ConfigManagerConfig,
    dynamic_config_tx: Sender<NodeDynamicConfig>,
    cli_args: Vec<String>,
}
```

**Step 3:** Remove `config_manager_client` and `dynamic_config_rx` from `ConfigManagerRunner::new`:

Old signature:
```rust
pub fn new(
    config_manager_config: ConfigManagerConfig,
    config_manager_client: SharedConfigManagerClient,
    dynamic_config_tx: Sender<NodeDynamicConfig>,
    dynamic_config_rx: Receiver<NodeDynamicConfig>,
    cli_args: Vec<String>,
) -> Self {
```
New signature:
```rust
pub fn new(
    config_manager_config: ConfigManagerConfig,
    dynamic_config_tx: Sender<NodeDynamicConfig>,
    cli_args: Vec<String>,
) -> Self {
```

Update the body accordingly (remove the fields from `Self { ... }`).

**Step 4:** Remove the `set_node_dynamic_config` async call block from `update_config`. Replace with just a success log:

Old (inside `update_config`):
```rust
debug!("Successfully sent node dynamic config to the channel");
// TODO(Arni): Remove this block once config_manager_client is removed from the runner.
match self.config_manager_client.set_node_dynamic_config(node_dynamic_config).await {
    Ok(()) => {
        info!("Successfully updated dynamic config");
        Ok(())
    }
    Err(e) => Err(format!("Failed to update dynamic config: {:?}", e).into()),
}
```
New:
```rust
info!("Successfully updated dynamic config");
Ok(())
```

**Step 5:** Remove unused imports from `config_manager_runner.rs`:
- `use apollo_config_manager_types::communication::SharedConfigManagerClient;`
- `use tokio::sync::watch::{Receiver, Sender};` → change to `use tokio::sync::watch::Sender;`
- `use tracing::{debug, error, info};` → change to `use tracing::{error, info};` if `debug!` is no longer used

**Step 6:** Update the tests in `config_manager_runner_tests.rs` — `ConfigManagerRunner::new` calls will need to drop the `config_manager_client` and `dynamic_config_rx` arguments. The mock client and `dynamic_config_rx.clone()` in the tests should be removed.

**Step 7:** Verify:
```bash
SEED=0 cargo test -p apollo_config_manager
```

---

## Task 6: Final build verification

**Step 1:** Build all affected crates:
```bash
cargo build -p apollo_config_manager_types
cargo build -p apollo_config_manager
cargo build -p apollo_node
```

**Step 2:** Run tests:
```bash
SEED=0 cargo test -p apollo_config_manager
SEED=0 cargo test -p apollo_node
```

**Step 3:** Run clippy on changed crates:
```bash
cargo clippy -p apollo_config_manager_types
cargo clippy -p apollo_config_manager
cargo clippy -p apollo_node
```

**Step 4:** Commit:
```bash
gt modify -m "apollo_node,apollo_config_manager,apollo_config_manager_types: use config manager reader client in node"
```

---

## What is NOT in scope

- Removing dead code: `SharedConfigManagerClient`, `ConfigManagerClient` async trait, `LocalConfigManagerClient`, `RemoteConfigManagerClient` type aliases, or `CONFIG_MANAGER_INFRA_METRICS` if still used elsewhere. Leave these for a follow-up.
- Removing `ConfigManager` struct itself (there's a TODO for that too).
- Updating any usages of `get_config_manager_shared_client` in test code — if they break, fix them; if they don't exist, don't worry.
