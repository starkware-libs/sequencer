# Restructure Config Manager Graphite Stack

**Date**: 2026-04-09  
**Goal**: Replace two broken branches (`arni/apollo_node/wire_config_manager_channel` and `arni/apollo_node/move_config_manager_to_top`) with a working watch-channel implementation, inserting the reader client variant into the stack.

**Problem**: The existing stack references `LocalComponentChannelClient` which was never defined in `apollo_infra`, causing compilation failures.

---

## Key Types Reference

Before you start, understand these types already defined in the lower stack:

**From `infra/create_component_clients_with_channel`:**
- `LocalComponentReaderClient<T>` (in `apollo_infra::component_client`) — wraps `watch::Receiver<T>`, implements `ComponentReaderClient<T>`
- `ComponentReaderClient<T>` trait (in `apollo_infra::component_definitions`)
- `SharedConfigManagerReaderClient = Arc<dyn ConfigManagerReaderClient>` (in `apollo_config_manager_types::communication`)
- `ConfigManagerReaderClient` sync trait in `apollo_config_manager_types::communication`

**From `clients/reader_client_variant` (to be amended):**
- `RpcClient<Request, Response> = Client<Request, Response, ()>` (in `apollo_infra::component_client::definitions`)
- `ReaderClient<T> = Client<(), (), T>` (in `apollo_infra::component_client::definitions`)
- `Client<Request, Response, T>` enum with `LocalReadOnlyClient(LocalComponentReaderClient<T>)` variant

---

## Step 0: Amend `clients/reader_client_variant`

**Action**: Remove premature changes to `crates/apollo_node/src/clients.rs` from this commit. Those belong in Step 1.

**Keep in this commit:**

`crates/apollo_infra/src/component_client/definitions.rs`:
- Add type aliases: `RpcClient<Request, Response>` and `ReaderClient<T>`
- Expand `Client<Request, Response>` to `Client<Request, Response, T>` with new `LocalReadOnlyClient(LocalComponentReaderClient<T>)` variant
- Add `get_local_read_only_client()` method

`crates/apollo_config_manager_types/src/communication.rs`:
- Add: `pub type LocalConfigManagerReaderClient = LocalComponentReaderClient<NodeDynamicConfig>;`

**Remove from this commit:**
- All changes to `crates/apollo_node/src/clients.rs` (move to Step 1)

**Update commit message to:**
```
apollo_infra,apollo_config_manager_types: add reader client variant for watch-channel-based clients
```

---

## Step 1: Create `apollo_node/add_config_manager_reader_client_to_node`

**Action**: NEW branch, stacked on amended `clients/reader_client_variant`

This introduces watch-channel infrastructure and the reader client field. No consumers change yet.

**File: `crates/apollo_node/src/communication.rs`**

Add to `SequencerNodeCommunication` struct:
```rust
dynamic_config_tx: Option<watch::Sender<NodeDynamicConfig>>,
dynamic_config_rx: Option<watch::Receiver<NodeDynamicConfig>>,
```

Add methods:
```rust
pub fn take_dynamic_config_tx(&mut self) -> watch::Sender<NodeDynamicConfig> {
    self.dynamic_config_tx.take().expect("dynamic_config_tx already taken")
}

pub fn take_dynamic_config_rx(&mut self) -> watch::Receiver<NodeDynamicConfig> {
    self.dynamic_config_rx.take().expect("dynamic_config_rx already taken")
}
```

In `create_node_channels()`, create the watch channel only for `LocalExecutionWithRemoteDisabled` (ConfigManager panics on other modes):
```rust
let (dynamic_config_tx, dynamic_config_rx) =
    match config.components.config_manager.execution_mode {
        ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled => {
            let (tx, rx) = watch::channel(NodeDynamicConfig::from(config));
            (Some(tx), Some(rx))
        }
        _ => (None, None),
    };
```

**File: `crates/apollo_node/src/clients.rs`**

