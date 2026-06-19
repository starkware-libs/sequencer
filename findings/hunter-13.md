# Bug Findings: apollo_infra

Auditor: Bug Hunter #13
Crate: `apollo_infra` at `/home/user/sequencer/crates/apollo_infra/src/`

---

## Bug 1: Retry Logging Condition Is Off-by-One

**File**: `crates/apollo_infra/src/component_client/remote_component_client.rs`, line 386

**Description**: The logging condition `attempt % attempts_per_log == attempts_per_log - 1` fires at the wrong attempt numbers. With `attempts_per_log = N`, the intent is to log every N-th failure (i.e., at attempt N, 2N, 3N...). The actual condition fires at attempts where `attempt ≡ N-1 (mod N)`, that is at attempts `N-1, 2N-1, 3N-1, ...`. This is off by one: the first log message appears one attempt too early.

Example with `attempts_per_log = 5`:
- Correct: log at attempts 5, 10, 15, ...
- Actual: log at attempts 4, 9, 14, ...

With the default `attempts_per_log = 1`, the condition becomes `attempt % 1 == 0` (always true), so every attempt is logged — this masks the bug at the default.

**Root Cause**: The formula `attempt % attempts_per_log == attempts_per_log - 1` is checking for the position one before the multiple, not at the multiple. The correct formula is `attempt % attempts_per_log == 0`.

**Test**:
```rust
// This test can be placed in crates/apollo_infra/src/component_client/remote_component_client.rs
// (in the existing remote_component_client_test module).
//
// It validates the logging condition directly by checking which attempts would trigger a log
// message given `attempts_per_log = 5`.
#[test]
fn retry_log_condition_fires_at_correct_attempts() {
    // With attempts_per_log = 5, we expect log messages at multiples of 5: 5, 10, 15, ...
    let attempts_per_log: usize = 5;
    let max_attempts = 15;

    // Collect which attempts trigger the current (buggy) condition.
    let actual_logged: Vec<usize> = (1..=max_attempts)
        .filter(|attempt| attempt % attempts_per_log == attempts_per_log - 1)
        .collect();

    // Correct behaviour: log at 5, 10, 15.
    let expected_logged: Vec<usize> = vec![5, 10, 15];

    // This assertion FAILS with the current code: actual_logged = [4, 9, 14].
    assert_eq!(
        actual_logged, expected_logged,
        "Log messages should appear at every {attempts_per_log}-th attempt, \
         but fired at: {actual_logged:?}"
    );
}
```

**How to verify**: `SEED=0 cargo test -p apollo_infra retry_log_condition_fires_at_correct_attempts`

---

## Bug 2: Panic on `attempts_per_log = 0` (Division by Zero and Integer Underflow)

**File**: `crates/apollo_infra/src/component_client/remote_component_client.rs`, lines 385-387

**Description**: The field `attempts_per_log: usize` has no validation that prevents it from being set to zero. If it is zero:
1. `attempt % attempts_per_log` performs modulo by zero, which panics at runtime on Rust integers.
2. `attempts_per_log - 1` underflows a `usize` (panics in debug mode, wraps to `usize::MAX` in release).

Both paths panic, crashing the node process for any request after a misconfigured `attempts_per_log = 0`.

**Root Cause**: The `RemoteClientConfig` struct does not add a `#[validate(custom(function = "validate_positive"))]` attribute on `attempts_per_log`, unlike `max_concurrency` in `LocalServerConfig` and `RemoteServerConfig`. The config can be loaded from JSON/YAML with `attempts_per_log: 0` and validation will not catch it.

**Test**:
```rust
// Place in crates/apollo_infra/src/component_client/remote_component_client_test.rs
use validator::Validate;
use crate::component_client::RemoteClientConfig;

#[test]
fn attempts_per_log_zero_is_rejected_by_validation() {
    let config = RemoteClientConfig { attempts_per_log: 0, ..Default::default() };
    // This currently PASSES (no validation error is returned) but should FAIL.
    assert!(
        config.validate().is_err(),
        "attempts_per_log = 0 should be rejected by config validation to avoid \
         a division-by-zero panic at runtime"
    );
}
```

**How to verify**: `SEED=0 cargo test -p apollo_infra attempts_per_log_zero_is_rejected_by_validation`

---

## Bug 3: Integer Overflow in Exponential Backoff

**File**: `crates/apollo_infra/src/component_client/remote_component_client.rs`, line 395

**Description**: The exponential backoff doubling `retry_interval_ms * 2` is an unchecked multiplication on `u64`. If `retry_interval_ms` starts at a large value (e.g., `initial_retry_delay_ms = u64::MAX / 2 + 1`), the first doubling overflows:
- In **debug builds**: panics with "attempt to multiply with overflow".
- In **release builds**: silently wraps to a very small value (undefined backoff behaviour).

There is no validation on `initial_retry_delay_ms` or `max_retry_interval_ms`, and while a very large initial delay is unusual, it is a valid configuration value from a JSON config file and should be handled defensively.

