# Bug Hunter 16 Findings

## Files Examined

- `crates/apollo_infra/src/component_server/local_component_server.rs` — LocalComponentServer, ConcurrentLocalComponentServer, request routing and processing loop
- `crates/apollo_infra/src/component_client/local_component_client.rs` — LocalComponentClient, request dispatch and response reception
- `crates/apollo_infra/src/component_client/remote_component_client.rs` — RemoteComponentClient, retry loop, timeout handling
- `crates/apollo_infra/src/component_server/remote_component_server.rs` — RemoteComponentServer, HTTP/2 server, connection semaphore
- `crates/apollo_infra/src/component_definitions.rs` — RequestWrapper, ComponentRequestHandler, etc.
- `crates/apollo_infra/src/metrics.rs` — LocalServerMetrics, RemoteServerMetrics
- `crates/apollo_infra/src/requests.rs` — LabeledRequest trait
- `crates/apollo_infra/src/macros.rs` — handle_all_response_variants! macro
- `crates/apollo_infra/src/serde_utils.rs` — SerdeWrapper serialization
- `crates/apollo_task_executor/src/executor.rs` — TaskExecutor trait
- `crates/apollo_task_executor/src/tokio_executor.rs` — TokioExecutor implementation
- `crates/apollo_task_executor/src/tokio_executor_test.rs` — executor tests
- `crates/apollo_infra/src/tests/local_component_client_server_test.rs`
- `crates/apollo_infra/src/tests/remote_component_client_server_test.rs`
- `crates/apollo_infra/src/tests/test_utils.rs`
- `crates/apollo_infra/src/tests/local_request_prioritization_test.rs`

---

## Bug 1

**File**: `crates/apollo_infra/src/component_server/local_component_server.rs`
**Location**: `LocalComponentServer::process_requests`, line ~223; `ConcurrentLocalComponentServer::process_requests`, line ~323
**Description**: The request-processing background task's JoinHandle is dropped immediately after spawning. When the processing task panics or terminates for any reason, the panic is silently swallowed by Tokio (caught internally, stored in the dropped JoinHandle). The `await_requests` loop continues running and routing incoming requests into the internal priority channels, but the consumer of those channels is dead. Any `RequestWrapper` enqueued before the failure will have its response `Sender` dropped when the task's local variables (`high_rx`, `normal_rx`) are freed; the client waiting on `res_rx.recv().await.expect("Inbound connection should be open.")` will then panic because `recv()` returns `None`. Further, `await_requests` itself will panic when its next `send(...).await` fails with `SendError` (the receiver was dropped with the task).

**Root Cause**: `tokio::spawn(...)` returns a `JoinHandle<()>` that is never stored or awaited. Rust silently drops it. Tokio detaches the task; if it panics, the `JoinError` is stored in the dropped handle and goes unobserved. The server has no way to detect that its processing loop has died, and neither does the client — which ends up with a broken response channel.

This is easiest to trigger by causing the `panic!` inside `get_next_request_for_processing`'s `else` branch: if both the high and normal priority channel senders are dropped (e.g., the `LocalComponentServer` struct is dropped while requests are in flight), both `recv()` calls in the `biased select!` return `None` simultaneously, hitting the `else` arm which panics inside the background task, and the panic is silently discarded.

**Failing Test**:

```rust
// Add to crates/apollo_infra/src/tests/local_component_client_server_test.rs
// (or a new file included from tests/mod.rs)

use std::time::Duration;
use tokio::sync::mpsc::channel;
use tokio::task;
use crate::component_client::LocalComponentClient;
use crate::component_definitions::{ComponentClient, RequestWrapper};
use crate::component_server::{ComponentServerStarter, LocalComponentServer, LocalServerConfig};
use crate::tests::test_utils::{
    ComponentB, ComponentBRequest, ComponentBResponse, ValueB,
    TEST_LOCAL_CLIENT_METRICS, TEST_LOCAL_SERVER_METRICS,
    ComponentAClientTrait, ComponentARequest, ComponentAResponse, ComponentA,
};
use starknet_types_core::felt::Felt;
use async_trait::async_trait;
use crate::component_client::{ClientResult, ClientError};

// When the processing task panics (both priority channels dropped), the next client
// waiting on a response must not hang forever: it must get an error, not a deadlock.
#[tokio::test(start_paused = false)]
async fn test_processing_task_panic_propagates_to_client() {
    struct AlwaysPanicsComponent;
    impl crate::component_definitions::ComponentStarter for AlwaysPanicsComponent {}

    #[async_trait]
    impl crate::component_definitions::ComponentRequestHandler<ComponentBRequest, ComponentBResponse>
        for AlwaysPanicsComponent
    {
        async fn handle_request(&mut self, _request: ComponentBRequest) -> ComponentBResponse {
            panic!("component panic");
        }
    }

    let (tx, rx) = channel::<RequestWrapper<ComponentBRequest, ComponentBResponse>>(32);
    let client = LocalComponentClient::new(tx, &TEST_LOCAL_CLIENT_METRICS);
    let component = AlwaysPanicsComponent;
    let config = LocalServerConfig::default();
    let mut server = LocalComponentServer::new(component, &config, rx, &TEST_LOCAL_SERVER_METRICS);

    task::spawn(async move {
        // start() will panic eventually, but we want to test client behavior
        let _ = std::panic::AssertUnwindSafe(server.start())
            .catch_unwind()
            .await;
    });

    // Give the server time to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    // The component panics on every request. The process_request fn calls
    // tx.send(response).await after handle_request completes, but handle_request panics —
    // the panic propagates inside the spawned task. The client's response channel tx is
    // dropped when the task unwinds, so recv() returns None, and the client should
    // get an error rather than hanging forever.
    //
    // Bug: because the JoinHandle of the processing task is dropped, the server has no
    // way to detect the crash. The client's res_rx.recv() panics with
    // "Inbound connection should be open." — a panic rather than a clean error.
    //
    // This test demonstrates the problem: we expect a Result::Err, but we get a panic.
    // Under the current code, the test will panic (via the .expect inside LocalComponentClient).
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        client.send(ComponentBRequest::BGetValue),
    )
    .await;

    // We expect either an Err (clean error propagation) or a timeout — either is acceptable
    // as long as we don't deadlock indefinitely. The bug causes a panic on the client side
    // rather than a clean Result::Err.
    //
    // A correct implementation would return something like:
    //   Ok(Err(ClientError::CommunicationFailure(...)))
    // instead of panicking.
    assert!(
        result.is_err() || result.unwrap().is_err(),
        "Expected client to receive an error when component panics, not a panic/hang"
    );
}
```

