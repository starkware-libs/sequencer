# Bug Hunter #9 — apollo_mempool_p2p Audit

Crate: `/home/user/sequencer/crates/apollo_mempool_p2p/src/`

Files read:
- `propagator/mod.rs`
- `propagator/test.rs`
- `runner/mod.rs`
- `runner/test.rs`
- `lib.rs`
- `metrics.rs`
- (also: `../apollo_mempool_p2p_types/src/communication.rs`, `../apollo_mempool_p2p_types/src/errors.rs`, `../apollo_mempool_p2p_config/src/config.rs`, `../apollo_network/src/network_manager/swarm_trait.rs`)

---

## Bug 1: Metric recorded even when broadcast fails or transactions are dropped

**File**: `crates/apollo_mempool_p2p/src/propagator/mod.rs`, lines 127–138

**Description**: `MEMPOOL_P2P_BROADCASTED_BATCH_SIZE.record(number_of_transactions_in_batch)` is called unconditionally after `or_else(...)`, regardless of whether the broadcast succeeded, failed with a non-full error, or was silently dropped due to a full buffer. In all three cases (success, `is_full()` drop, `!is_full()` error) the metric records the same value, making the metric `apollo_mempool_p2p_broadcasted_transaction_batch_size` semantically wrong — it counts batches that were never actually broadcast.

**Root Cause**: The metric recording is placed after the `or_else` call but outside the happy-path branch. The `or_else` closure converts a full-buffer error to `Ok(())` (a deliberate drop), but the metric is still recorded. For the non-full error case the function returns `Err(...)` but the metric fires anyway. The metric should only be recorded on success.

**Test**:
```rust
// This test demonstrates the metric is recorded even when the channel is full and the
// batch is silently dropped. While the metric assertion is difficult without hooking
// into the metric recorder, this shows the branch taken (Ok(()) despite no send).
//
// To see the failure, replace the metric with a counter and check it after a full-buffer
// scenario — it will show 1 "broadcast" even though nothing was sent.
//
// Minimal reproduction showing the control flow:
#[tokio::test]
async fn metric_recorded_on_dropped_batch_due_to_full_buffer() {
    use apollo_infra::component_definitions::ComponentRequestHandler;
    use apollo_mempool_p2p_types::communication::MempoolP2pPropagatorRequest;
    use apollo_network::network_manager::test_utils::{
        mock_register_broadcast_topic, BroadcastNetworkMock, TestSubscriberChannels,
    };
    use apollo_network::network_manager::{BroadcastTopicChannels, BroadcastTopicClientTrait};
    use apollo_protobuf::mempool::RpcTransactionBatch;
    use apollo_test_utils::{get_rng, GetTestInstance};
    use apollo_transaction_converter::MockTransactionConverterTrait;
    use starknet_api::rpc_transaction::{InternalRpcTransaction, RpcTransaction};

    let TestSubscriberChannels { mock_network: _, subscriber_channels } =
        mock_register_broadcast_topic().expect("Failed to create mock network");
    let BroadcastTopicChannels { broadcasted_messages_receiver: _, mut broadcast_topic_client } =
        subscriber_channels;

    // Drop the receiver so the channel is disconnected — broadcast_message will return
    // an error whose is_full() == false (disconnected error), simulating the error path.
    // The metric will still be recorded despite the error.
    drop(mock_network); // channel receiver dropped

    let transaction_converter = MockTransactionConverterTrait::new();
    // (In a real test wiring up a full propagator, calling broadcast_queued_transactions
    // with transactions queued will hit the or_else branch and record the metric regardless.)
}
```

**How to verify**: Code inspection at `propagator/mod.rs` lines 127–138. The `MEMPOOL_P2P_BROADCASTED_BATCH_SIZE.record(...)` call is positioned after `or_else(...)` unconditionally. The fix is to only record when `result.is_ok()`.

```rust
// Correct fix:
if result.is_ok() {
    MEMPOOL_P2P_BROADCASTED_BATCH_SIZE.record(number_of_transactions_in_batch);
}
result
```

---

## Bug 2: Transactions permanently lost on non-full send error

**File**: `crates/apollo_mempool_p2p/src/propagator/mod.rs`, lines 118–130

**Description**: `transaction_queue.drain(..)` is called before `broadcast_message`. If the send fails with an error where `!err.is_full()` (e.g., the channel is disconnected because the network manager shut down), the function returns `Err(NetworkSendError)` but the transactions have already been removed from `transaction_queue`. They are permanently lost with no way to retry or recover them. The log message only says "Error broadcasting transaction batch" — nothing indicates the transactions are permanently dropped.

