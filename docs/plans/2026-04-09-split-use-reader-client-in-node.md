# Split Plan: `arni/apollo_node/use_reader_client_in_node`

The current branch touches 30 files across 10 crates in one shot. This document describes how to split it into a series of small, reviewable PRs following the same pattern used in the existing stack (e.g., `arni/http_server/add_client_to_server` → `arni/http_server/remove_deprecated_config_manager_shared_client`).

---

## Context

**Parent branch:** `arni/clients/reader_client_variant`
(Already adds `ConfigManagerReaderClient` trait, `SharedConfigManagerReaderClient` type alias, `LocalConfigManagerReaderClient` type alias, `RpcClient` and `ReaderClient` type aliases.)

**What the current branch does (all at once):**
1. Adds watch channel to `SequencerNodeCommunication`
2. Wires channels into `create_node_components`
3. Replaces `config_manager_client` in `SequencerNodeClients` with the reader client
4. Simplifies `ConfigManagerRunner` (removes old RPC client + `_dynamic_config_rx`)
5. Switches all 8 consumer crates from async `SharedConfigManagerClient` to sync `SharedConfigManagerReaderClient`

**What to avoid touching here:** `apollo_http_server` — already handled by `arni/http_server/add_client_to_server` and `arni/http_server/remove_deprecated_config_manager_shared_client` in the existing stack.

---

## PR 1 — Add reader client alongside the existing client (middle state)

**Branch name:** `arni/apollo_node/add_config_manager_reader_client_to_node`
**Commit scope:** `apollo_node,apollo_config_manager`

This PR introduces the new reader client **without removing anything**. All existing consumers continue working unchanged. This is the "add alongside" pattern — analogous to `arni/http_server/add_client_to_server`.

### Changes

**`apollo_node/src/communication.rs`**
- Add `dynamic_config_tx: Option<watch::Sender<NodeDynamicConfig>>` and `dynamic_config_rx: Option<watch::Receiver<NodeDynamicConfig>>` fields to `SequencerNodeCommunication`
- Add `take_dynamic_config_tx()` and `take_dynamic_config_rx()` methods
- In `create_node_channels`: create the watch channel (conditioned on `LocalExecutionWithRemoteDisabled`) with `NodeDynamicConfig::from(config)` as initial value

**`apollo_node/src/utils.rs`**
- Pass `&mut channels` to `create_node_components`

**`apollo_node/src/clients.rs`**
- Keep `config_manager_client: Client<ConfigManagerRequest, ConfigManagerResponse, ()>` unchanged (old RPC client field)
- Add a new field: `reader_config_manager_client: ReaderClient<NodeDynamicConfig>`
- Initialize it in `create_node_clients` from `channels.take_dynamic_config_rx()`
- Keep `get_config_manager_shared_client()` unchanged
- Add `get_config_manager_reader_client() -> Option<SharedConfigManagerReaderClient>`

**`apollo_node/src/components.rs`**
- Add `channels: &mut SequencerNodeCommunication` parameter
- Replace `let (dynamic_config_tx, dynamic_config_rx) = watch::channel(...)` with `let dynamic_config_tx = channels.take_dynamic_config_tx()`
- Keep passing `config_manager_client` (old RPC) to `ConfigManagerRunner::new` as before
- Remove `dynamic_config_rx` from the `ConfigManagerRunner::new` call (the reader field in `SequencerNodeClients` now holds the rx, keeping the channel alive — this was the purpose of `_dynamic_config_rx` in the runner)

**`apollo_config_manager/src/config_manager_runner.rs`**
- Remove `_dynamic_config_rx: Receiver<NodeDynamicConfig>` field (channel stays alive via `SequencerNodeClients::reader_config_manager_client`)
- Remove `dynamic_config_rx: Receiver<NodeDynamicConfig>` parameter from `::new()`
- Remove the TODO comment that referenced this

**`apollo_config_manager/src/config_manager_runner_tests.rs`**
- Update `ConfigManagerRunner::new` call sites to drop the `dynamic_config_rx` argument

### Invariant
After this PR: `get_config_manager_shared_client()` still works for all consumers. `get_config_manager_reader_client()` is available but unused.

---

## PR 2 — Switch consumers to the reader client

Each consumer crate gets its own PR. Each is tiny (6-15 lines): swap the client type, remove `.await` from config reads.

**Do not touch `apollo_http_server`** — handled by existing branches.