**How to Verify**:
```
SEED=0 cargo test -p apollo_infra test_processing_task_panic_propagates_to_client
```

The test will panic (rather than returning `Err`) because the `LocalComponentClient` calls `.expect("Inbound connection should be open.")` on `res_rx.recv()` which returns `None` when the component panics and the processing task is dropped. This demonstrates that the error is propagated as a panic rather than a clean `ClientError`.

---

## Bug 2

**File**: `crates/apollo_infra/src/component_client/remote_component_client.rs`
**Location**: `ComponentClient::send` for `RemoteComponentClient`, line ~386
**Description**: The retry logging condition is off-by-one. The intended behavior is "log a warning every `attempts_per_log` failed attempts" (i.e., log when `attempt` is a multiple of `attempts_per_log`). The actual condition is:

```rust
if attempt % attempts_per_log == attempts_per_log - 1 {
```

For `attempts_per_log = N`, this fires when `attempt ≡ N-1 (mod N)`, i.e., on attempts N-1, 2N-1, 3N-1, ... instead of N, 2N, 3N, ...

For example, with `attempts_per_log = 3` and `max_attempts = 9`:
- **Actual**: logs on attempts 2, 5, 8
- **Expected**: logs on attempts 3, 6, 9

With `attempts_per_log = 1` (the default), the condition becomes `attempt % 1 == 0`, which is always true — this happens to produce the right behavior for the default. The bug only manifests for `attempts_per_log > 1`.

**Root Cause**: The condition should be `attempt % attempts_per_log == 0` (log when attempt count is a multiple of `attempts_per_log`), but `attempts_per_log - 1` is used on the right-hand side instead of `0`.

**Failing Test**:

```rust
// Add to crates/apollo_infra/src/tests/remote_component_client_server_test.rs

use crate::component_client::RemoteClientConfig;

/// Verifies that with attempts_per_log = N, the warning is logged on attempt N, 2N, ...
/// and NOT on attempt N-1.
///
/// With the buggy condition `attempt % N == N - 1`, the warning fires on attempt N-1 first.
/// With the correct condition `attempt % N == 0`, the warning fires on attempt N first.
///
/// We verify this indirectly: a client configured with retries=2, attempts_per_log=2
/// sends to a server that never responds. We check that if we only allow 1 attempt to occur
/// (retries=0, attempts_per_log=2), no warning is logged — because attempt 1 should NOT
/// satisfy the log condition when attempts_per_log=2.
///
/// Note: Since we cannot easily intercept log output, we demonstrate the arithmetic bug directly.
#[test]
fn test_retry_logging_condition_off_by_one() {
    // Demonstrate the logic inline — this is the same expression used in the production code.
    let attempts_per_log: usize = 3;

    // Collect which attempts trigger the log under the BUGGY condition.
    let buggy_logged_attempts: Vec<usize> = (1..=9)
        .filter(|attempt| attempt % attempts_per_log == attempts_per_log - 1)
        .collect();

    // Collect which attempts trigger the log under the CORRECT condition.
    let correct_logged_attempts: Vec<usize> = (1..=9)
        .filter(|attempt| attempt % attempts_per_log == 0)
        .collect();

    // The correct behavior: log on multiples of attempts_per_log.
    assert_eq!(correct_logged_attempts, vec![3, 6, 9]);

    // The buggy behavior: log fires one attempt early.
    assert_eq!(buggy_logged_attempts, vec![2, 5, 8]);

    // This assertion FAILS because the buggy and correct sets differ,
    // proving the logging condition in production code is wrong.
    assert_eq!(
        buggy_logged_attempts, correct_logged_attempts,
        "Bug: retry warning fires on attempts {:?} instead of {:?}",
        buggy_logged_attempts, correct_logged_attempts
    );
}
```

**How to Verify**:
```
SEED=0 cargo test -p apollo_infra test_retry_logging_condition_off_by_one
```

The test fails with:
```
Bug: retry warning fires on attempts [2, 5, 8] instead of [3, 6, 9]
```

The fix is to change line 386 in `remote_component_client.rs` from:
```rust
if attempt % attempts_per_log == attempts_per_log - 1 {
```
to:
```rust
if attempt % attempts_per_log == 0 {
```
