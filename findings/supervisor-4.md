# Supervisor 4 Report

## Hunter 13 — Bug 1

**Verdict**: confirmed

**Rationale**: Traced the logic in `crates/starknet_api/src/transaction/fields.rs`, lines 51–63.

`checked_div_ceil` calls `self.checked_div(rhs)` which returns `Some(GasAmount(u64::MAX))` when
`floor(fee / price) == u64::MAX` (it still fits in `u64`). Then, if `value * price < self` (i.e.
there is a non-zero remainder), the code executes `(value.0 + 1).into()`. With `value.0 = u64::MAX`,
`u64::MAX + 1` overflows: panic in debug builds, wraps to 0 in release builds.

The example values check out:
- `fee = 2 * u64::MAX + 1` (fits in `u128`)
- `price = 2`
- `floor(fee/price) = u64::MAX` with remainder `1`, so `checked_div` returns `Some(GasAmount(u64::MAX))`
- `checked_mul(rhs.into())` computes `GasPrice(2).checked_mul(GasAmount(u64::MAX))` → `Some(Fee(2 * u64::MAX))`, no panic from `expect`
- `Fee(2 * u64::MAX) < Fee(2*u64::MAX + 1)` is `true`, so the `+1` branch is taken
- `u64::MAX + 1_u64` overflows → panic (debug) / 0 (release)

The test is legitimate: it constructs input values using only public APIs (`Fee`, `NonzeroGasPrice::try_from`), calls `fee.checked_div_ceil(price)`, and asserts `None`. No internal state is manufactured. The scenario is reachable whenever a fee computed in `u128` happens to have a floor quotient of exactly `u64::MAX` with a non-zero remainder.

---

## Hunter 14 — Bug 1

**Verdict**: confirmed

**Rationale**: Traced the logic in `crates/apollo_consensus_orchestrator/src/sequencer_consensus_context.rs`, lines 1092–1102.

`queued_proposals` is a `BTreeMap<Round, ...>`. `first_entry()` returns the entry with the **minimum** key. The comparison `self.current_round.cmp(entry.key())` produces:

- `Less`  → `current_round < min_queued_key` → the smallest queued round is in the future → correct action: stop iterating. **Actual**: removes the future-round entry.
- `Equal` → process the entry. Correct.
- `Greater` → `current_round > min_queued_key` → the smallest queued round is stale → correct action: remove and continue. **Actual**: returns early, leaving the stale entry.

Both `Less` and `Greater` arms perform the wrong action. Consequence A (`Less`): advancing to an intermediate round (say round 2 when round 4 is queued) silently drops the round-4 proposal; the corresponding `fin_sender` is dropped, causing `fin_receiver.await` to return `Err(Canceled)`. Consequence B (`Greater`): skipping a round leaves a stale entry in the map indefinitely.

The test uses the public `validate_proposal` and `set_height_and_round` APIs to queue two future-round proposals and check that the further-future one survives an intermediate advance. This is an idiomatic scenario (out-of-order proposal delivery under asynchrony) using the same helpers (`SetupDepsArgs`, `create_test_and_network_deps`, `proposal_init`) that the existing tests use.

---

## Hunter 14 — Bug 2

**Verdict**: confirmed

**Rationale**: Traced `truncate_to_executed_txs` in `crates/apollo_consensus_orchestrator/src/utils.rs`, lines 452–473.

With `final_n_executed_txs = 0`, `remaining_tx_count = 0`. The loop is entered for the first batch. The condition `remaining_tx_count < batch.len()` is `0 < N` which is `true` for any non-empty batch. The code therefore executes `batch.into_iter().take(0).collect()` (an empty vec) and pushes it before breaking. Result: `vec![vec![]]`.

The correct result is `vec![]` (no batches). As traced in `send_reproposal` (line 1243), iterating `txs` sends one `ProposalPart::Transactions(TransactionBatch { transactions: vec![] })` to the network. The validator's `handle_proposal_part` receives this and calls `batcher.send_txs_for_proposal` with an empty txs list, which is structurally different from a proposal with no transaction batches. The bug is real.

The test calls `truncate_to_executed_txs` directly (a `pub(crate)` function) with a concrete non-empty content and `0`, then asserts the return is empty. This is a direct, legitimate unit test with no artificial state.

---

## Hunter 15 — Bug 1

**Verdict**: suspected

**Rationale**: The underlying vulnerability is real and confirmed in the source. `build_peer_identity_message_digest` in `crates/apollo_signature_manager/src/signature_manager.rs`, lines 127–136, concatenates `INIT_PEER_ID || peer_id.to_bytes() || challenge.0` with no length delimiter between the variable-length `peer_id` bytes and the fixed-length `challenge`. The code's own TODO comment acknowledges this. Because `peer_id.to_bytes()` is not fixed-width, two distinct `(peer_id, challenge)` pairs that share the same overall byte sequence produce identical message digests and hence identical signatures. A signature for `(A, x)` would pass `verify_identity(B, y, sig, pk)` if `A.bytes() ++ x == B.bytes() ++ y`.

