# CLAUDE.md

## Security Warnings

**CRITICAL**: Never read, modify, or expose secret/credential files:
- Files: `.env`, `*_secrets.json`, `*keys.json`, Kubernetes `secret.yaml`
- Patterns: `*secret*`, `*key*.json`, `.env*`

## Project Overview

Starknet sequencer (Apollo) - modular blockchain sequencer in Rust. Components are wrapped in servers (local or remote). Local servers use tokio mpsc channels; remote servers use HTTP with JSON serialization (via hyper). Deployment topology is configured without code changes.

**Tx ingestion**: Gateway → Mempool (+ P2P propagation)
**Block production**: Consensus Manager orchestrates Batcher (pulls from Mempool + L1 Provider, executes via Blockifier)
**Finalization**: Batcher commits → Committer (state root) → State Sync

**Crate pattern**: Each component has 3 crates: `apollo_*` (logic), `apollo_*_config`, `apollo_*_types`

**Storage**: Apollo storage (MDBX) for sync, batcher & class manager; RocksDB with Patricia merkle trees for state commitment

## Essential Commands

**Python scripts require the venv**: Run `source sequencer_venv/bin/activate` first.

```bash
cargo build -p <package>
SEED=0 cargo test -p <package>
unset CI && scripts/rust_fmt.sh
python scripts/run_tests.py --command clippy --changes_only --commit_id HEAD
python scripts/run_tests.py --command integration --changes_only --include_dependencies --commit_id HEAD
```

## Git Workflow

Commit format (enforced by commitlint in CI): `scope[,scope2,...]: subject`

- **No type prefix** - don't use `feat:`, `fix:`, etc.
- **Scope required** - use crate names: `apollo_gateway`, `blockifier`, etc.
- **Special scopes**: `ci`, `deployment`, `release`, `scripts`, `workspace`, `infra`
- **Max 100 chars**

Examples:
```
apollo_gateway: add transaction validation caching
apollo_mempool,apollo_batcher: optimize streaming
ci: update rust toolchain
```

## Code Conventions

### Project Patterns
- Types crate has `communication.rs` with Client trait, Request/Response enums
- Config structs implement `SerializeConfig`
- Test-only code gated with `#[cfg(any(feature = "testing", test))]`
- Mock traits use `mockall` with `#[cfg(feature = "testing")]`
- Metrics use `apollo_metrics` framework

## Common Gotchas

- **Proto rebuild**: `cargo clean -p apollo_protobuf && cargo build -p apollo_protobuf`
- **Dashboard regen**: `cargo run --bin sequencer_dashboard_generator`
- **Test isolation**: Use `tempfile::TempDir` for storage paths in tests
- **Cairo native feature**: When modifying `cairo_native`-gated code, verify with `cargo check -p blockifier --features cairo_native`
- **CI flaky tests**: `blockifier_reexecution` can fail from transient GCloud network issues (not code-related). `merge-gatekeeper` fails when other checks fail (downstream).
- **Feature flag removal**: `define_infra_metrics!` and `define_metrics!` macros expand code with `cfg(feature = "testing")`. Crates using these macros MUST keep `testing = []` in Cargo.toml even if it looks unused — `check-cfg` will error otherwise. Grep for macro invocations, not just direct `cfg` checks.
- **Adding `Copy` derive**: Search the entire codebase for `.clone()` on that type (including tests, other crates). Clippy treats `clone_on_copy` as deny-level in CI.

## Mandatory Practices

### Debugging
When encountering any bug, test failure, or unexpected behavior: investigate the root cause before proposing fixes. Read error messages, trace the execution path, and form a hypothesis. Do not guess-and-check. If `/systematic-debugging` or `/debugging-wizard` skill is available, invoke it.

### Verification
Before claiming work is complete, fixed, or passing: run the relevant verification commands fresh and confirm the output. No claims without evidence. "Should work" is not verification. If `/verification-before-completion` skill is available, invoke it.

### Receiving Code Review
When addressing PR review comments: read the full comment, understand the reviewer's intent, and verify the suggestion is technically correct before implementing. Do not blindly apply feedback — push back if the suggestion would introduce a bug or violate project conventions. If `/receiving-code-review` skill is available, invoke it.

## Code Guidelines

Reusable coding lessons are in `.claude/rules/code-style.md`.