Change all existing `Client<Req, Res>` fields to `Client<Req, Res, ()>` (add `()` as type param), **EXCEPT** `config_manager_client` which stays `Client<ConfigManagerRequest, ConfigManagerResponse, ()>` (old RPC field unchanged).

Add new field:
```rust
reader_config_manager_client: ReaderClient<NodeDynamicConfig>,
```

Update `get_shared_client!` macro to handle `LocalReadOnlyClient` variant (return `None`).

Keep `get_config_manager_shared_client()` unchanged (old RPC path still active).

Add new method:
```rust
pub fn get_config_manager_reader_client(&self) -> Option<SharedConfigManagerReaderClient> {
    match &self.reader_config_manager_client {
        Client::LocalReadOnlyClient(client) => {
            let reader_client: SharedConfigManagerReaderClient = Arc::new(client.clone());
            Some(reader_client)
        }
        Client::Disabled => None,
        _ => panic!("config_manager reader client must be LocalReadOnlyClient or Disabled"),
    }
}
```

In `create_node_clients()`, initialize `reader_config_manager_client`:
```rust
reader_config_manager_client: match config.components.config_manager.execution_mode {
    ReactiveComponentExecutionMode::LocalExecutionWithRemoteDisabled => {
        Client::LocalReadOnlyClient(LocalConfigManagerReaderClient::new(
            channels.take_dynamic_config_rx(),
        ))
    }
    _ => Client::Disabled,
},
```

**File: `crates/apollo_node/src/utils.rs`**

Pass `&mut channels` to `create_node_components()`.

**File: `crates/apollo_node/src/components.rs`**

- Add `channels: &mut SequencerNodeCommunication` parameter to `create_node_components()`
- Replace `watch::channel(node_dynamic_config)` creation with `channels.take_dynamic_config_tx()`
- Keep using `clients.get_config_manager_shared_client()` for `ConfigManagerRunner` (old RPC path still works)
- Remove `dynamic_config_rx` argument from `ConfigManagerRunner::new()` call
- Remove `use tokio::sync::watch;` import (no longer created here)

**File: `crates/apollo_config_manager/src/config_manager_runner.rs`**

Remove:
- `_dynamic_config_rx: Receiver<NodeDynamicConfig>` field
- `dynamic_config_rx: Receiver<NodeDynamicConfig>` parameter from `::new()`
- TODO comment referencing this field

The channel stays alive via `SequencerNodeClients::reader_config_manager_client`.

**File: `crates/apollo_config_manager/src/config_manager_runner_tests.rs`**

Drop `dynamic_config_rx` argument from all `ConfigManagerRunner::new()` call sites.

**Commit message:**
```
apollo_node,apollo_config_manager: add config manager reader client to node
```

**Invariant after this step:** `get_config_manager_shared_client()` still works for old consumers. `get_config_manager_reader_client()` is available but unused. No consumer has changed.

---

## Step 2: Update `http_server/add_client_to_server`

**Action**: Replace broken `LocalComponentChannelClient` references with `SharedConfigManagerReaderClient`

**File: `crates/apollo_http_server/src/http_server.rs`**

- Import `SharedConfigManagerReaderClient` (not `SharedConfigManagerChannelClient`)
- Add `config_manager_reader_client: SharedConfigManagerReaderClient` to `AppState`
- In `get_dynamic_config()`, use `self.config_manager_reader_client.get_http_server_dynamic_config()` to read config
- Add `config_manager_reader_client: SharedConfigManagerReaderClient` parameter to `HttpServer::new()` and `create_http_server()`

**File: `crates/apollo_http_server/src/http_server_test.rs`**

Update mock setup to use reader client approach.

**File: `crates/apollo_http_server/src/test_utils.rs`**

Update test utilities to use `MockConfigManagerReaderClient`.

**File: `crates/apollo_node/src/components.rs`**

- Pass `clients.get_config_manager_reader_client().expect(...)` to `create_http_server()`
- Remove any `_config_manager_channel_client` placeholder