However, the test is rejected as written. `build_peer_identity_message_digest_for_test` does not exist in the codebase; the test requires modifying the production code to expose a private function before it compiles. Additionally, the test does not actually call any production code — it only proves that two crafted byte sequences are equal (which is trivially true by construction), then defers the actual signature-acceptance demonstration to commentary. The test never calls `verify_identity` or `sign_identification`, so it does not demonstrate the bug through normal idiomatic usage. It is a theoretical proof-of-concept that does not exercise the production code path.

The vulnerability exists and is worth fixing, but the test does not meet the legitimacy bar.

---

## Hunter 15 — Bug 2

**Verdict**: suspected

**Rationale**: The design-level problem is real and confirmed in the source. `convert_internal_rpc_tx_to_rpc_tx` in `crates/apollo_transaction_converter/src/transaction_converter.rs`, lines 212–215, calls `self.get_proof(&tx.proof_facts)` which fetches the proof from the local proof manager. A node that received the transaction via P2P mempool propagation never had `spawn_verify_and_store_proof` called locally (that path is only triggered by the gateway flow in `convert_rpc_tx_to_internal`). Therefore, when such a node later calls `convert_internal_consensus_tx_to_consensus_tx` during proposal building, `get_proof` returns `None` → `Err(ProofNotFound)`.

However, the test is partially artificial. It uses a `setup_converter` to run the full gateway flow (including the verify-and-store task) and then creates a second "empty" `proposer_converter` with a fresh mock that has no proof stored. This simulates a P2P receiver's state but not through the P2P code path itself — there is no actual P2P ingestion. Additionally, `invoke_tx_client_side_proving` from `mempool_test_utils` requires specific Cairo version fixtures. The test requires infrastructure (`MockProofManagerClient`, `await_verify_and_store_proof_task`) that implies the scenario is only exercisable in testing contexts, not through the real P2P stack. The hunter also explicitly acknowledges the P2P proof propagation path is not yet complete, meaning this is a known incomplete feature, not an active regression. The verdict is **suspected** rather than confirmed because the bug is architecturally real but the test manufactures the "P2P receiver" state through the gateway path rather than actual P2P ingestion.

---

## Hunter 16 — Bug 1

**Verdict**: suspected

**Rationale**: The structural problem is real. In `crates/apollo_infra/src/component_server/local_component_server.rs`, `process_requests` (line 223) spawns a background processing task and discards the `JoinHandle`. If the processing task panics (e.g. inside `component.handle_request`), Tokio captures the panic in the dropped handle; it is never observed. The channel receivers (`high_rx`, `normal_rx`) are dropped when the task unwinds. The `RequestWrapper::tx` (the response sender) is also dropped. The client's `res_rx.recv().await` returns `None`, hitting `.expect("Inbound connection should be open.")` at line 54 of `local_component_client.rs` — a panic rather than a clean `ClientError`.

The test uses `AlwaysPanicsComponent` to reliably trigger this path. This is somewhat artificial (components are not expected to panic in production) but it is a valid way to demonstrate that a component panic is not correctly propagated to the caller. The real-world trigger would be any unexpected panic in a component handler, which is a possible production event. The test pattern is otherwise idiomatic (uses existing test helpers). However, the test structure wraps `server.start()` in `std::panic::AssertUnwindSafe(...).catch_unwind()`, which is a non-standard pattern not found in the existing test suite. The test's own assertion (`result.is_err() || result.unwrap().is_err()`) passes if a timeout occurs, which could mask that the actual failure is a panic rather than a clean error — making it harder to be certain this test specifically proves the bug through normal usage. The verdict is **suspected**.

---

## Hunter 16 — Bug 2

**Verdict**: suspected

**Rationale**: The off-by-one in the retry logging condition is real and confirmed in the source. `crates/apollo_infra/src/component_client/remote_component_client.rs`, line 386:

```rust
if attempt % attempts_per_log == attempts_per_log - 1 {
```

For `attempts_per_log = N > 1`, this fires at attempts `N-1, 2N-1, 3N-1, ...` instead of `N, 2N, 3N, ...`. The default `attempts_per_log = 1` masks the bug (both `attempt % 1 == 0` and `attempt % 1 == 0` are always true). The bug only manifests for `attempts_per_log > 1`.

However, the test is rejected on legitimacy grounds: it does not call any production code. It duplicates the formula inline and asserts that two different arithmetic expressions produce different result sets. This is purely a proof of the mathematical discrepancy, not a test that exercises the production retry loop. A legitimate test would configure a `RemoteComponentClient` with `attempts_per_log > 1`, point it at an unreachable server, intercept logs, and verify the first warning appears at attempt `N` rather than `N-1`. As written, the test only asserts that `[2,5,8] != [3,6,9]`, which trivially passes without invoking any production logic. The bug is real but the test does not demonstrate it through normal usage.

---

## Summary

- Confirmed: 3 bugs (Hunter 13 Bug 1; Hunter 14 Bug 1; Hunter 14 Bug 2)
- Suspected: 4 bugs (Hunter 15 Bug 1; Hunter 15 Bug 2; Hunter 16 Bug 1; Hunter 16 Bug 2)
- Rejected: 0 bugs
