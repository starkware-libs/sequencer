# Stack Review: Config Manager Reader Client

**Date**: 2026-04-09  
**Stack top**: `arni/gateway/use_config_manager_client`  
**Reviewed by**: Arnon (self-review, collegial style)

---

## Branch 1: `clients/reader_client_variant`

**Files**: `apollo_infra/src/component_client/definitions.rs`, `apollo_config_manager_types/src/communication.rs`

**Overall**: Clean. No concerns.

- `RpcClient<Request, Response>` and `ReaderClient<T>` type aliases are well-documented and clear.
- `Client<Request, Response, T>` generalisation is minimal — adds one variant, one method, one bound.
- `LocalConfigManagerReaderClient` type alias in `communication.rs` is the right place.

**Fixed**: `get_local_read_only_client()` panic message was copied verbatim from `get_local_client()` ("should be set for inbound remote connections") — that message only makes sense for the RPC path. Changed to "Expected LocalReadOnlyClient, got a different client variant."

---

## Branch 2: `apollo_node/add_config_manager_reader_client_to_node`

**Files**: `communication.rs`, `clients.rs`, `components.rs` (apollo_node), `config_manager_runner.rs`, `config_manager_runner_tests.rs`

**Overall**: Clean. The middle-state design is correct.

### Watch channel ownership

The `_dynamic_config_rx` field was in `ConfigManagerRunner` to keep the sender alive (a `watch::Sender` becomes closed when all receivers are dropped). This branch removes it because `SequencerNodeClients::reader_config_manager_client` now holds the receiver, keeping the channel alive for the lifetime of the node. The invariant is preserved, just with better ownership.

### `get_config_manager_reader_client()` — the `_ => panic!` arm

```rust
_ => panic!("config_manager reader client must be LocalReadOnlyClient or Disabled"),
```

`reader_config_manager_client` is typed as `ReaderClient<NodeDynamicConfig>` which is `Client<(), (), NodeDynamicConfig>`. The `Local` and `Remote` variants of this type are `LocalComponentClient<(), ()>` and `RemoteComponentClient<(), ()>` — syntactically valid but semantically meaningless. They can only arise from a programmer error in initialisation, never from a runtime condition. The panic is correct and the message is clear.

### `LocalReadOnlyClient(_) => None` in `get_shared_client!` macro

```rust
macro_rules! get_shared_client {
    ($self:ident, $client_field:ident) => {{
        match &$self.$client_field {
            Client::Local(local_client) => Some(Arc::new(local_client.clone())),
            Client::LocalReadOnlyClient(_) => None,  // ← this arm
            Client::Remote(remote_client) => Some(Arc::new(remote_client.clone())),
            Client::Disabled => None,
        }
    }};
}
```

This macro is only applied to `RpcClient<Req, Res>` fields (batcher, class_manager, etc.), never to `reader_config_manager_client`. The `LocalReadOnlyClient` arm is required for exhaustiveness but can never be triggered in practice — all `RpcClient` fields are initialised as `Local`, `Remote`, or `Disabled`. Returning `None` is acceptable here; it would propagate to an `expect()` downstream and fail with a clear message. A panic in the macro arm would give a slightly better error message but either is fine.

---

## Branch 3: `http_server/add_client_to_server`

**Files**: `http_server.rs`, `http_server_test.rs`, `test_utils.rs` (apollo_http_server), `components.rs` (apollo_node)

**Overall**: Correct middle state. The comparison logic is intentional — see note below.

### Shadow-mode comparison (intentional, has good reason)

```rust
fn get_dynamic_config(&self) -> HttpServerDynamicConfig {
    let config_from_reader = self.config_manager_reader_client.get_http_server_dynamic_config();
    let config = self.dynamic_config_rx.borrow().clone();  // old source
    let config_from_reader = config_from_reader.expect("...");
    if config_from_reader != config {
        warn!("Http server dynamic config from reader differs from config from dynamic_config_rx");
    }
    config  // still returns old source
}
```

This branch runs both the old polling path (`dynamic_config_poll` driven by `SharedConfigManagerClient`) and the new watch-channel path (`LocalConfigManagerReaderClient`) simultaneously, compares them, and warns if they diverge. This is intentional migration-safety: if the two sources ever disagree, the warning is visible in logs before any consumer is switched over. The old source is returned so behavior is unchanged for all consumers. This code is removed in the next branch once it has served its purpose.

This is the right pattern for this kind of infrastructure migration and is **not** redundant code without reason.

### `HttpServer` still holds the old `config_manager_client` field

The old `SharedConfigManagerClient` field stays alive to drive the `dynamic_config_poll` async task. This too is transitional — removed in Branch 4 along with the poll task.

---

## Branch 4: `http_server/remove_deprecated_config_manager_shared_client`

**Files**: `http_server.rs`, `http_server_test.rs`, `test_utils.rs`, `components.rs`

**Overall**: Clean removal. `dynamic_config_poll` and everything related to it disappears. `AppState::get_dynamic_config()` becomes a one-liner:

```rust
fn get_dynamic_config(&self) -> HttpServerDynamicConfig {
    self.config_manager_reader_client
        .get_http_server_dynamic_config()
        .expect("Failed to get http server dynamic config")
}
```

No concerns.

---

## Branch 5: `apollo_node/replace_with_config_manager_channel_client`

**Files**: `batcher`, `class_manager`, `consensus`, `consensus_manager`, `consensus_orchestrator`, `mempool`, `state_sync`, `staking` (all source + tests), `components.rs`

