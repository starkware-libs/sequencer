# Skill Registry

Last updated: 2026-03-11

Load only the skill that matches the task. For any code change, also load `testing-and-presubmit`.

| Skill | Path | Triggers | Priority |
|-------|------|----------|----------|
| Component Development | `component-development/SKILL.md` | component, communication.rs, client trait, request/response, apollo_node wiring | Core |
| Testing and Presubmit | `testing-and-presubmit/SKILL.md` | test, nextest, presubmit, clippy, workspace_tests, CI parity | Core |
| Debugging | `debugging/SKILL.md` | bug, failure, panic, timeout, regression, root cause | Core |
| Consensus and Block Building | `consensus-and-block-building/SKILL.md` | consensus, batcher, proposal, round, vote, decision_reached | Core |
| Storage and State | `storage-and-state/SKILL.md` | storage, MDBX, RocksDB, patricia, global root, reader/writer | Core |
| Network and Protobuf | `network-and-protobuf/SKILL.md` | p2p, libp2p, protobuf, gossipsub, sqmr, mempool_p2p, p2p_sync | Core |
| Deployment Topology | `deployment-topology/SKILL.md` | deploy, topology, consolidated, distributed, hybrid, execution mode, cdk8s | Extend |
| Performance and Benchmarks | `performance-and-benchmarks/SKILL.md` | slow, optimize, benchmark, criterion, bench_tools, regression-limit | Extend |
| Release, Branching, and Artifacts | `release-branching-and-artifacts/SKILL.md` | release, backport, main-v, merge_branches, artifact, docker publish, GCS | Extend |

## Missing Skills (Recommended)
- [ ] Security review for gateway, RPC, and external-input DoS surfaces
- [ ] Observability and dashboard updates for metrics-heavy changes
- [ ] Cross-repo change coordination for private StarkWare repos once those repos are available locally
