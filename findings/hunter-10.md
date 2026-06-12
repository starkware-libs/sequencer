# Bug Hunter 10 Findings

## Files Examined

- `crates/apollo_network/src/peer_manager/mod.rs` — peer selection, round-robin, blocking/unblocking logic
- `crates/apollo_network/src/peer_manager/behaviour_impl.rs` — libp2p `NetworkBehaviour` impl for `PeerManager`, event queue polling
- `crates/apollo_network/src/peer_manager/peer.rs` — individual peer state
- `crates/apollo_network/src/sqmr/behaviour.rs` — SQMR protocol behaviour, event queue polling
- `crates/apollo_network/src/sqmr/handler.rs` — connection handler, inbound/outbound session lifecycle
- `crates/apollo_network/src/sqmr/handler/inbound_session.rs` — inbound session write-stream state machine
- `crates/apollo_network/src/sqmr/mod.rs` — SQMR types and config
- `crates/apollo_network/src/network_manager/mod.rs` — top-level network manager, routing
- `crates/apollo_network/src/misconduct_score.rs` — peer reputation scoring
- `crates/apollo_p2p_sync/src/client/block_data_stream_builder.rs` — generic block-data stream builder (used by all sync sub-protocols)
- `crates/apollo_p2p_sync/src/client/header.rs`, `state_diff.rs`, `transaction.rs`, `class.rs` — per-data-type stream builders
- `crates/apollo_p2p_sync/src/client/mod.rs` — P2pSyncClient run loop, internal block fan-out
- `crates/apollo_p2p_sync/src/server/mod.rs` — P2pSyncServer, query dispatch
- `crates/apollo_p2p_sync/src/server/utils.rs` — block number calculation

---

## Bug 1

**File**: `crates/apollo_network/src/peer_manager/behaviour_impl.rs`  
**Location**: `fn poll`, line ~187  
**Description**: `pending_events` is a `Vec<ToSwarm<…>>`. Events are pushed with `.push(…)` (appended to the end) but drained with `.pop()` (removed from the end). This gives LIFO (stack) ordering instead of FIFO (queue) ordering. The consequence is that when multiple events are accumulated in one `assign_peer_to_session` batch — e.g. when a new peer arrives and many queued sessions are re-assigned — the `SessionAssigned` events are emitted in reverse order. Downstream code that maps session IDs to peer IDs may therefore process a later assignment before an earlier one, causing the wrong peer to be used for a session.

**Root Cause**: The field is declared as `Vec`, which is a stack, not a queue. All other usages in this codebase (the SQMR `behaviour.rs`) correctly use `VecDeque` with `pop_front`. The `PeerManager` should also use `VecDeque` and `pop_front`.

Note: the second `.pop()` call at line 197 (after the sleep future fires) has the same ordering problem but is less likely to have multiple events pending at that point.

**Failing Test**:

```rust
// Place in crates/apollo_network/src/peer_manager/test.rs (or a new integration test).
//
// This test registers two sessions while no peers exist, then adds a peer.
// The PeerManager batches both SessionAssigned events in one push loop inside add_peer
// -> assign_peer_to_session -> pending_events.push(...).
// Because pop() drains the Vec in reverse (LIFO), the second session is emitted first,
// so the event order does NOT match the order the sessions were queued.

#[tokio::test]
async fn session_assigned_events_are_emitted_in_fifo_order() {
    let config = PeerManagerConfig::default();
    let mut peer_manager = PeerManager::new(config);

    // Register two sessions before any peer exists.
    let session_a = OutboundSessionId { value: 10 };
    let session_b = OutboundSessionId { value: 20 };
    peer_manager.assign_peer_to_session(session_a);
    peer_manager.assign_peer_to_session(session_b);

    // No events yet.
    assert!(peer_manager.next().now_or_never().is_none());

    // Add a peer with an active connection. PeerManager will re-assign both sessions
    // in the order [session_a, session_b] and push two SessionAssigned events onto
    // pending_events with push(). Because poll() calls pop(), it emits them LIFO:
    // session_b first, then session_a — which violates FIFO expectations.
    let peer_id = *DUMMY_PEER_ID;
    let connection_id = ConnectionId::new_unchecked(0);
    let mut peer = Peer::new(peer_id, Multiaddr::empty());
    peer.add_connection_id(connection_id);
    peer_manager.add_peer(peer);

    // Collect the two events.
    let event1 = peer_manager.next().await.unwrap();
    let event2 = peer_manager.next().await.unwrap();

    // Extract session IDs from the events.
    let session_id_1 = match event1 {
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::SessionAssigned {
            outbound_session_id, ..
        }) => outbound_session_id,
        other => panic!("Unexpected event: {:?}", other),
    };
    let session_id_2 = match event2 {
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::SessionAssigned {
            outbound_session_id, ..
        }) => outbound_session_id,
        other => panic!("Unexpected event: {:?}", other),
    };

    // Expect FIFO: session_a (registered first) emitted first, session_b second.
    // With the bug (LIFO/pop), session_b is emitted first and the assertion fails.
    assert_eq!(session_id_1, session_a, "first emitted session should be the first registered");
    assert_eq!(session_id_2, session_b, "second emitted session should be the second registered");
}
```

