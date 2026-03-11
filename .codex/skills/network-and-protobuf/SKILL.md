---
name: network-and-protobuf
description: Use this skill when changing P2P behavior, protobuf schemas, `apollo_network`, `apollo_mempool_p2p`, `apollo_p2p_sync`, or any message or topic consumed across processes. It should also trigger when proposal or sync traffic crosses the network boundary.
---

# Network and Protobuf

<purpose>
Change wire-format or transport code without leaving producers, consumers, or generated protobuf output out of sync.
</purpose>

<context>
- `apollo_network` provides libp2p primitives such as broadcast topics and SQMR clients/servers.
- `apollo_mempool_p2p` and `apollo_p2p_sync` sit on top of those primitives.
- Protobuf sources live under `crates/apollo_protobuf/src/proto/p2p/proto/`.
- Generated output lives in `crates/apollo_protobuf/src/protobuf/protoc_output.rs` and is not a hand-edited file.
</context>

<procedure>
1. Classify the change:
   - transport/runtime behavior -> `apollo_network`
   - consumer/producer logic -> `apollo_mempool_p2p`, `apollo_p2p_sync`, or consensus orchestrator
   - schema change -> `apollo_protobuf/src/proto/**`
2. For schema changes:
   - edit the `.proto` source
   - rebuild with `cargo clean -p apollo_protobuf && cargo build -p apollo_protobuf`
   - inspect every producer and consumer of that message
3. For topic or SQMR behavior changes, search across all registration sites before editing one crate in isolation.
4. If consensus messages are involved, also load `consensus-and-block-building`.
5. Verify with crate tests and any affected integration flow.
</procedure>

<patterns>
<do>
- Edit protobuf source files, not generated output.
- Trace every message through both registration and handling paths.
- Keep malformed-message handling and peer-reporting behavior intact.
</do>
<dont>
- Don't manually edit `protoc_output.rs`.
- Don't change a protobuf-backed type in one crate without checking all downstream users.
- Don't assume message ordering is incidental; proposal and sync streams rely on it.
</dont>
</patterns>

<examples>
Example: protobuf regeneration
```bash
cargo clean -p apollo_protobuf
cargo build -p apollo_protobuf
```
</examples>

<troubleshooting>
| Symptom | Cause | Fix |
|---------|-------|-----|
| Deserialization or conversion failure | generated code or consumers are stale | rebuild `apollo_protobuf` and update all users |
| P2P message accepted locally but not propagated | topic registration or report-peer path diverged | inspect `apollo_mempool_p2p` runner/propagator and network registration |
| `Change file not found` after schema edit | generated benchmark or regression output missing | regenerate or rerun the owning package's verification path |
</troubleshooting>

<references>
- `crates/apollo_network/src/`: transport primitives and libp2p integration
- `crates/apollo_mempool_p2p/src/lib.rs`: broadcast-topic registration for mempool traffic
- `crates/apollo_p2p_sync/src/client/mod.rs`: SQMR-backed sync client
- `crates/apollo_protobuf/src/regression_test_utils.rs`: protobuf source inventory
- `crates/apollo_protobuf/src/proto/p2p/proto/`: schema sources
</references>