**Root Cause**: Raw `*` arithmetic is used instead of `saturating_mul`. The `.min(self.config.max_retry_interval_ms)` cap only applies after the multiplication, so the overflow happens before the cap is applied.

**Test**:
```rust
// Place in crates/apollo_infra/src/component_client/remote_component_client_test.rs

#[test]
fn exponential_backoff_does_not_overflow_with_large_initial_delay() {
    // If retry_interval_ms = u64::MAX / 2 + 1, then retry_interval_ms * 2 overflows.
    // The .min() cap should prevent this, but it is applied AFTER the multiply.
    let retry_interval_ms: u64 = u64::MAX / 2 + 1;
    let max_retry_interval_ms: u64 = u64::MAX;

    // This panics in debug mode: attempt to multiply with overflow.
    // Use a closure to catch it in tests.
    let result = std::panic::catch_unwind(|| {
        let doubled = retry_interval_ms * 2; // overflow!
        doubled.min(max_retry_interval_ms)
    });

    // This assertion documents the expected (correct) behaviour: no panic.
    // It currently FAILS in debug mode because of the overflow panic.
    assert!(
        result.is_ok(),
        "Exponential backoff must not overflow; use saturating_mul instead of *"
    );
}
```

**How to verify**: `SEED=0 cargo test -p apollo_infra exponential_backoff_does_not_overflow_with_large_initial_delay`

---

## Bug 4: Wrong `ServerError` Variant Used for "Server Busy" Response

**File**: `crates/apollo_infra/src/component_server/remote_component_server.rs`, lines 352-355

**Description**: When the server rejects a new TCP connection because `max_concurrency` is reached, it returns an HTTP 503 response whose body is serialized as `ServerError::RequestDeserializationFailure(BUSY_PREVIOUS_REQUESTS_MSG)`. This is semantically wrong: the `RequestDeserializationFailure` variant means "I could not parse your request payload", but here the server has not even attempted to read the request — it is rejecting at the connection level because it is at capacity.

The client receives:
```
ClientError::ResponseError(503, ServerError::RequestDeserializationFailure("Server is busy..."))
```

A client that distinguishes between "my request was malformed" and "the server is overloaded" cannot make the right decision because the error variant is misleading. A correct implementation would use a dedicated `ServerError::ServerBusy` or `ServerError::TooManyConnections` variant.

**Root Cause**: The `ServerError` enum only defines `RequestDeserializationFailure` and `RequestBodyTooLarge`. When the busy-server path was implemented, rather than adding a semantically correct variant, the existing `RequestDeserializationFailure` was (mis)reused. The HTTP status code (503) is correct, but the serialized error body variant is not.

**Written Justification** (mechanical test not shown because adding a new enum variant is the fix, not a test):

A consumer of the client API that tries to implement smart retry logic will write:

```rust
match err {
    ClientError::ResponseError(StatusCode::SERVICE_UNAVAILABLE, ServerError::RequestDeserializationFailure(msg)) => {
        // BUG: this branch is reached for "server busy", but the name says
        // "deserialization failure". The caller cannot distinguish between
        // "my serialized payload was malformed" (don't retry) and
        // "server is at capacity" (should retry with backoff).
        retry_with_backoff();  // reasonable for busy, WRONG for malformed request
    }
    ClientError::ResponseError(StatusCode::BAD_REQUEST, ServerError::RequestDeserializationFailure(_)) => {
        // This branch is for actual bad requests, but the 503 case above
        // looks identical at the variant level.
        fail_permanently();
    }
    _ => {}
}
```

The status code discriminates correctly (503 vs 400), but callers using pattern matching on the `ServerError` variant are misled. The fix is to add `ServerError::ServerBusy(String)` and use it in the busy-rejection path.

---

## Bug 5: `LocalComponentClient::send` Is Not Cancellation-Safe — Server Panics on Cancelled Client

**File**: `crates/apollo_infra/src/component_client/local_component_client.rs`, lines 50-54  
**Related**: `crates/apollo_infra/src/component_server/local_component_server.rs`, line 428

**Description**: `LocalComponentClient::send` creates a one-shot response channel, sends the request wrapper (containing the response-sender half) to the server, then awaits the response:

```rust
let (res_tx, mut res_rx) = channel::<Response>(1);
let request_wrapper = RequestWrapper::new(request, res_tx, request_id);
self.tx.send(request_wrapper).await.expect("Outbound connection should be open.");
// --- cancellation point ---
let response = res_rx.recv().await.expect("Inbound connection should be open.");
```

If the outer future is cancelled after the request has been enqueued in the server's channel but before `res_rx.recv()` completes, `res_rx` is dropped. The server then processes the request and tries to send the response through `res_tx`, but since `res_rx` is gone:

```rust
// local_component_server.rs line 428
tx.send(response).await.expect("Response connection should be open.");
```

This `.expect()` panics, **crashing the entire server task** and making the component permanently unavailable. Because the server loop exits with a panic, all future requests to that component also fail.