### PR 2a — `apollo_batcher`
**Commit scope:** `apollo_batcher`
- `src/communication.rs`: change `SharedConfigManagerClient` → `SharedConfigManagerReaderClient`
- `src/batcher.rs`: change field type, remove `.await` from `get_batcher_dynamic_config()`
- `src/batcher_test.rs`: update mock/test setup

### PR 2b — `apollo_mempool`
**Commit scope:** `apollo_mempool`
- `src/communication.rs`: change client type
- `src/communication.rs` (impl): remove `.await`
- `src/fee_mempool_test.rs`, `src/recorder_integration_test.rs`, `benches/utils.rs`: update test setup

### PR 2c — `apollo_consensus,apollo_consensus_orchestrator`
**Commit scope:** `apollo_consensus,apollo_consensus_orchestrator`
- `apollo_consensus/src/manager.rs`: change client type, remove `.await`
- `apollo_consensus/src/manager_test.rs`: update test setup
- `apollo_consensus_orchestrator/src/sequencer_consensus_context.rs`: same
- `apollo_consensus_orchestrator/src/sequencer_consensus_context_test.rs`: same

### PR 2d — `apollo_consensus_manager`
**Commit scope:** `apollo_consensus_manager`
- `src/consensus_manager.rs`: change client type, remove `.await`
- `src/consensus_manager_test.rs`: update test setup

### PR 2e — `apollo_class_manager`
**Commit scope:** `apollo_class_manager`
- `src/class_manager.rs`: change client type, remove `.await`
- `src/class_manager_test.rs`, `src/communication.rs`: update

### PR 2f — `apollo_state_sync,apollo_staking`
**Commit scope:** `apollo_state_sync,apollo_staking`
These are small changes, group them:
- `apollo_state_sync/src/lib.rs`, `src/runner/mod.rs`, `src/test.rs`
- `apollo_staking/src/staking_manager.rs`, `src/staking_manager_test.rs`

---

## PR 3 — Remove the deprecated client (cleanup)

**Branch name:** `arni/apollo_node/remove_deprecated_config_manager_client_from_node`
**Commit scope:** `apollo_node,apollo_config_manager`

Mirror of `arni/http_server/remove_deprecated_config_manager_shared_client`.

Precondition: All consumers (PR 2a–2f) have been merged. No callers of `get_config_manager_shared_client()` remain in non-http-server code.

### Changes

**`apollo_node/src/clients.rs`**
- Remove `config_manager_client: Client<ConfigManagerRequest, ConfigManagerResponse, ()>` field
- Remove `get_config_manager_shared_client()` getter
- Remove now-unused imports: `CONFIG_MANAGER_INFRA_METRICS`, `ConfigManagerRequest`, `ConfigManagerResponse`, `LocalConfigManagerClient`, `RemoteConfigManagerClient`, `SharedConfigManagerClient`

**`apollo_node/src/components.rs`**
- Remove the `get_config_manager_shared_client()` call and stop passing it to `ConfigManagerRunner`

**`apollo_config_manager/src/config_manager_runner.rs`**
- Remove `config_manager_client: SharedConfigManagerClient` field and `::new()` parameter
- Remove the `set_node_dynamic_config` async call block from `update_config` (replace with just `info!` + `Ok(())`)
- Remove the `// TODO(Arni): Remove this block` comment

**`apollo_config_manager/src/config_manager_runner_tests.rs`**
- Update `ConfigManagerRunner::new` call sites to drop `config_manager_client`
- Replace mock client assertions with direct watch channel observation

---

## Summary

| PR | Branch | Scope | Risk |
|----|--------|-------|------|
| 1 | `arni/apollo_node/add_config_manager_reader_client_to_node` | apollo_node, apollo_config_manager | Low — no consumer changes |
| 2a | `arni/apollo_batcher/use_config_manager_reader_client` | apollo_batcher | Low |
| 2b | `arni/apollo_mempool/use_config_manager_reader_client` | apollo_mempool | Low |
| 2c | `arni/apollo_consensus/use_config_manager_reader_client` | apollo_consensus, apollo_consensus_orchestrator | Low |
| 2d | `arni/apollo_consensus_manager/use_config_manager_reader_client` | apollo_consensus_manager | Low |
| 2e | `arni/apollo_class_manager/use_config_manager_reader_client` | apollo_class_manager | Low |
| 2f | `arni/apollo_state_sync/use_config_manager_reader_client` | apollo_state_sync, apollo_staking | Low |
| 3 | `arni/apollo_node/remove_deprecated_config_manager_client_from_node` | apollo_node, apollo_config_manager | Low — precondition: 2a–2f merged |

PRs 2a–2f are independent of each other and can be submitted in parallel or in any order.