**Commit message:**
```
apollo_http_server: add config manager reader client to http server
```

---

## Step 3: Update `http_server/remove_deprecated_config_manager_shared_client`

**Action**: Remove old `SharedConfigManagerClient` field from http_server

**File: `crates/apollo_http_server/src/http_server.rs`**

- Remove `config_manager_client: SharedConfigManagerClient` field
- Update `get_dynamic_config()` to read exclusively from `config_manager_reader_client` (no `dynamic_config_rx` fallback)

**Commit message:**
```
apollo_http_server: remove use of deprecated config manager shared client
```

---

## Step 4: Update `apollo_node/replace_with_config_manager_channel_client`

**Action**: Switch all consumer crates from async `SharedConfigManagerClient` to sync `SharedConfigManagerReaderClient`. **Drop all http_server file changes** (already handled in steps 2–3).

Remove `.await` from all `get_*_dynamic_config()` calls. These consumers now use the sync reader client:

- `apollo_batcher` (`batcher.rs`, `batcher_test.rs`, `communication.rs`)
- `apollo_mempool` (`communication.rs`, `fee_mempool_test.rs`, `recorder_integration_test.rs`, `benches/utils.rs`)
- `apollo_consensus` (`manager.rs`, `manager_test.rs`)
- `apollo_consensus_orchestrator` (`sequencer_consensus_context.rs`, `sequencer_consensus_context_test.rs`, `test_utils.rs`)
- `apollo_consensus_manager` (`consensus_manager.rs`, `consensus_manager_test.rs`)
- `apollo_class_manager` (`class_manager.rs`, `class_manager_test.rs`, `communication.rs`)
- `apollo_state_sync` (`lib.rs`, `runner/mod.rs`, `test.rs`)
- `apollo_staking` (`staking_manager.rs`, `staking_manager_test.rs`)

**File: `crates/apollo_node/src/components.rs`**

- Consumers now get client via `clients.get_config_manager_reader_client()`

**File: `crates/apollo_config_manager_types/Cargo.toml`**

- No new dependencies needed

**Commit message:**
```
apollo_node: switch all consumers to config manager reader client
```

---

## Step 5: Update `config_manager/remove_client_from_runner`

**Action**: Remove old async client field from runner (watch channel removal already done in Step 1)

**File: `crates/apollo_config_manager/src/config_manager_runner.rs`**

Remove:
- `config_manager_client: SharedConfigManagerClient` field
- `config_manager_client: SharedConfigManagerClient` parameter from `::new()`
- Async config update block in `update_config()` — replace with just `info!` + `Ok(())`

**File: `crates/apollo_node/src/components.rs`**

- Stop calling `get_config_manager_shared_client()` when constructing `ConfigManagerRunner`

**File: `crates/apollo_config_manager/src/config_manager_runner_tests.rs`**

- Drop `config_manager_client` argument from all `ConfigManagerRunner::new()` call sites
- Update test assertions

**Commit message:**
```
apollo_config_manager: remove config manager client from runner
```

---

## Step 6: Restack `config_manager/remove_config_manager`

**Action**: Restack (likely compiles without changes)

This removes the `ConfigManager` RPC server class entirely. Verify no conflicts in `components.rs` after restacking.

**Commit message:**
```
apollo_config_manager,apollo_node: remove ConfigManager
```

---

## Step 7: Update `config_manager_types/remove_rpc_infrastructure`

**Action**: Remove all old RPC types (except reader client types)

**File: `crates/apollo_config_manager_types/src/communication.rs`**