**Overall**: Mechanical, correct. Each change is:

1. `SharedConfigManagerClient` → `SharedConfigManagerReaderClient` on the type
2. `.await` removed from the method call
3. Mock type updated in tests

No logic changes. The `state_sync/src/lib.rs` still has `async fn update_dynamic_config` which now calls the sync `get_state_sync_dynamic_config()` — fine, `async fn` can call sync functions.

---

## Branch 6: `config_manager/remove_client_from_runner`

**Files**: `config_manager_runner.rs`, `config_manager_runner_tests.rs`, `components.rs`, `Cargo.toml`

**Overall**: Correct simplification with one bug (fixed).

### `update_config()` simplification

Before: send via channel → if changed → async RPC call → log on success.  
After: send via channel → if changed → log.

The RPC call was the entire reason the channel needed to return its boolean. Now that the RPC call is gone, the log should still be conditional. 

**Fixed**: `info!("Successfully updated dynamic config")` was firing unconditionally — even when `send_if_modified` returned `false` (config unchanged). It now fires only when config actually changed.

### Cargo.toml cleanup

Removing 6 unused crate deps from `apollo_config_manager` (`batcher_config`, `class_manager_config`, etc.) is correct — the `ConfigManager` RPC handler was the only consumer of those types.

---

## Branch 7: `config_manager/remove_config_manager`

**Files**: `communication.rs`, `config_manager.rs`, `config_manager_tests.rs` (deleted), `components.rs`, `Cargo.toml`

**Overall**: Clean deletion. The `ConfigManager` RPC component (which received async requests and forwarded them to component dynamic configs) is gone — replaced by the watch-channel broadcast pattern where `ConfigManagerRunner` writes directly to the channel and all consumers read synchronously.

The test file `config_manager_tests.rs` is also deleted — those tests covered the RPC request/response dispatch logic which no longer exists.

---

## Branch 8: `config_manager_types/remove_rpc_infrastructure`

**Files**: `communication.rs`, `Cargo.toml` (apollo_config_manager_types), `Cargo.toml` + `metrics.rs` (apollo_config_manager)

**Overall**: Clean removal of the old async RPC types:
- `ConfigManagerRequest` / `ConfigManagerResponse` enums
- `LocalConfigManagerClient` / `RemoteConfigManagerClient` type aliases
- `SharedConfigManagerClient` (the old `Arc<dyn ConfigManagerClient>` async variant)
- `ConfigManagerClientError`, `ConfigManagerClientResult`, `ConfigManagerRequestWrapper`
- `CONFIG_MANAGER_REQUEST_LABELS` and infra metrics
- `apollo_config_manager_types` dep removed from `apollo_config_manager` (no longer needed)

**Keep**: `SharedConfigManagerClient` now refers to the renamed sync client (done in Branch 9). `ConfigManagerClient` trait now refers to the sync reader trait. `LocalConfigManagerClient` now refers to the watch-channel client.

---

## Branch 9: `config_manager_types/rename_channel_client`

**Rename**:
- `ConfigManagerReaderClient` → `ConfigManagerClient`
- `SharedConfigManagerReaderClient` → `SharedConfigManagerClient`
- `LocalConfigManagerReaderClient` → `LocalConfigManagerClient`
- `MockConfigManagerReaderClient` → `MockConfigManagerClient`

**Overall**: Mechanical rename across ~30 files. No logic changes. After this branch the `ReaderClient` suffix disappears — from the consumer's perspective the type is just `ConfigManagerClient`, which is the right public API.

---

## Branch 10: `gateway/use_config_manager_client`

**Files**: `gateway.rs`, `gateway_test.rs`, `test_utils.rs` (apollo_gateway), `communication.rs`, `Cargo.toml` (apollo_config_manager_types), `components.rs` (apollo_node)

**Overall**: New consumer correctly wired up.

### Dynamic config source changed for `native_classes_whitelist`

Before:
```rust
.instantiate_validator(self.config.dynamic_config.native_classes_whitelist.clone())
```

After:
```rust
let native_classes_whitelist = self
    .config_manager_client
    .get_gateway_dynamic_config()
    .expect("Failed to get gateway dynamic config")
    .native_classes_whitelist;
.instantiate_validator(native_classes_whitelist)
```

The whitelist now comes from the watch channel rather than a static field on `self.config`. This means the gateway picks up whitelist changes at runtime without a restart — which is the whole point. The `expect` is safe because the watch channel read cannot fail (it returns whatever is currently in the channel).

### Test mock setup

`default_config_manager_client()` in `gateway_test.rs` returns `GatewayDynamicConfig::default()` for all calls. Tests that construct `GenericGateway` directly add `config_manager_client: Arc::new(default_config_manager_client())` inline. The `gateway_for_benchmark()` helper in `test_utils.rs` duplicates the mock construction rather than calling `default_config_manager_client()` (which lives in a different file). This duplication is minor and acceptable.

---

## Summary of Changes Made During Review

| Issue | Branch | Fix |
|-------|--------|-----|
| `get_local_read_only_client()` panic message was copy-pasted from `get_local_client()` | Branch 1 | Updated to "Expected LocalReadOnlyClient, got a different client variant." |
| `info!("Successfully updated dynamic config")` fired unconditionally, even when config was unchanged | Branch 6 | Captured return value of `send_if_modified` and made the log conditional |
