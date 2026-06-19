# Bug Hunt Report: apollo_consensus (crates/apollo_consensus/src/)

Files read deeply: `state_machine.rs`, `single_height_consensus.rs`, `manager.rs`,
`stream_handler.rs`, `votes_threshold.rs`, `storage.rs`, `types.rs`,
`state_machine_test.rs`, `test_utils.rs`, `votes_threshold_test.rs`.

---

## Bug 1: Integer overflow in `should_cache_msg` round-limit check

**File**: `/home/user/sequencer/crates/apollo_consensus/src/manager.rs`, line 384

**Description**: The expression `current_round + limits.future_round_limit` uses unchecked `u32` addition. `Round` is `pub type Round = u32` (from `apollo_protobuf::consensus`) and `future_round_limit` is also `u32`. After many round advances, or with a crafted vote carrying a large round number, `current_round` can be near `u32::MAX`. In debug builds the addition panics (killing the consensus task). In release builds it silently wraps to a small number, making the `msg_round <= wrapped_value` comparison incorrect: votes that should be dropped may be accepted, and legitimate votes near the actual limit may be erroneously rejected.

**Root Cause**: No saturating/checked arithmetic guards the addition. The fix is to use `current_round.saturating_add(limits.future_round_limit)`.

**Relevant code**:
```rust
// manager.rs line 382-386
let should_cache = height_diff <= limits.future_height_limit.into()
    && (height_diff == 0 && msg_round <= current_round + limits.future_round_limit
    //                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    //                                   unchecked u32 + u32 addition
        || height_diff > 0 && msg_round <= limits.future_height_round_limit);
```

**Test**:
```rust
// Place in any test module — reproduces the exact arithmetic.
#[test]
#[should_panic]
fn should_cache_round_limit_overflow_panics_in_debug() {
    // Reproduces the expression from should_cache_msg.
    let current_round: u32 = u32::MAX;
    let future_round_limit: u32 = 1;
    // Panics in debug (overflow), silently wraps in release.
    let _ = current_round + future_round_limit;
}
```

**How to verify**:
```
SEED=0 cargo test -p apollo_consensus should_cache_round_limit_overflow_panics_in_debug
```
The test passes (panics as expected) in debug mode, demonstrating the overflow. In release mode the addition wraps silently to 0, making the window calculation wrong.

---

## Bug 2: Late-duplicate inbound stream message destroys active stream state and loses buffered data

**File**: `/home/user/sequencer/crates/apollo_consensus/src/stream_handler.rs`, lines 518-523

**Description**: When `handle_message_inner` receives a content message whose `message_id` is less than `data.next_message_id` (a stale duplicate of an already-delivered message), it returns `None` from the function. Returning `None` signals to the caller `handle_inbound_message` that the stream is finished or failed, so the `StreamData` is **not** reinserted into the LRU cache — it is simply dropped.

This causes two concrete problems:

1. **Buffered future messages are permanently lost.** Any messages that arrived out of order and were waiting in `data.message_buffer` are silently discarded because the `StreamData` struct (which owns the buffer) is dropped.

2. **Duplicate stream receiver sent to the application.** The next message that arrives for the same `(peer_id, stream_id)` pair finds no entry in the cache and creates a fresh `StreamData` with `next_message_id = 0`. A new `mpsc::Receiver` is sent to the application via `inbound_channel_sender`. The application now holds two receivers for the same logical stream. The new receiver's internal channel starts from scratch; if `message_id = 0` never arrives again, the new receiver will block forever. All data delivered on the old receiver is invisible to the new one.

**Root Cause**: The `Ordering::Less` arm should silently ignore the duplicate and return `Some(data)` to preserve stream state. Instead it returns `None`, reusing the "stream finished" sentinel.

**Relevant code**:
```rust
// stream_handler.rs lines 518-523
Ordering::Less => {
    warn!(?peer_id, ?stream_id, ?message_id, ?data.next_message_id,
        "Received message with id that is smaller than the next message expected!");
    return None;   // BUG: should be `return Some(data);`
}
```

**Written justification** (hard to make a small compilable test due to the generic bounds and async context):

Trace the scenario:
1. Peer sends messages 0, 1, 2, 3 on stream S.
2. Node processes 0 (`next_message_id` → 1), buffers 2 and 3 (1 not yet received).
3. Peer retransmits 0. `message_id(0) < next_message_id(1)` → `Ordering::Less` → returns `None`.
4. `handle_inbound_message` does `return Ok(())` without reinserting data. `StreamData` with `{message_buffer: {2, 3}}` is dropped.
5. Message 1 arrives. `data_option = None` (stream not in cache) → new `StreamData` created, new `Receiver` sent to application. Node now waits for `message_id = 0`. Messages 2 and 3 are permanently lost.

**How to verify**: Code review. After the `Ordering::Less` arm returns `None`, trace `handle_inbound_message` (lines 397-423): the `let Some(data) = ... else { return Ok(()); }` short-circuits, and the `push(stream_id, data)` call at line 419 never executes. The stream is gone.

---

## Bug 3: `inbound_send` returns `false` for `Fin`, conflating successful stream completion with a send error

**File**: `/home/user/sequencer/crates/apollo_consensus/src/stream_handler.rs`, lines 274-278 and 476-487

