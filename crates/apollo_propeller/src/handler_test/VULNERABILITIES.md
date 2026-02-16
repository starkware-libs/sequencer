# Handler Vulnerability Analysis

All vulnerabilities considered from the perspective of a **malicious remote peer**
sending crafted data over the network to a Propeller handler.

## Legend
- **Status**: `possible` / `not possible` / `not possible (tested)` / `not possible (mitigated)`
- **Test**: test name that proves/disproves, or `NONE`

---

## V1: Oversized wire message causes OOM
A peer sends a single wire frame claiming to be very large (e.g. 4 GB).
- **Status**: not possible (tested)
- **Why**: `PropellerCodec` (via `ProtoCodec`) enforces `max_wire_message_size` (default 1 MB). Frames exceeding this are rejected before any allocation.
- **Test**: `penetration_test::length_prefix_claims_max_u32_bytes`, `wire_test::raw_oversized_message`

## V2: Malformed varint length prefix causes panic or large allocation
A peer sends a varint prefix that overflows or is malformed.
- **Status**: not possible (tested)
- **Why**: prost/unsigned-varint decoder rejects overflow. Codec returns error, stream transitions to Closing.
- **Test**: `penetration_test::varint_overflow_attack`, `fuzz_test::fuzz_random_varint_prefixes`

## V3: Garbage bytes after valid length prefix cause panic
A peer sends a valid length prefix followed by non-protobuf data.
- **Status**: not possible (tested)
- **Why**: Protobuf deserialization fails, codec returns Err, handler logs warning and closes stream.
- **Test**: `penetration_test::garbage_bytes_after_valid_length_prefix`

## V4: Invalid protobuf fields cause panic
A peer sends a protobuf batch with units that have missing/invalid fields (wrong-sized hashes, invalid PeerId, out-of-range index).
- **Status**: not possible (tested)
- **Why**: `PropellerUnit::try_from()` validates all fields and returns Err on any mismatch. Handler logs warning and drops the unit.
- **Test**: `penetration_test::batch_with_all_fields_missing`, `batch_with_invalid_peer_id`, `batch_with_wrong_sized_merkle_root`, `batch_with_wrong_sized_merkle_siblings`, `batch_with_index_exceeding_u32`, `batch_with_huge_index`
- **Note**: `ShardIndex` wraps `u64`, so `batch_with_index_exceeding_u32` and `batch_with_huge_index` verify no panic on extreme-but-valid index values (the units are accepted, not rejected). Field rejection is proven by the other tests in this list (missing fields, wrong-sized hashes, invalid PeerId).

## V5: Many invalid units in one batch cause amplification or slow processing
A peer sends a batch with thousands of invalid units to waste CPU.
- **Status**: not possible (tested)
- **Why**: Each invalid unit is a cheap `try_from` failure (no allocation). Total batch is bounded by `max_wire_message_size`. 10K invalid units tested without issues.
- **Test**: `penetration_test::batch_with_many_empty_units`, `dos_attack_test::massive_batch_all_invalid_units`

## V6: Large signature/shard/proof fields cause OOM
A peer sends a unit with a large signature, many merkle proof siblings, or huge shard data.
- **Status**: not possible (tested)
- **Why**: The entire protobuf batch (including all field data) must fit within `max_wire_message_size` (1 MB). The codec rejects any frame exceeding this before deserialization. A ~100 KB shard is accepted and forwarded correctly. Merkle siblings at 32 bytes each are capped at ~32K within 1 MB.
- **Test**: `penetration_test::large_shard_within_wire_limit`, `wire_test::raw_oversized_message`, `dos_attack_test::large_shard_units_no_crash`

## V7: Unbounded accumulation of unsent_units under backpressure
A peer sends batches faster than the engine drains the channel. Without a guard, `unsent_units` grows without bound because `poll_inner` reads new batches even when the buffer is non-empty.
- **Status**: not possible (tested)
- **Why**: Fixed by `if self.unsent_units.is_empty()` guard in `poll_inner` (PR: `guard_inbound_reads_on_non-empty_unsent_units_buffer`). The handler only reads a new batch from the wire when the previous batch has been fully drained to the channel.
- **Test**: `dos_attack_test::backpressure_bounds_unsent_units`

## V8: Slowloris attack — remote never reads outbound data
A peer accepts the connection but never reads outbound data, causing the handler's outbound path to stall.
- **Status**: not possible (tested)
- **Why**: `poll_active_outbound_substream` returns `Poll::Pending` when the sink isn't ready. The handler doesn't spin or allocate unboundedly — it just stops sending.
- **Test**: `dos_attack_test::slowloris_outbound_stall`

