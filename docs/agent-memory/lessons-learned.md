# Lessons Learned

Living collection of high-signal repo gotchas and recurring patterns.

Use this file when:
- debugging cross-crate behavior
- touching shared wiring or transport paths
- deciding how broad verification should be
- reasoning about repo-local constraints that are easy to forget

## Lessons

- Local component/server wiring follows `communication.rs -> clients.rs -> components.rs -> servers.rs`; if one changes, inspect the full chain.
- `workspace_tests` protects workspace manifest integrity and should run whenever manifests or publish-related metadata change.
- Root toolchain is stable 1.92, but `starknet_transaction_prover` uses `nightly-2025-07-14`.
- [human][verify] Related private repos exist outside this checkout: `starkware-industries/starkware` and `starkware-industries/starkware-envs-production`.