**How to Verify**: `SEED=0 cargo test -p apollo_network session_assigned_events_are_emitted_in_fifo_order`

---

## Bug 2

**File**: `crates/apollo_p2p_sync/src/client/block_data_stream_builder.rs`  
**Location**: `fn create_stream`, line ~125 — `last_block_number.0 - current_block_number.0`  
**Description**: The subtraction `last_block_number.0 - current_block_number.0` is performed on plain `u64` values. If `current_block_number > last_block_number`, the subtraction underflows (wraps around to a huge `u64` value). The guard `if limit == 0` on the next line is only reached when the result is zero — the guard does NOT protect against underflow. An underflow produces a very large `limit`, so `num_blocks_per_query` will likely cap it via `min(…, num_blocks_per_query)` — but only after the subtraction has already overflowed. In debug/test builds this panics; in release builds it silently computes the wrong limit.

**Root Cause**: The code assumes `last_block_number.0 >= current_block_number.0` without asserting it. The `current_block_number` advances every iteration, and `last_block_number` is re-read from storage each loop iteration. A storage writer (external to this task) could momentarily return a stale/smaller marker while the sync task has already advanced past it, or a logic error in another part of the sync could put `current_block_number` ahead of the marker.

**How to Verify** (textual, because triggering it requires mocking storage): Set `last_block_number = BlockNumber(5)` and `current_block_number = BlockNumber(6)`. Then `last_block_number.0 - current_block_number.0 = 5u64 - 6u64` — on a debug build this panics with "attempt to subtract with overflow". The fix is `last_block_number.0.saturating_sub(current_block_number.0)`.

**Failing Test**:

```rust
// This is a unit test for the arithmetic itself, which is the root cause.
// Place in a new test module inside block_data_stream_builder.rs.
#[cfg(test)]
mod arithmetic_tests {
    #[test]
    #[should_panic] // demonstrates the panic in debug mode
    fn limit_calculation_panics_on_underflow() {
        // Simulate: current_block_number has advanced past last_block_number.
        let last_block_number_u64: u64 = 5;
        let current_block_number_u64: u64 = 6;
        // In the actual code this is: min(last_block_number.0 - current_block_number.0, num_blocks_per_query)
        // In debug builds the subtraction panics; in release it wraps to u64::MAX - 0 = huge number.
        let _limit = last_block_number_u64 - current_block_number_u64;
    }
}
```

**How to Verify**: `cargo test -p apollo_p2p_sync arithmetic_tests::limit_calculation_panics_on_underflow` (debug profile).

---

## Bug 3

**File**: `crates/apollo_p2p_sync/src/client/block_data_stream_builder.rs`  
**Location**: `fn create_stream`, `tokio::select!` block starting at line ~162  
**Description**: The `select!` has two arms:
1. `res = Self::parse_data_for_block(&mut client_response_manager, …)` — reads from the network response stream
2. `block = Self::get_internal_block_at(…)` — waits for a locally produced block

`parse_data_for_block` is NOT cancel-safe: it holds intermediate parse state (for multi-part messages like `StateDiffChunk`) across `.await` points inside `client_response_manager.next().await`. When `select!` cancels the `parse_data_for_block` future because the internal-block arm fires first, the partially consumed messages are discarded. The `client_response_manager` is then dropped when `continue 'send_query_and_parse_responses` is taken, and a new query is issued — but the network-side session is still mid-stream. The peer sent some messages that were consumed from the channel but never processed, so the new query will either get confused responses or none.

**Root Cause**: `tokio::select!` cancels the non-winning future at the poll level. For a future that has already received part of a multi-message protocol unit (e.g., state diff chunks), cancellation discards the received partial data. `ClientResponsesManager` wraps a bounded channel; messages pulled out by `next()` are gone once the future is dropped.

**Confidence level**: High for state-diff and transaction data streams (multi-message per block). Lower for headers (one message per block — though there is also a `timeout` wrapper that adds another `.await` point).

**Textual Justification**: Consider a state diff that spans 3 `StateDiffChunk` messages. The `parse_data_for_block` future is polled:
- It calls `client_response_manager.next().await` → chunk 1 arrives and is stored in `result`.
- While waiting for chunk 2, an internal block for the same block number arrives.
- `select!` picks the internal-block arm. The `parse_data_for_block` future is dropped.
- Chunks 2 and 3 remain in the channel.
- The code does `continue 'send_query_and_parse_responses`, dropping `client_response_manager`.
- A new `send_new_query` is issued, obtaining a fresh `client_response_manager`. Chunks 2 and 3 from the old session are in a different channel (bound to the old `outbound_session_id`) and will never be read; they just hold a slot in the response senders map until the old session closes.
- No data loss for the block (the internal block was used), but the old peer session is orphaned and the peer manager is not notified to clean it up.

A direct test is hard to write without mocking the network layer. Textual justification is given above.

---

## Bug 4