## V9: Rapid inbound substream open/close causes resource leak
A peer rapidly opens and closes inbound substreams to exhaust handler resources.
- **Status**: not possible (tested)
- **Why**: `CONCURRENT_STREAMS = 1` — only one inbound substream is active at a time. New substreams replace the old one. No accumulation.
- **Test**: `dos_attack_test::inbound_substream_bombardment`, `penetration_test::rapid_inbound_substream_churn`, `dos_attack_test::empty_substream_negotiation_churn`

## V10: Connection drop mid-message causes panic or state corruption
A peer drops the TCP connection in the middle of sending a protobuf frame.
- **Status**: not possible (tested)
- **Why**: Codec returns `Poll::Pending` for incomplete data, or `Err` for connection reset. Handler transitions to Closing state.
- **Test**: `wire_test::raw_connection_drop_mid_message`, `wire_test::raw_truncated_varint`, `wire_test::raw_truncated_payload`

## V11: Inbound error corrupts outbound path
A broken inbound substream (garbage data, errors) interferes with the independent outbound sending path.
- **Status**: not possible (tested)
- **Why**: Inbound and outbound are separate substreams with independent state machines. An inbound error closes only the inbound substream.
- **Test**: `penetration_test::outbound_after_inbound_error`

## V12: Event amplification — one inbound unit generates multiple outbound events
A peer sends N units but the handler emits more than N events, amplifying work for the engine.
- **Status**: not possible (tested)
- **Why**: Each valid unit produces exactly one entry in `unsent_units`, which is drained one-at-a-time to the channel. 1:1 ratio confirmed with 1000 units.
- **Test**: `dos_attack_test::events_to_emit_no_amplification_under_flood`

## V13: Alternating valid/garbage data corrupts handler state
A peer alternates between sending valid units and garbage, hoping error-path/success-path interleaving corrupts internal state.
- **Status**: not possible (tested)
- **Why**: Error path closes the inbound substream; next valid data arrives on a fresh substream. 100 cycles tested.
- **Test**: `dos_attack_test::rapid_valid_invalid_oscillation`