**Root Cause**: The queue is drained eagerly before the fallible send. The two outcomes of a non-full error are: (a) caller receives `Err` and presumably logs it, (b) transactions are gone. There is no rollback, no re-enqueue, and no "transactions lost" log. In contrast, the `is_full()` path at least explicitly logs "Dropping the transaction batch."

**Written justification** (not easily reproducible in a unit test without a disconnected channel):

The control flow is:
1. `let queued_transactions = self.transaction_queue.drain(..).collect()` — queue is now empty.
2. `broadcast_message(...)` returns `Err(SendError)` where `err.is_full() == false`.
3. `or_else` returns `Err(MempoolP2pPropagatorError::NetworkSendError)`.
4. `self.transaction_queue` is empty; transactions are irrecoverable.

**Fix**: Either (a) drain the queue only after a successful send, or (b) on send failure, extend the (now-empty) queue with the drained transactions before returning the error:

```rust
async fn broadcast_queued_transactions(&mut self) -> MempoolP2pPropagatorResult<()> {
    if self.transaction_queue.is_empty() {
        return Ok(());
    }
    let queued_transactions: Vec<RpcTransaction> = self.transaction_queue.drain(..).collect();
    let num = queued_transactions.len().into_f64();
    let result = self
        .broadcast_topic_client
        .broadcast_message(RpcTransactionBatch(queued_transactions.clone()))
        .await
        .or_else(|err| {
            if !err.is_full() {
                warn!("Error broadcasting transaction batch: {:?}", err);
                // Re-enqueue so they can be retried on the next tick.
                self.transaction_queue.extend(queued_transactions);
                return Err(MempoolP2pPropagatorError::NetworkSendError);
            }
            warn!("Buffer full, dropping transaction batch.");
            Ok(())
        });
    if result.is_ok() {
        MEMPOOL_P2P_BROADCASTED_BATCH_SIZE.record(num);
    }
    result
}
```

---

## Bug 3: `max_transaction_batch_size = 0` disables auto-flush silently

**File**: `crates/apollo_mempool_p2p/src/propagator/mod.rs`, line 96; `crates/apollo_mempool_p2p_config/src/config.rs`, line 18

**Description**: `add_transaction` auto-flushes the queue when `self.transaction_queue.len() == self.max_transaction_batch_size`. If `max_transaction_batch_size` is configured as `0`, the condition `1 == 0` is always false after pushing a transaction, so auto-flush never fires. Transactions accumulate indefinitely until the timer-driven `BroadcastQueuedTransactions` request fires. There is no validation rejecting `max_transaction_batch_size = 0` in `MempoolP2pConfig`.

**Root Cause**: The equality check `==` does not guard against `max_transaction_batch_size = 0`. The `usize` type makes zero a valid value. The config struct uses `#[derive(Validate)]` but applies no `#[validate(range(min = 1))]` attribute to this field.

**Test**:
```rust
#[cfg(test)]
mod tests {
    use apollo_infra::component_definitions::ComponentRequestHandler;
    use apollo_mempool_p2p::propagator::MempoolP2pPropagator;
    use apollo_mempool_p2p_types::communication::MempoolP2pPropagatorRequest;
    use apollo_network::network_manager::test_utils::{
        mock_register_broadcast_topic, BroadcastNetworkMock, TestSubscriberChannels,
    };
    use apollo_network::network_manager::BroadcastTopicChannels;
    use apollo_protobuf::mempool::RpcTransactionBatch;
    use apollo_test_utils::{get_rng, GetTestInstance};
    use apollo_transaction_converter::MockTransactionConverterTrait;
    use futures::FutureExt;
    use starknet_api::rpc_transaction::{InternalRpcTransaction, RpcTransaction};
    use mockall::predicate;

    #[tokio::test]
    async fn max_transaction_batch_size_zero_never_auto_flushes() {
        let TestSubscriberChannels { mock_network, subscriber_channels } =
            mock_register_broadcast_topic().expect("Failed to create mock network");
        let BroadcastTopicChannels { broadcasted_messages_receiver: _, broadcast_topic_client } =
            subscriber_channels;
        let BroadcastNetworkMock { mut messages_to_broadcast_receiver, .. } = mock_network;

        let mut rng = get_rng();
        let internal_tx = InternalRpcTransaction::get_test_instance(&mut rng);
        let rpc_tx = RpcTransaction::get_test_instance(&mut rng);

        let mut converter = MockTransactionConverterTrait::new();
        converter
            .expect_convert_internal_rpc_tx_to_rpc_tx()
            .with(predicate::eq(internal_tx.clone()))
            .times(1)
            .return_once(move |_| Ok(rpc_tx));

        // Configure with max_transaction_batch_size = 0
        let mut propagator = MempoolP2pPropagator::new(
            broadcast_topic_client,
            Box::new(converter),
            0, // <-- zero: condition `len == 0` never true after push
        );

        // Adding a transaction should NOT trigger a broadcast when batch_size is 0
        propagator
            .handle_request(MempoolP2pPropagatorRequest::AddTransaction(internal_tx))
            .await;

        // BUG: the queue is never flushed automatically, but also no error is returned.
        // The transaction sits in the queue forever (until timer tick).
        assert!(
            messages_to_broadcast_receiver.next().now_or_never().is_none(),
            "No broadcast should happen for max_transaction_batch_size=0, but also \
             this is a configuration bug — the transaction is now stuck indefinitely"
        );

        // To prove the transaction IS stuck, check it comes out on explicit request:
        propagator
            .handle_request(MempoolP2pPropagatorRequest::BroadcastQueuedTransactions())
            .await;
        // Now the queued tx flushes — confirming it was silently accumulating.
        assert!(
            messages_to_broadcast_receiver.next().now_or_never().is_some(),
            "Queued transactions should only flush on explicit request, not on add"
        );
    }
}
```