**File**: `crates/apollo_network/src/peer_manager/behaviour_impl.rs`  
**Location**: `fn poll`, lines 190–196  
**Description**: After the sleep future for "unblocked peer" fires, the code re-assigns pending sessions. If `assign_peer_to_session` determines that all peers are still blocked (e.g., the timeout fired slightly early due to clock resolution), it pushes a new `sleep_waiting_for_unblocked_peer` into `self.sleep_waiting_for_unblocked_peer`. But the very next line (`self.sleep_waiting_for_unblocked_peer = None;` at line 196) unconditionally clears that newly-set sleep future, discarding it. The pending sessions are then lost — no re-wakeup will ever occur, and the sessions stay queued forever.

**Root Cause**: The `None` assignment at line 196 runs whether or not `assign_peer_to_session` set a new sleep future. It should only clear the field if a new one was not installed.

```rust
// current code (buggy):
if let Some(sleep_future) = &mut self.sleep_waiting_for_unblocked_peer {
    ready!(sleep_future.as_mut().poll(cx));
    for outbound_session_id in std::mem::take(&mut self.sessions_received_when_no_peers) {
        self.assign_peer_to_session(outbound_session_id);   // may set a NEW sleep future
    }
}
self.sleep_waiting_for_unblocked_peer = None;  // <-- overwrites the newly-set future!
self.pending_events.pop().map_or(Poll::Pending, Poll::Ready)
```

**Failing Test**:

```rust
// Place in crates/apollo_network/src/peer_manager/test.rs.
// Reproduces: all peers blocked, session queued, sleep fires, peer still blocked (clock
// resolution / short timeout), new sleep installed, immediately cleared by the bug,
// session is stuck forever with no wakeup.
#[tokio::test]
async fn session_not_stuck_when_all_peers_still_blocked_after_sleep_fires() {
    use tokio::time::{pause, advance, resume, timeout, Duration};

    const LONG_TIMEOUT: Duration = Duration::from_secs(10);
    const SHORT_TIMEOUT: Duration = Duration::from_millis(100);

    // Use a very short unstable_timeout so the sleep fires quickly.
    let config = PeerManagerConfig {
        malicious_timeout_seconds: LONG_TIMEOUT,
        unstable_timeout_millis: SHORT_TIMEOUT,
    };
    let mut peer_manager = PeerManager::new(config);

    let peer_id = *DUMMY_PEER_ID;
    let connection_id = ConnectionId::new_unchecked(0);
    let mut peer = Peer::new(peer_id, Multiaddr::empty());
    peer.add_connection_id(connection_id);
    peer_manager.add_peer(peer);

    // Block the peer.
    peer_manager.report_peer(peer_id, ReputationModifier::Unstable).unwrap();
    // Drain the blacklisted event.
    let _ = peer_manager.next().now_or_never();

    // Queue a session while peer is blocked.
    let session = OutboundSessionId { value: 1 };
    assert!(peer_manager.assign_peer_to_session(session).is_none());

    // Advance time by SHORT_TIMEOUT so the first sleep fires.
    // BUT do NOT advance enough for the peer to be unblocked yet (advance only 99ms).
    pause();
    advance(Duration::from_millis(99)).await;
    resume();

    // Poll once: the sleep fires. assign_peer_to_session sees peer still blocked,
    // installs a NEW sleep future. The bug clears it immediately.
    // A correct implementation would keep the new sleep and eventually fire again.

    // Now advance past the full timeout so the peer becomes unblocked.
    pause();
    advance(Duration::from_millis(2)).await; // now total >= 100ms
    resume();

    // With the bug: sleep_waiting_for_unblocked_peer was cleared, no wakeup is scheduled,
    // so next() will never produce an event — timeout fires.
    // With the fix: a second sleep was kept, fires, and SessionAssigned is emitted.
    let event = timeout(Duration::from_secs(1), peer_manager.next())
        .await
        .expect("SessionAssigned should have been emitted after peer unblocked");

    assert_matches!(
        event.unwrap(),
        ToSwarm::GenerateEvent(ToOtherBehaviourEvent::SessionAssigned {
            outbound_session_id, ..
        }) if outbound_session_id == session
    );
}
```

**How to Verify**: `SEED=0 cargo test -p apollo_network session_not_stuck_when_all_peers_still_blocked_after_sleep_fires`

---

## Summary

| # | Severity | File | Nature |
|---|----------|------|--------|
| 1 | Medium | `peer_manager/behaviour_impl.rs` | LIFO instead of FIFO event queue — wrong session→peer assignment order |
| 2 | Medium | `p2p_sync/client/block_data_stream_builder.rs` | u64 underflow panic (debug) / silent overflow (release) in limit calculation |
| 3 | Low-Medium | `p2p_sync/client/block_data_stream_builder.rs` | `select!` cancels non-cancel-safe `parse_data_for_block` future, orphans network session |
| 4 | High | `peer_manager/behaviour_impl.rs` | Unconditional `= None` clobbers newly-installed sleep future; sessions stuck forever after edge-case wakeup |
