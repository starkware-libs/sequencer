<identity>
Apollo sequencer: a 107-crate Rust workspace for Starknet sequencing, consensus, execution, storage, networking, and deployment.
</identity>

<context_routing>
Workspace scale: 107 crates, 1423 Rust files, ~292k Rust LOC. Do not load broad context by default.

Start every task with this routing flow:
1. Identify the target branch and touched crates before reading implementation files.
2. Load only the matching skills from `.codex/skills/`:

| Task shape | Load |
|-----------|------|
| Any code change | `testing-and-presubmit` |
| Bug, panic, timeout, regression | `debugging` |
| Component API or node wiring | `component-development` |
| Batcher, gateway, mempool, consensus, proposal flow | `consensus-and-block-building` |
| Storage, state, global root, Patricia, committer | `storage-and-state` |
| P2P, protobuf, mempool_p2p, p2p_sync | `network-and-protobuf` |
| Topology, execution mode, cdk8s, service layout | `deployment-topology` |
| Benchmarking, perf regression, Criterion | `performance-and-benchmarks` |
| Release branch, backport, artifacts, CI/publish | `release-branching-and-artifacts` |

3. Prefer crate-local edits. Only read `apollo_node`, `apollo_node_config`, `apollo_deployments`, `.github`, or `scripts/` when the task crosses crate boundaries.
</context_routing>

<stack>
| Area | Technology | Version | Notes |
|------|------------|---------|-------|
| Runtime | Rust | 1.92 stable | root `rust-toolchain.toml` |
| Special-case runtime | Rust nightly | `nightly-2025-07-14` | `crates/starknet_transaction_prover` only |
| Edition | Rust edition | 2021 | workspace-wide |
| Async | Tokio | workspace | local component IPC uses mpsc |
| Remote transport | Hyper HTTP/2 + bincode | workspace | typed request/response transport |
| Storage | libmdbx | workspace | sequencer state and block data |
| Commitment storage | RocksDB | workspace | Patricia/global-root path in committer |
| P2P | libp2p + Protobuf | workspace | gossipsub + SQMR |
| Testing | `cargo test` + `cargo-nextest` | workspace | `scripts/run_tests.py` drives changed-only CI parity |
| Formatting | rustfmt | `nightly-2024-04-29` | via `scripts/rust_fmt.sh` |
| TOML formatting | taplo-cli | 0.9.3 | check-only via `scripts/taplo.sh` |
| Python tooling | Python | 3.9/3.10 [verify] | 3.9 for Cairo/OS CI helpers, 3.10 for `deployments/sequencer` CDK8s |
| Deployment tooling | Pipenv + CDK8s CLI + Node | Node 22 in CI | deployment synthesis only |
</stack>

<structure>
```
crates/                         # Primary Rust workspace [agent: create/modify]
├── apollo_*                    # Sequencer components, config crates, type crates
├── blockifier                  # Execution engine [modify with care]
├── starknet_*                  # Shared Starknet primitives / commitment / OS [modify with care]
├── papyrus_*                   # Base-layer integration
└── *_test_utils / integration  # Test harness crates
workspace_tests/                # Workspace-level invariant tests [agent: create/modify]
scripts/                        # CI, presubmit, branch merge, Python helpers [agent: gated]
deployments/                    # Docker + CDK8s + runtime config [agent: gated]
.github/                        # CI and artifact workflows [agent: gated]
docs/diagrams/                  # Flow diagrams [agent: create/modify]
echonet/                        # Separate monitoring/smoke-test tooling [agent: modify with care]
.codex/skills/                  # Canonical repo-local skills [agent: create/modify]
```

High-fanout boundaries:
- `crates/*_types/src/communication.rs`: component interfaces and transport contract.
- `crates/apollo_node/src/{communication,clients,components,servers}.rs`: whole-node assembly.
- `crates/apollo_node_config/src/component_execution_config.rs`: execution-mode contract.
- `crates/apollo_deployments/src/deployments/*.rs`: service topology contract.
- `crates/apollo_protobuf/src/proto/**`: wire-format contract.
</structure>