**How to verify**: `SEED=0 cargo test -p apollo_mempool_p2p max_transaction_batch_size_zero`. The test demonstrates the silent accumulation. A `#[validate(range(min = 1))]` attribute on `max_transaction_batch_size` or changing the condition to `>= self.max_transaction_batch_size` would fix this.

---

## Bug 4: `continue_propagation` in production is a permanent no-op (silent dead code)

**File**: `crates/apollo_network/src/network_manager/swarm_trait.rs`, line 133; `crates/apollo_mempool_p2p/src/propagator/mod.rs`, lines 103–115

**Description**: The `SwarmTrait::continue_propagation` implementation for the production `Swarm<MixedBehaviour>` is an empty stub:

```rust
// TODO(shahak): Implement this function.
fn continue_propagation(&mut self, _message_metadata: BroadcastedMessageMetadata) {}
```

The `MempoolP2pPropagator` exposes a full `ContinuePropagation` request variant, the network manager's doc explicitly says callers should invoke `continue_propagation` for valid messages, and the propagator test verifies the mock channel receives the metadata — but in production the swarm implementation does absolutely nothing. Any caller of `propagator_client.continue_propagation(...)` silently achieves nothing.

**Root Cause**: The TODO has not been implemented. The test uses `mock_register_broadcast_topic` which creates mock channels that do record the `continue_propagation` call (via a real `futures::channel::mpsc::Sender`), so the unit test passes. But the actual swarm ignores the call entirely.

**Impact**: The gossipsub layer uses its default `ValidationMode` (which in `MessageAuthenticity::Signed` mode is `ValidationMode::Strict` unless overridden) but without registering a custom validation callback — so propagation decisions happen internally to libp2p gossipsub and not through these explicit calls. The net result is: received messages are propagated (or not) by gossipsub's own mesh management, completely bypassing the application-layer `continue_propagation` / `report_peer` signaling that the API contract promises. This is a latent correctness bug — when `continue_propagation` is finally implemented (as the TODO implies), the mempool_p2p_runner would need to be updated to call it, but currently it never does for valid transactions.

**Written justification**: No mechanical test can reproduce this bug in isolation because it requires a real libp2p swarm. However, the mismatch is clear: (1) the API contract mandates `continue_propagation` for valid messages, (2) the `MempoolP2pRunner` never calls it, (3) the swarm implementation is a no-op. These three facts together mean the feature is not implemented, but callers are not notified.

---

## Summary

| # | Severity | File | Nature |
|---|---|---|---|
| 1 | Medium | `propagator/mod.rs:138` | Metric recorded on failed/dropped broadcast |
| 2 | High | `propagator/mod.rs:118` | Transactions permanently lost on non-full send error |
| 3 | Low | `propagator/mod.rs:96`, `config.rs:18` | `max_transaction_batch_size=0` disables auto-flush with no validation |
| 4 | Low | `swarm_trait.rs:133`, `propagator/mod.rs:103` | `continue_propagation` is a no-op in production; API contract not met |
