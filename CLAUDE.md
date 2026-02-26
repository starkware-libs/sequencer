# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Environment Setup

Always activate the Python virtual environment before running any commands:
```bash
. sequencer_venv/bin/activate
```

Install dependencies (first-time setup):
```bash
bash scripts/dependencies.sh
```

## Build and Test Commands

**Build:**
```bash
cargo build
cargo build --bin apollo_node
```

**Run tests (uses `cargo-nextest`, not `cargo test`):**
```bash
# Single crate - always use -p to compile only the target crate
cargo nextest run -p <crate_name>

# Specific test within a crate
cargo nextest run -p <crate_name> <test_name>

# All workspace crates
cargo nextest run --workspace --no-fail-fast
```

**Lint:**
```bash
cargo clippy --workspace --all-targets --all-features
```

**Format (uses nightly-2024-04-29 toolchain):**
```bash
bash scripts/rust_fmt.sh          # Format
bash scripts/rust_fmt.sh --check  # Check only
```

**TOML format:**
```bash
bash scripts/taplo.sh
```

**Local presubmit (runs all fast checks against parent branch):**
```bash
bash scripts/local_presubmit.sh
```

**Run tests for only changed crates:**
```bash
python3 scripts/run_tests.py --command test --changes_only --commit_id <ancestor_sha>
```

## Code Conventions

**Comments:** Every sentence in comments (`//`, `///`, `/* */`) must start with a capital letter and end with a period.

**TODOs:** All TODO comments must follow the format `TODO(owner_name): description`. Bare `TODO` without an owner is rejected by the presubmit checker.

**Warnings:** Rust and Clippy lints are warnings locally but are denied on CI. To enforce locally, add `RUSTFLAGS="-Dwarnings"` to your environment.

## Architecture

This is **Apollo**, a Starknet sequencer node. The main binary is `apollo_node` (`crates/apollo_node`).

### Component-Based Architecture

The node is composed of independently configurable components. Each component can run in one of these modes (set in `SequencerNodeConfig`):
- `LocalExecutionWithRemoteEnabled` / `LocalExecutionWithRemoteDisabled` — component runs locally
- `Remote` — delegates to a remote process via RPC
- `Disabled` — component is off

**Core components** (each typically has a `_types` crate for shared interfaces and a `_config` crate):
- `apollo_batcher` — builds blocks by pulling txs from mempool and L1 provider, executing via Blockifier
- `apollo_consensus_manager` + `apollo_consensus_orchestrator` — Tendermint consensus and proposal coordination
- `apollo_gateway` — entry point for incoming transactions; runs stateless + stateful validation
- `apollo_http_server` — REST API layer forwarding to gateway
- `apollo_mempool` + `apollo_mempool_p2p` — transaction pool with P2P propagation (GossipSub)
- `apollo_state_sync` — syncs chain state from peers
- `apollo_class_manager` + `apollo_compile_to_casm` / `apollo_compile_to_native` — Sierra contract storage and compilation
- `apollo_l1_provider` + `apollo_l1_scraper` — tracks L1 events (deposits, handler txs)
- `apollo_l1_gas_price` — scrapes and provides L1 gas prices
- `apollo_committer` — commits finalized blocks
- `apollo_proof_manager` — manages zero-knowledge proofs
- `apollo_signature_manager` — handles consensus signing
- `apollo_storage` — persistent storage (MDBX/libmdbx) with typed read/write API

### Infrastructure (`apollo_infra`)

The `apollo_infra` crate defines the inter-component communication framework:
- `ComponentClient<Request, Response>` trait — async `.send()` interface
- `ComponentRequestHandler<Request, Response>` trait — server-side handler
- `ComponentStarter` trait — component startup lifecycle
- Local communication uses tokio `mpsc` channels; remote communication uses HTTP/binary serialization

Components communicate only through their typed request/response enums, never by direct function calls across crate boundaries.

### Configuration (`apollo_config`)

Layered configuration system with ascending priority: default values → config files → environment variables → CLI arguments. The `SequencerNodeConfig` struct aggregates all component configs, each gated by an execution mode.

### Key External Crates
- `blockifier` — Starknet transaction execution engine
- `starknet_api` — Starknet types
- `papyrus_base_layer` — Ethereum L1 interaction
- `apollo_network` — libp2p networking (SQMR protocol + GossipSub)

## Workspace Layout

- `crates/` — all workspace members
- `scripts/` — Python and bash tooling for CI, presubmit, formatting
- `docs/diagrams/` — Mermaid sequence diagrams for key flows (tx submission, block building, consensus, state sync, L1 handling, gas price)
- `deployments/` — Kubernetes/CDK8s deployment configuration
- `workspace_tests/` — cross-crate integration tests