<commands>
| Task | Command | Notes |
|------|---------|-------|
| Install Python helpers | `python3 -m pip install -r scripts/requirements.txt` | Use a venv locally if possible |
| Build one crate | `cargo build -p <crate>` | stable toolchain unless noted otherwise |
| Check one crate | `cargo check -p <crate>` | fastest compile sanity check |
| Test one crate | `SEED=0 cargo test -p <crate>` | deterministic ordering |
| Workspace invariants | `cargo test -p workspace_tests` | manifest/lint/package integrity |
| Changed tests | `SEED=0 python3 scripts/run_tests.py --command test --changes_only --include_dependencies --commit_id <base_sha>` | use PR base or merge-base, never `HEAD` |
| Changed clippy | `python3 scripts/run_tests.py --command clippy --changes_only --commit_id <base_sha>` | same base-sha rule |
| Changed docs | `python3 scripts/run_tests.py --command doc --changes_only --commit_id <base_sha>` | same base-sha rule |
| Changed integration | `SEED=0 python3 scripts/run_tests.py --command integration --changes_only --include_dependencies --commit_id <base_sha>` | builds `apollo_node` + integration binaries |
| Local presubmit | `scripts/local_presubmit.sh [--parent_branch <branch>]` | mirrors main CI; creates its own venv |
| Rustfmt check | `scripts/rust_fmt.sh --check` | installs `nightly-2024-04-29` if needed |
| Rustfmt fix | `scripts/rust_fmt.sh` | same toolchain as CI |
| Taplo check | `scripts/taplo.sh` | check-only |
| Dependency hygiene | `cargo machete` | part of main CI |
| Security/license scan | `cargo deny check` | warning-only in CI today |
| Rebuild protobuf | `cargo clean -p apollo_protobuf && cargo build -p apollo_protobuf` | after `.proto` changes |
| Cairo native check | `cargo check -p blockifier --features cairo_native` | feature-specific code path |
| Prover test | `cd crates/starknet_transaction_prover && cargo test -p starknet_transaction_prover --features stwo_proving --release` | nightly-only crate |
</commands>

<conventions>
  <code_style>
    Naming follows `.claude/rules/code-style.md`: no single-letter locals outside trivial closures, no standalone adjectives, use domain abbreviations only (`tx`, `addr`, `pk`, `ctx`, `msg`).
    Keep `use` statements at module scope. Never add inline `use` inside function bodies.
    Comments explain why, not what. No decorative separator comments.
    User-controlled data must not panic the process: avoid `unwrap()`, `expect()`, and unchecked indexing on request-reachable paths.
    Test-only code belongs behind `#[cfg(any(feature = "testing", test))]`.
  </code_style>

  <patterns>
    <do>
      - Use the 3-crate component pattern: `apollo_X`, `apollo_X_config`, `apollo_X_types`.
      - Keep request/response transport in `communication.rs`; keep business logic in the logic crate.
      - Implement config structs with `SerializeConfig::dump()`.
      - Use `tempfile::TempDir` for any test that opens storage.
      - Treat `apollo_node` as wiring only; prefer fixing logic in the component crate first.
    </do>
    <dont>
      - Don't use `--commit_id HEAD` with `scripts/run_tests.py` for changed-only runs; it produces empty diffs.
      - Don't remove a crate's `testing = []` feature when metrics macros or test-only APIs rely on it.
      - Don't manually edit `crates/apollo_protobuf/src/protobuf/protoc_output.rs`; regenerate it by rebuilding `apollo_protobuf`.
      - Don't add `Copy` without scanning for `.clone()` callsites; clippy treats `clone_on_copy` as a hard failure.
      - Don't widen task scope into `.github`, `scripts`, or `deployments` unless the change truly crosses crate boundaries.
    </dont>
  </patterns>

  <commit_conventions>
    Format: `scope[,scope2,...]: subject`
    No type prefix (`feat`, `fix`, etc.).
    Scope is required and must be a valid crate or one of: `ci`, `deployment`, `release`, `scripts`, `workspace`, `infra`.
    Header max length: 100.
  </commit_conventions>
</conventions>

<workflows>
  <bug_fix>
    1. Reproduce the failure with the smallest crate-level test or command.
    2. If the task is changed-only, compute the real base commit first (`git merge-base HEAD origin/<base_branch>`).
    3. Trace the failing path through trait boundaries before editing code.
    4. Fix the root cause in the narrowest crate possible.
    5. Run crate tests first, then broader verification only if interfaces/manifests changed.
  </bug_fix>

  <component_or_interface_change>
    1. Decide whether the change is logic-only, interface-only, or node-wiring.
    2. For logic-only work, stay inside `crates/apollo_X/`.
    3. For interface work, update `crates/apollo_X_types/src/communication.rs`, the logic handler, and all downstream callsites.
    4. If execution mode, client/server wiring, or service ownership changes, update `apollo_node`, `apollo_node_config`, and topology files together.
    5. Verify with crate tests, `workspace_tests` when manifests/configs changed, and integration tests for cross-component flows.
  </component_or_interface_change>

  <release_branch_task>
    1. Confirm the target base branch before coding; this repo actively maintains multiple `main-v*` branches.
    2. For local presubmit on a non-`main` change, run `scripts/local_presubmit.sh --parent_branch <base_branch>`.
    3. Use `scripts/merge_paths.json` only to understand promotion order. Running `scripts/merge_branches.py` is gated.
    4. Treat artifact upload, Docker publish, and benchmark workflows as supervised work.
  </release_branch_task>
</workflows>