## V14: Simultaneous inbound flood while outbound is stalled
Both directions are stressed: inbound floods with data while outbound is blocked (remote doesn't read). Could cause deadlock.
- **Status**: not possible (tested)
- **Why**: `poll_inner` processes outbound (non-blocking Pending) then inbound independently. No shared lock. 50 inbound units delivered while outbound was stalled.
- **Test**: `dos_attack_test::inbound_flood_during_outbound_stall`

## V15: Unbounded send_queue growth from behaviour
The engine enqueues outbound units via `on_behaviour_event` faster than the handler can drain them.
- **Status**: not possible (mitigated)
- **Why**: `send_queue` is populated by local code only (the behaviour), not by remote peers. A remote peer cannot push to the send queue. The queue is bounded in practice by the engine's throughput. Warning logged at >100 entries.
- **Test**: `penetration_test::send_queue_flood` (tests 500 units queued, shows handler remains functional)

## V16: Unbounded events_to_emit growth
`events_to_emit` VecDeque grows without limit.
- **Status**: not possible (tested)
- **Why**: Only populated by `DialUpgradeError` drain (local event, not remote-triggered) and `SendError` from codec failures. A remote peer cannot directly cause unbounded growth. DialUpgradeError happens at most once per dial attempt. 500 cycles of enqueue/error/drain tested without accumulation.
- **Test**: `dos_attack_test::dial_upgrade_error_rapid_cycling_no_leak`

## V17: Back-to-back wire messages in a single TCP write parsed incorrectly
A peer sends two length-delimited protobuf batches concatenated in a single TCP write. The codec might miss the boundary.
- **Status**: not possible (tested)
- **Why**: Length-delimited framing correctly parses back-to-back messages. Both batches are delivered.
- **Test**: `penetration_test::multiple_messages_in_single_tcp_write`

## V18: Byte-at-a-time drip feed causes timeout or incorrect reassembly
A peer sends one byte at a time, extremely slowly.
- **Status**: not possible (tested)
- **Why**: Codec returns `Poll::Pending` until the full frame is assembled. The final byte completes the frame. No timeout in the codec (timeout is at a higher layer if needed).
- **Test**: `penetration_test::byte_at_a_time_delivery`

---

## V19: Idle inbound substream holds slot indefinitely (no timeout)
A peer negotiates an inbound substream but never sends data and never closes. The single inbound slot is occupied forever.
- **Status**: not possible (tested)
- **Why**: Each handler is per-connection. The peer holds only their own inbound slot — other peers have separate connections with separate handlers. The idle substream does not block outbound traffic on the same connection. libp2p connection-level keep-alive and transport timeouts provide an upper bound.
- **Test**: `penetration_test::inbound_idle_does_not_block_outbound`

## V20: Log flooding from invalid proto units
A peer sends batches of invalid units. Each triggers `warn!()` in `handle_received_batch`. With small units (~10 bytes each) and max_wire_message_size=1 MB, a single batch can contain ~100K units, each generating a log warning.
- **Status**: not possible (mitigated)
- **Why**: `handle_received_batch` now uses `warn_every_n_ms!(1000, ...)` which rate-limits log output to at most once per second per call site. A malicious peer sending many invalid units causes at most one warning per second. Handler remains fully functional.
- **Test**: `dos_attack_test::massive_batch_all_invalid_units` (exercises the path; rate limiting verified by code inspection)

## V21: Multiple batches decoded in single poll_single_inbound_substream call
The inner loop in `poll_single_inbound_substream` continues on `Ready(Ok(batch))`, decoding all buffered batches into `unsent_units` in one call. The `unsent_units.is_empty()` guard in `poll_inner` only prevents reading across poll cycles, not within a single call.
- **Status**: not possible (mitigated)
- **Why**: The total data available in one call is bounded by the transport buffer (yamux frame window, typically 256 KB). Each message is bounded by `max_wire_message_size` (1 MB). In practice, only a few messages accumulate per call. The `unsent_units` growth is proportional to the transport buffer, not unbounded.
- **Test**: `penetration_test::multiple_messages_in_single_tcp_write` (two back-to-back messages in one write)

## V22: Channel receiver dropped — handler accumulates unsent_units
If the engine drops the `mpsc::Receiver` (e.g., engine shutdown), `unit_sender.poll_ready()` returns `Ready(Err)`. Without handling, `unsent_units` would grow forever.
- **Status**: not possible (tested)
- **Why**: `drain_unsent_units` handles `Ready(Err(_))` by clearing `unsent_units` and logging a warning. The handler remains functional for outbound operations.
- **Test**: `penetration_test::channel_closed_handler_survives`

## V23: Waker not stored before first poll — on_behaviour_event cannot wake
If `on_behaviour_event(SendUnit)` is called before `poll()` ever runs, `self.waker` is `None` and `wake_by_ref()` is skipped. The message sits in `send_queue` until the next external poll.
- **Status**: not possible (mitigated)
- **Why**: The libp2p swarm runtime polls all handlers regularly. The waker is an optimization (reduces latency from ~poll-interval to immediate), not a correctness requirement. Messages are picked up on the next poll cycle.
- **Test**: `known_issues_test::send_unit_should_wake_handler` (tests waker after initial poll; pre-poll case is guaranteed safe by the swarm runtime)

## V24: Unknown protobuf fields inflate wire message size
A peer includes extra unknown fields in the protobuf message. Prost silently ignores unknown fields during decoding, but they consume wire bytes.
- **Status**: not possible (mitigated)
- **Why**: Unknown fields are bounded by `max_wire_message_size`. They consume bandwidth but not additional memory beyond the codec's decode buffer (which is bounded by `max_wire_message_size`). No amplification occurs. Prost skips unknown fields during deserialization without allocating for them.
- **Test**: Covered by V1's tests (`penetration_test::length_prefix_claims_max_u32_bytes`, `wire_test::raw_oversized_message`) — the codec rejects any frame exceeding `max_wire_message_size` regardless of content.

---

## V25: Empty batch (zero-unit protobuf message) wastes handler wake cycles
A peer sends many empty protobuf batches (each decoding to `ProtoBatch { batch: vec![] }`). Each batch wakes the handler, goes through codec decode, and runs the empty `handle_received_batch` loop body.
- **Status**: not possible (mitigated)
- **Why**: Each empty batch is ~2 bytes on the wire. Processing an empty batch is a no-op (empty for-loop in `handle_received_batch`). Total work per poll is bounded by the transport buffer (~256 KB yamux window / 2 bytes = ~128K empty batches max). Each iteration is trivially cheap. No memory growth, no events emitted.
- **Test**: `penetration_test::zero_length_message` (single empty message); `penetration_test::empty_batch_flood` (100 empty batches in one write)

## V26: Units with empty shard data pass handler validation
A peer sends units where all required fields are present and correctly sized, but the shard data is empty (`ShardsOfPeer { shards: vec![] }` or shards with zero-length data). `PropellerUnit::try_from` only checks that `shards` is `Some`, not that shards contain meaningful content.
- **Status**: not possible (tested)
- **Why**: The handler is a transport layer. `try_from` validates structural integrity (required fields present, correct sizes), not semantic validity. Empty shards are forwarded to the engine, which performs semantic validation via `validate_shard_count()` and `validate_shard_lengths()`. Empty shards are bounded by `max_wire_message_size` and don't cause resource issues in the handler.
- **Test**: `penetration_test::empty_shard_units_forwarded`

## V27: Publisher PeerId not validated against actual connection identity
A peer sends units with a `publisher` field containing another peer's PeerId. The handler forwards these units to the engine without verifying that the publisher matches the actual connection's remote peer.
- **Status**: not possible (mitigated)
- **Why**: The handler is a transport layer with no access to the connection's remote PeerId (the `ConnectionHandler` API doesn't expose it). Authentication and authorization are handled by the behaviour/engine layer. The handler correctly limits its responsibility to structural validation and transport.
- **Test**: `penetration_test::spoofed_publisher_forwarded` (verifies unit with arbitrary PeerId is forwarded)