At this point, the file contains:
- **Remove:** Old async RPC types (`ConfigManagerRequest`, `ConfigManagerResponse`, `LocalConfigManagerClient`, `RemoteConfigManagerClient`, `SharedConfigManagerClient` old async variant, async `ConfigManagerClient` trait, `ConfigManagerClientError`, `ConfigManagerClientResult`, `ConfigManagerRequestWrapper`)
- **Remove:** Old RPC macros (`handle_all_response_variants`, `impl_debug_for_infra_requests_and_responses`) if only used by config_manager
- **Keep:** `SharedConfigManagerReaderClient`, `ConfigManagerReaderClient` trait, `LocalConfigManagerReaderClient`
- **Don't remove:** `SharedConfigManagerChannelClient` (it was never added in this approach — no need to remove something that doesn't exist)

**File: `crates/apollo_config_manager_types/Cargo.toml`**

- Remove `paste` dep if only used by the old RPC macros (check: `impl_reader_client_getter!` may still need it — keep if so)

**Commit message:**
```
apollo_config_manager_types: remove deprecated config manager RPC infrastructure
```

---

## Step 8: Update `config_manager_types/rename_channel_client`

**Action**: Rename reader client types globally

This renames:
- `ConfigManagerReaderClient` → `ConfigManagerClient`
- `SharedConfigManagerReaderClient` → `SharedConfigManagerClient`
- `LocalConfigManagerReaderClient` → `LocalConfigManagerClient`

Apply the rename across all crates that reference these types. Use your editor's "Find and Replace" feature or a script for consistency.

**Do NOT rename the branch itself** — only update the commit.

**Commit message:**
```
apollo_config_manager_types: rename ConfigManagerReaderClient to ConfigManagerClient
```

---

## Step 9: Update `gateway/use_config_manager_client`

**Action**: Adjust references after Step 8 rename

After step 8, `SharedConfigManagerReaderClient` is now `SharedConfigManagerClient`. Update any references in the gateway.

**Commit message:**
Update as needed, typically:
```
apollo_gateway: use config manager client
```

---

## Execution Checklist

- [ ] **Step 0:** Amend `clients/reader_client_variant` — remove `clients.rs` changes
- [ ] **Step 1:** Create new branch `apollo_node/add_config_manager_reader_client_to_node` with watch-channel infra
- [ ] **Step 2:** Update `http_server/add_client_to_server` — use `SharedConfigManagerReaderClient`
- [ ] **Step 3:** Update `http_server/remove_deprecated_...` — follow new approach
- [ ] **Step 4:** Update consumer branch — use reader client, drop http_server files
- [ ] **Step 5:** Update runner branch — remove async client field
- [ ] **Step 6:** Restack config_manager removal
- [ ] **Step 7:** Update infrastructure removal — no `SharedConfigManagerChannelClient`
- [ ] **Step 8:** Rename reader client types globally
- [ ] **Step 9:** Update gateway branch
- [ ] Build and test: `cargo build` and `SEED=0 cargo test` on key crates
- [ ] Push stack with Graphite: `gt stack sync`

---

## Summary: New Stack Order

```
main
add_sender_channel_to_config_manager_runner
infra/create_component_clients_with_channel
clients/reader_client_variant (amended)
apollo_node/add_config_manager_reader_client_to_node (new)
http_server/add_client_to_server (updated)
http_server/remove_deprecated_config_manager_shared_client (updated)
apollo_node/replace_with_config_manager_channel_client (updated, http_server files removed)
config_manager/remove_client_from_runner (updated)
config_manager/remove_config_manager (restacked)
config_manager_types/remove_rpc_infrastructure (updated)
config_manager_types/rename_channel_client (updated)
gateway/use_config_manager_client (updated)
```

---

## Key Invariants

1. **Step 1 changes no consumers**: Reader client field is available but not used yet.
2. **Step 4 drops http_server files**: They're already handled in steps 2–3. Don't duplicate changes.
3. **No `SharedConfigManagerChannelClient` ever exists**: It was never added in this approach. Don't try to remove it in step 7.
4. **Rename types once, in step 8**: Apply the rename across the entire codebase to avoid partial migrations.
5. **Always use absolute type names in error messages**: e.g., `Arc<dyn ConfigManagerClient>`, not just `Reader`.