<boundaries>
  <forbidden>
    DO NOT read, modify, or expose:
    - `.env`, `.env.*`
    - `**/*secret*.json`, `**/*secret*.yaml`
    - `**/*key*.json`, `**/*key*.yaml`
    - credentials used for GCS, GHCR, or deployment environments
  </forbidden>

  <zones>
    | Path | Zone | Rule |
    |------|------|------|
    | `crates/apollo_*/src/**`, `workspace_tests/**`, `docs/**`, `.codex/skills/**` | Autonomous | normal implementation, tests, docs, skills |
    | `crates/*_types/src/communication.rs` | Gated | cross-component API and transport contract |
    | `crates/apollo_node/src/{communication,clients,components,servers}.rs` | Gated | rewires the full node |
    | `crates/apollo_node_config/src/component_execution_config.rs` | Gated | execution-mode contract |
    | `crates/apollo_protobuf/src/proto/**` | Gated | wire-format change |
    | `crates/apollo_storage/**`, `crates/apollo_committer/**`, `crates/starknet_patricia*/**` | Autonomous for bug fixes; Gated for on-disk format or storage-contract changes | state compatibility risk |
    | `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `crates/starknet_transaction_prover/rust-toolchain.toml`, `.cargo/config.toml` | Gated | workspace/runtime contract |
    | `.github/**`, `scripts/**`, `deployments/**`, `build_native_in_docker.sh`, `commitlint.config.js` | Gated | CI, release, infra, deployment |
  </zones>

  <safety_checks>
    Before any destructive or high-fanout change:
    1. State the exact files or contracts affected.
    2. State the blast radius (interfaces, storage, protocol, CI, deployment, or release flow).
    3. Wait for approval if the change is in a gated zone.
  </safety_checks>
</boundaries>

<troubleshooting>
  <known_issues>
    | Symptom | Likely cause | Fix |
    |---------|--------------|-----|
    | `No changes detected.` from `scripts/run_tests.py` | wrong `--commit_id` | use PR base SHA or `git merge-base`; never pass `HEAD` |
    | `check-cfg` error on feature `testing` | crate uses metrics/test-only expansion without a declared feature | add `testing = []` to `[features]` |
    | Proto compile or deserialize failure after `.proto` edit | stale generated protobuf output | `cargo clean -p apollo_protobuf && cargo build -p apollo_protobuf` |
    | Integration binary timeout | missing dependent build, port conflict, or miswired topology | rebuild binaries, check ports/config, rerun focused integration flow |
    | Storage open failure / exclusive lock error | MDBX only allows one writer environment | use per-test `TempDir`; avoid reusing paths across processes |
    | Rustfmt toolchain missing | local machine lacks pinned nightly rustfmt | rerun `scripts/rust_fmt.sh`; it installs `nightly-2024-04-29` |
    | `Change file not found` in benchmark compare | no Criterion baseline exists | run `bench_tools -- run` first, then `run-and-compare` |
  </known_issues>

  <recovery_patterns>
    1. Read the full error message.
    2. Confirm the referenced file, crate, or binary actually exists.
    3. Re-run the smallest relevant build or test for one crate.
    4. Check `git status` and verify you are diffing against the correct base branch.
    5. If the failure crosses component boundaries, load the matching skill before continuing.
  </recovery_patterns>
</troubleshooting>

<environment>
  Designed for shell-first coding agents (Codex, Claude Code, similar tools).
  Repo scope: current checkout only. Private sibling repos are not present here by default.
  Expected tools: `git`, `cargo`, `rustup`, `python3`, and optionally `cargo-nextest`, `cargo-machete`, `cargo-deny`, `taplo`.
  Some CI workflows also require `pypy3.9`, GCS auth, Docker, Pipenv, CDK8s CLI, and GitHub CLI.
</environment>

<skills>
  Repo-local skills live in `.codex/skills/` and are symlinked into `.claude/skills/` and `.agents/skills/`.
  Load only the skill(s) that match the current task.

  | Skill | Path | Use when |
  |-------|------|----------|
  | Component Development | `.codex/skills/component-development/SKILL.md` | component API, 3-crate pattern, node wiring |
  | Testing and Presubmit | `.codex/skills/testing-and-presubmit/SKILL.md` | selecting verification commands |
  | Debugging | `.codex/skills/debugging/SKILL.md` | root-cause analysis |
  | Consensus and Block Building | `.codex/skills/consensus-and-block-building/SKILL.md` | batcher/gateway/mempool/consensus changes |
  | Storage and State | `.codex/skills/storage-and-state/SKILL.md` | MDBX, RocksDB, Patricia, global-root work |
  | Network and Protobuf | `.codex/skills/network-and-protobuf/SKILL.md` | P2P, protobuf, wire format, p2p sync |
  | Deployment Topology | `.codex/skills/deployment-topology/SKILL.md` | execution mode, service layout, cdk8s |
  | Performance and Benchmarks | `.codex/skills/performance-and-benchmarks/SKILL.md` | Criterion and perf-regression work |
  | Release, Branching, and Artifacts | `.codex/skills/release-branching-and-artifacts/SKILL.md` | backports, release branches, artifacts, CI publish |
</skills>

<memory>
  - [docs/agent-memory/project-decisions.md](docs/agent-memory/project-decisions.md): durable architectural and context decisions. Read before changing repo-wide rules, boundaries, branch workflow, or skill architecture.
  - [docs/agent-memory/lessons-learned.md](docs/agent-memory/lessons-learned.md): operational gotchas and recurring patterns discovered while working in the repo. Read when debugging, touching shared wiring, or widening verification scope.
</memory>
