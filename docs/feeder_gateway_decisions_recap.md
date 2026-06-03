# Feeder Gateway in Apollo: design decisions recap

One-page summary of the decisions made while implementing the legacy feeder gateway as an Apollo
sequencer component, and why we made them.

## What was built

A working feeder gateway component serving the legacy HTTP API from the sequencer's own synced
storage: health endpoints, get_contract_addresses, get_public_key, get_signature,
get_block_hash_by_id, get_block_id_by_hash, plus metrics, an end-to-end test harness, and the full
byte-compatibility layer that unblocks the remaining endpoints (get_block, get_transaction,
receipts).

## Architecture

- **One read abstraction, two backends.** Handlers depend on a `ChainDataReader` trait, never on a
  concrete data source. A co-located backend reads MDBX storage directly (maximum throughput); a
  remote backend reads via the state-sync client (own-pod deployments). Topology is pure config.
- **Bounded blocking reads.** Storage reads run through a semaphore-bounded executor (~1.5x cores)
  so parallel MDBX reads never starve the async runtime and cannot pile up unboundedly. We chose
  the 10-line semaphore over a dedicated thread pool until perf data justifies more.
- **Read-only by design.** The component is deliberately excluded from tx-ingestion validation, so
  validation-only nodes can still re-serve reads.

## Compatibility: the live service is the only ground truth

The single most important methodological decision: **every parity claim is verified against the
live Python feeder gateway and locked with byte-exact fixtures.** Our design docs were wrong
repeatedly and only live probing caught it. Examples: errors are HTTP 400, not 404; `blockId`
rejects `latest`/`pending` despite docs saying otherwise; error messages are echoed verbatim, not
sanitized; `get_signature` without arguments serves the latest block instead of erroring.

Key parity decisions, each fixture-locked against live captures:

- **Transaction wire format.** Python puts the `type` tag last and uses per-version field orders
  that a single struct layout cannot express (invoke v0 even uses a different key name). We wrote
  small version-aware serializers in the shared client crate instead of flipping a global
  serde_json setting that would have silently reordered every JSON map in the workspace.
- **Receipts.** Reordered to live order; the L1-handler consumed message is omitted everywhere
  else; L1 addresses are EIP-55 checksummed, implemented once in the client crate.
- **get_contract_addresses.** The live response is network-variable (mainnet 4 L1 contracts,
  sepolia 8, different orders), so the contract set became ordered configuration rather than a
  fixed struct, rendered with EIP-55 checksums.
- **Error messages.** We replicate Python's exact texts, including its `int(json.loads(...))`
  quirks (`blockId=1.5` really serves block 1 on mainnet today). The few unreplicable corners
  (instance-local range bounds, exotic float echoes) are documented divergences, not silent ones.

## Scope cuts (verified, not assumed)

- `get_storage_at` / `get_nonce` / `get_class_hash_at` / `get_transaction_receipt` are
  **deprecated on the live service** and intentionally not implemented.
- Caching is deferred: the system must hit target RPS without it; it would only trim hot keys.
- Own-pod deployment (k8s) and the load-test suite are deferred until topology needs them.

## How the work is organized

The ~60 small PRs form a **tree, not a line**: a trunk that is a complete working feeder gateway,
with independently droppable branches per concern: metrics, byte-identical behaviour (itself split
into wire format / contract addresses / error messages), tests, and groundwork for future
endpoints. Each branch compiles and passes tests along its whole path, so product can drop or
defer any concern without unraveling the rest. Two things are trunk-bound by real code
dependencies and would need content splits to extract: the remote read backend and the
Python-style serializer.

## Open items for product

1. Do we need byte-identical behaviour at all, or is semantic compatibility enough? (Decides the
   entire parity branch.)
2. Do we need the distributed (own-pod) topology? (Decides deployment work; the remote backend
   itself ships with the trunk.)
3. Priority of the remaining endpoints (get_block is the big one; its serialization blockers are
   now resolved).