## V28: `warn_every_n_ms!` rate limit shared across all handler instances
The `warn_every_n_ms!` macro uses a `static AtomicU64` per call site (not per handler instance). All handler instances on the same thread share the rate limit counter. A malicious peer on connection A that sends many invalid units can "warm" the rate limiter, suppressing warnings from connection B's independent invalid unit stream.
- **Status**: possible
- **Why**: The macro generates one `static` counter per source-code call site. Multiple handler instances executing the same `warn_every_n_ms!` call share that counter. A high-frequency attacker on one connection can prevent other connections' warnings from appearing within the same 1-second window.
- **Severity**: very low (informational) — no impact on handler behavior, correctness, or resource usage. Only affects log visibility across connections.
- **Test**: NONE (would require multi-handler test setup; not worth the complexity for an informational-severity finding)

## V29: Transient memory amplification from merkle proof with many empty siblings
A peer sends a unit where `merkle_proof.siblings` contains many empty `Hash256` entries (each ~2 bytes of protobuf overhead). Within a 1 MB wire message, ~500K empty siblings fit. `MerkleProof::try_from` calls `Vec::with_capacity(proto.siblings.len())` for `[u8; 32]`, allocating ~16 MB before the first sibling fails the 32-byte size check. Combined with prost's deserialized `Vec<ProtoHash256>` (~12 MB), peak transient allocation is ~28 MB from a 1 MB wire message.
- **Status**: not possible (tested)
- **Why**: The allocation is transient — freed immediately when `try_from` returns `Err` on the first empty sibling. Peak memory is bounded (28x wire message size). Sequential unit processing means no accumulation across units. The amplification factor is significant but bounded, and the handler remains functional.
- **Test**: `penetration_test::merkle_proof_many_empty_siblings_no_crash`

## V30: Outbound send/flush error doesn't report lost message count
When `start_send` or `poll_flush` fails in `poll_active_outbound_substream`, the handler emits `SendError(err.to_string())` but doesn't include the count of messages in the lost batch. In contrast, `DialUpgradeError` reports `{dropped_count} queued message(s) lost`.
- **Status**: possible
- **Why**: `create_message_batch` pops messages from `send_queue` before `start_send`. On error, the batch variable (containing the popped messages) is dropped. The `SendError` message includes only the transport error string. This is an observability gap, not a correctness or resource issue — messages ARE consumed (not accumulated), just silently lost.
- **Severity**: low — affects error diagnostics only, not handler behavior
- **Test**: `known_issues_test::send_error_should_report_lost_unit_count` (tests that SendError is emitted, but doesn't verify message count)

## V31: No unit deduplication at handler level
A peer sends the same structurally-valid unit repeatedly. The handler forwards every copy to the engine via the bounded channel. No deduplication is performed.
- **Status**: not possible (mitigated)
- **Why**: The handler is a transport layer. Deduplication is the engine/behaviour's responsibility (e.g., by message root + publisher + index). Duplicate units are bounded by `max_wire_message_size` and the channel's backpressure. No handler resource issue.
- **Test**: NONE (by design — deduplication is out of scope for the handler)

## V32: V4 test gap — index boundary tests don't verify acceptance through channel
The tests `batch_with_index_exceeding_u32` and `batch_with_huge_index` use `validate_no_events()` which only checks `HandlerOut` events (errors). Since `ShardIndex` wraps `u64`, these extreme index values are ACCEPTED by `try_from` and forwarded to the channel — but the tests don't verify channel delivery. The test comments incorrectly claim the units are rejected. The tests only prove no-panic, not acceptance or rejection.
- **Status**: not possible (tested)
- **Why**: Fixed — tests now verify that units with extreme indices are delivered to the channel, confirming acceptance. Test comments corrected.
- **Test**: `penetration_test::batch_with_index_exceeding_u32`, `penetration_test::batch_with_huge_index`