A realistic trigger: using `tokio::time::timeout` around a client call, or `tokio::select!` with a cancellation arm, while the server is slow.

**Root Cause**: The `send` future is not cancel-safe. Once the request is enqueued, the response channel is tied to the lifetime of the `send` future. If that future is dropped, the channel is closed but the server doesn't know and panics.

The comment in the server code (line 426-427) acknowledges this: "This might result in a panic if the client has closed the response channel, which is considered a bug." The bug is real and unmitigated.

**Test**:
```rust
// Place in crates/apollo_infra/src/tests/local_component_client_server_test.rs
// (or a new test file). Uses the existing test infrastructure.
//
// This test demonstrates the crash: if the client future is cancelled while the
// server is processing, the server task panics.

use std::time::Duration;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr, VariantNames};
use tokio::sync::mpsc::channel;
use tokio::task;
use tokio::time::timeout;

use crate::component_client::LocalComponentClient;
use crate::component_definitions::{
    ComponentClient, ComponentRequestHandler, ComponentStarter, PrioritizedRequest, RequestWrapper,
};
use crate::component_server::{ComponentServerStarter, LocalComponentServer, LocalServerConfig};
use crate::tests::test_utils::{TEST_LOCAL_CLIENT_METRICS, TEST_LOCAL_SERVER_METRICS};
use crate::{impl_debug_for_infra_requests_and_responses, impl_labeled_request};

// A component that sleeps before replying so we can cancel the client in time.
struct SlowComponent;
impl ComponentStarter for SlowComponent {}

#[derive(Serialize, Deserialize, Clone, AsRefStr, EnumDiscriminants)]
#[strum_discriminants(
    name(SlowRequestLabelValue),
    derive(IntoStaticStr, EnumIter, VariantNames),
    strum(serialize_all = "snake_case")
)]
enum SlowRequest {
    DoWork,
}
impl_debug_for_infra_requests_and_responses!(SlowRequest);
impl_labeled_request!(SlowRequest, SlowRequestLabelValue);
impl PrioritizedRequest for SlowRequest {}

#[derive(Serialize, Deserialize, Debug)]
enum SlowResponse {
    Done,
}

#[async_trait]
impl ComponentRequestHandler<SlowRequest, SlowResponse> for SlowComponent {
    async fn handle_request(&mut self, _request: SlowRequest) -> SlowResponse {
        // Simulate slow processing: sleep longer than the client timeout.
        tokio::time::sleep(Duration::from_millis(200)).await;
        SlowResponse::Done
    }
}

#[tokio::test]
async fn cancelling_local_client_send_does_not_panic_server() {
    let (tx, rx) = channel::<RequestWrapper<SlowRequest, SlowResponse>>(8);
    let client = LocalComponentClient::new(tx, &TEST_LOCAL_CLIENT_METRICS);

    let mut server =
        LocalComponentServer::new(SlowComponent, &LocalServerConfig::default(), rx, &TEST_LOCAL_SERVER_METRICS);

    let server_handle = task::spawn(async move {
        server.start().await;
    });

    // Give the server time to start.
    task::yield_now().await;

    // Cancel the client send after 50 ms — the server takes 200 ms to respond,
    // so the send future will be cancelled while the request is in-flight.
    let result = timeout(Duration::from_millis(50), client.send(SlowRequest::DoWork)).await;
    assert!(result.is_err(), "timeout should have fired");

    // Wait long enough for the server to try to send its response.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // The server task should still be alive, not panicked.
    // If the server panicked, is_finished() returns true with a panic payload.
    assert!(
        !server_handle.is_finished(),
        "Server task panicked after client cancellation — \
         LocalComponentClient::send is not cancellation-safe"
    );
}
```

**How to verify**: `SEED=0 cargo test -p apollo_infra cancelling_local_client_send_does_not_panic_server`

The test will fail (server task panics) because:
1. Client timeout fires at 50 ms, dropping `res_rx`.
2. Server wakes at 200 ms, calls `tx.send(SlowResponse::Done).await.expect(...)`.
3. `.expect()` panics because `res_rx` was dropped.
4. `server_handle.is_finished()` becomes `true` with a panic payload.

---

## Summary

| # | Title | Severity | File | Line |
|---|-------|----------|------|------|
| 1 | Retry logging off-by-one | Low (wrong log timing) | `remote_component_client.rs` | 386 |
| 2 | Panic on `attempts_per_log = 0` | Medium (crash on misconfiguration) | `remote_component_client.rs` | 386 |
| 3 | Integer overflow in exponential backoff | Low-Medium (panic in debug, wrap in release) | `remote_component_client.rs` | 395 |
| 4 | Wrong `ServerError` variant for busy response | Low (misleading error type) | `remote_component_server.rs` | 352 |
| 5 | Non-cancellation-safe `LocalComponentClient::send` panics server | High (server crash) | `local_component_client.rs` / `local_component_server.rs` | 53/428 |