**Description**: `inbound_send` is documented as returning `true` on success and `false` on error (disconnected channel or full channel). When the message body is `Fin`, the function returns `false` with the comment "This is a no-op, can safely return false."

The caller `handle_message_inner` uses the return value to decide whether to close the stream:

```rust
if data.message_buffer.is_empty()
    && data.fin_message_id.is_some()
    && data.fin_message_id.unwrap() == data.next_message_id
    || !message_sent   // ← Fin returns false, triggering this branch
{
    data.sender.close_channel();
    CONSENSUS_INBOUND_STREAM_FINISHED.increment(1);
    info!(?peer_id, ?stream_id, "Inbound stream finished.");
    return None;
}
```

When a Fin arrives in order (`message_id == data.next_message_id`), `inbound_send` returns `false`, `!message_sent` is `true`, and the stream is closed. This is the intended behavior for Fin, but it happens via the *error* path. The stream-finished metric and log are emitted, which is also correct. The problem is that the boolean return type conflates three distinct outcomes: "message delivered", "delivery error", and "this was a Fin". Today the code happens to work because the Fin always closes the stream anyway, but:

- The design is fragile: any future caller that distinguishes `false`-as-Fin from `false`-as-error cannot do so with this API.
- The error log path inside `inbound_send` (line 281-288) is skipped for Fin, but if `inbound_send` is ever called on a buffered Fin (currently impossible since `store` skips Fin), the `!message_sent` branch fires without distinguishing the cause.

**Root Cause**: Boolean return type is overloaded for three distinct states.

**How to verify**: Code review. `inbound_send` lines 274-278 return `false` for Fin; the caller at line 481 tests `!message_sent` and closes the channel on `true`. This works today only because a Fin always should close the stream, but a tri-state return (e.g. an enum `SentOk | SentFin | SentErr`) would make the contract explicit.

---

## Bug 4: `handle_vote_broadcasted` panics for observer SHC instances

**File**: `/home/user/sequencer/crates/apollo_consensus/src/single_height_consensus.rs`, line 173

**Description**: `handle_vote_broadcasted` retrieves the last self-vote and calls `.expect("No last vote to send")`:

```rust
fn handle_vote_broadcasted(&mut self, vote: Vote) -> Requests {
    let last_vote = match vote.vote_type {
        VoteType::Prevote => self.state_machine.last_self_prevote(),
        VoteType::Precommit => self.state_machine.last_self_precommit(),
    };
    let last_vote = last_vote.expect("No last vote to send");  // panics
    ...
}
```

For observer nodes, `StateMachine::make_self_vote` returns immediately without recording any vote (state_machine.rs lines 258-261):

```rust
if self.is_observer {
    return output;  // empty; last_self_prevote / last_self_precommit never set
}
```

Therefore `last_self_prevote()` and `last_self_precommit()` always return `None` for observers. If a `VoteBroadcasted` event is fed to an observer's SHC — which can happen due to incorrect event routing in the manager, or during height transitions where a stale future arrives after the observer flag is set — the code panics, terminating the consensus task.

**Root Cause**: Missing guard for the observer case. A `VoteBroadcasted` event should never reach an observer SHC, but there is no compile-time or runtime guarantee; `expect` is the only defense and it panics rather than recovering.

**Test**:
```rust
// crates/apollo_consensus/src/single_height_consensus_test.rs (add alongside existing tests)

use apollo_protobuf::consensus::{Vote, VoteType, DEFAULT_VALIDATOR_ID};
use starknet_api::{block::BlockNumber, crypto::utils::RawSignature};

use crate::single_height_consensus::SingleHeightConsensus;
use crate::state_machine::StateMachineEvent;
use crate::test_utils::test_committee_with_weights;
use crate::votes_threshold::QuorumType;

#[test]
#[should_panic(expected = "No last vote to send")]
fn observer_panics_when_vote_broadcasted_event_received() {
    let proposer_id = DEFAULT_VALIDATOR_ID.into();
    let observer_id = (DEFAULT_VALIDATOR_ID + 1).into();

    let committee = test_committee_with_weights(
        vec![(proposer_id, 1), (observer_id, 1)],
        Box::new(move |_| proposer_id),
        Box::new(move |_| Ok(proposer_id)),
    );
    let timeouts = apollo_consensus_config::config::TimeoutsConfig::default();

    // Create an OBSERVER SHC (is_observer = true).
    let mut shc = SingleHeightConsensus::new(
        BlockNumber(0),
        true, // is_observer
        observer_id,
        QuorumType::Byzantine,
        timeouts,
        committee,
        false,
    );
    shc.start();

    // Simulate a VoteBroadcasted event being incorrectly routed to an observer.
    let vote = Vote {
        vote_type: VoteType::Prevote,
        height: BlockNumber(0),
        round: 0,
        proposal_commitment: None,
        voter: observer_id,
        signature: RawSignature::default(),
    };
    // This panics with "No last vote to send" because observers never set last_self_prevote.
    shc.handle_event(StateMachineEvent::VoteBroadcasted(vote));
}
```

**How to verify**:
```
SEED=0 cargo test -p apollo_consensus observer_panics_when_vote_broadcasted_event_received
```
The test should pass (panic matches `#[should_panic]`), confirming the bug.
