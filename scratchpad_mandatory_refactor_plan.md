# chain_params `mandatory` refactor — distributed execution plan

## Decisions (locked)
- **mandatory set** = `chain_id, starknet_url, recorder_url, starknet_contract_address, base_layer,
  staking_default_committee, proof_archive_bucket_name, nodes_at_same_cluster, topology`
  (8 fields + topology; `recorder_url` added to the user's original 6).
- **`nodes_at_same_cluster`** = new `bool`, **currently unused** (no component / `build.libsonnet` reads
  it yet — it flattens into `chainParams` harmlessly). Value `false` in **every** env
  (mainnet, sepolia-alpha, sepolia-integration, testing/node-0, testing/all-constructs, and
  `testing/chain_params.libsonnet`). Lives in `mandatory` (no `default_replacers` fallback), so every
  `chain_params` producer must supply it.
- **Bootstrap multiaddrs stay flat** (`consensus_bootstrap_peer_multiaddr`,
  `mempool_bootstrap_peer_multiaddr`) — devops `node.jsonnet` reads them flat via `std.split(...)`.
- **Landing** = distributed into the branch that introduced each file (not a top-of-stack commit).

## Target shape

`chain_params.jsonnet` (per env):
```jsonnet
{
  mandatory: {
    chain_id: 'SN_MAIN',
    starknet_url: '...',
    recorder_url: '...',
    starknet_contract_address: '...',
    base_layer: constants.ETH_MAINNET_BASE_LAYER,
    staking_default_committee: '...',
    proof_archive_bucket_name: '...',
    nodes_at_same_cluster: false,   // currently unused; false in every env
    topology: import 'lib/layouts/hybrid.libsonnet',
  },
  // optional overrides (unchanged, flat) — each falls back to default_replacers
  native_classes_whitelist: '...',
  ...
}
```

`lib/build.libsonnet`:
```jsonnet
assert std.objectHas(params.chain_params, 'mandatory') : 'params.chain_params.mandatory is required';
assert std.objectHas(params.chain_params.mandatory, 'topology') : 'params.chain_params.mandatory.topology is required';
local topology = params.chain_params.mandatory.topology;
local chainParams = defaultReplacers + params.chain_params.mandatory + params.chain_params;
```
Output is byte-identical: `mandatory` is flattened before components read `chain_params.X`. No component
libsonnet, no Python, no Rust consumer of `build()` output changes. `nodes_at_same_cluster` adds one
key to `chainParams` that no component reads, so per-service output stays byte-identical too.
`build.libsonnet` asserts only `mandatory` + `mandatory.topology`; it does not assert per-field, so the
new field needs no assertion.

Rust topology injection changes `{ chain_params+: { topology: <layout> } }`
→ `{ chain_params+: { mandatory+: { topology: <layout> } } }`.

## ⚠️ PATHS DIFFER PER BRANCH (audit fix — load-bearing)
On every landing branch **below the tip**, the jsonnet tree is at **`crates/apollo_deployments/jsonnet/`**
and the Rust evaluator is **`crates/apollo_deployments/src/jsonnet.rs`**. The tip commit `common_dir`
(3a8ef25270) renames the tree to `deployments/sequencer/configs/jsonnet/`; the `jsonnet.rs` →
`jsonnet_eval.rs`/`jsonnet_tests.rs` split happens at `harness-expose-evaluator`. So:
- node-config-deserialization, applicative-defaults-parity → edit `crates/apollo_deployments/jsonnet/...`
  and `crates/apollo_deployments/src/jsonnet.rs`.
- harness-expose-evaluator and up → `jsonnet_eval.rs` + `jsonnet_tests.rs`.
- The `deployments/sequencer/configs/jsonnet/...` paths exist only at the tip. `gt restack` carries the
  rename forward automatically.

Real bottom-up ancestry (edited branches in **bold**; others get no edit but must stay green):
**build-node-config** → **node-config-deserialization** → **applicative-defaults-parity** → add-cli-arg
→ storage-reader-ports → config-format-arg → native-config-builder → **native-override-integration** →
**native-override-alpha** → **native-override-mainnet** → extract-base-layer-constants →
**harness-expose-evaluator** → config-serde-symmetric → harness-native-config → native-sole-load →
**dummy-for-testing-native-layer** → **testing-overlays-native-layers** → **retire-preset-config-path**.

## Ordered edits (bottom-up so `gt restack` flows upward)

| # | Branch | Edit | Verify |
|---|--------|------|--------|
| 1 | `build-node-config` | `build.libsonnet` → read `mandatory` + flatten (above). No caller here → safe. | `cargo build -p apollo_deployments` |
| 2 | `node-config-deserialization` | `testing/chain_params.libsonnet`: wrap the 7 fields in `mandatory {}` (recorder_url already present; multiaddrs stay flat). Rust build() eval (in `jsonnet.rs` at this branch) → inject topology under `mandatory+`. | `SEED=0 cargo test -p apollo_deployments` |
| 3 | `applicative-defaults-parity` | Restack-conflict resolution on the Rust injection (it re-authors `jsonnet.rs`, calls `eval_build("consolidated")`). Re-apply `mandatory+`. | `SEED=0 cargo test -p apollo_deployments` |
| 4 | `native-override-integration` | `sepolia-integration/chain_params.jsonnet` → `mandatory` block incl. `topology: import 'lib/layouts/hybrid.libsonnet'`. Delete `sepolia-integration/topology.jsonnet` (unused until node.jsonnet). | eval below |
| 5 | `native-override-alpha` | same for `sepolia-alpha` + delete its `topology.jsonnet`. | eval below |
| 6 | `native-override-mainnet` | same for `mainnet` + delete its `topology.jsonnet`. | eval below |
| 7 | `harness-expose-evaluator` | Restack-conflict resolution: `jsonnet_eval.rs` re-authors the injection (`build_service_configs`, `jsonnet_tests.rs`) → `mandatory+`. | `SEED=0 cargo test -p apollo_deployments --features testing` |
| 8 | `dummy-for-testing-native-layer` | Delete orphan `dummy_for_testing/topology.jsonnet` (imported by no one). | synth |
| 9 | `testing-overlays-native-layers` | `node-0` + `all-constructs` `chain_params.jsonnet` → `mandatory` block (+topology). Author their `node.jsonnet` **without** `+ { topology }`. Delete both `topology.jsonnet`. | pytest + synth |
| 10 | `retire-preset-config-path` | Prod `node.jsonnet` (introduced here) authored **without** `+ { topology: import './topology.jsonnet' }` — `build.build({ chain_params: (import './chain_params.jsonnet') + dummy_multiaddrs, node_params: dummy_node_params })`. | synth all prod overlays |

After each edit: `gt ca -a` (amend) then `gt restack` + `gt continue` on conflicts. Watch the Rust
injection conflicts at #3 and #7 specifically.

## Intermediate branches to green-check (get no edit but must still pass)
`add-cli-arg`, `storage-reader-ports`, `config-format-arg`, `native-config-builder`,
`extract-base-layer-constants`, `config-serde-symmetric`, `harness-native-config`, `native-sole-load`.
`native-config-builder` (introduces `native.py`) is the one to watch — confirm whatever node.jsonnet /
fixture its tests evaluate is mandatory-shaped by the time the restack reaches it.

## Per-overlay eval parity check (byte-identical proof)
For each overlay, before/after must match. Reconstruct HEAD output, then compare after each branch edit:
```bash
cd deployments/sequencer/configs/jsonnet
jsonnet -J . overlays/hybrid/mainnet/node.jsonnet          # + sepolia-alpha, sepolia-integration, testing/*
```
And the Rust harness: `build('hybrid'|'consolidated'|'distributed')` deserializes into `SequencerNodeConfig`
(the `build_*_deserializes_into_node_config` guards).

## ⚠️ Cross-repo lockstep — devops repo (separate commit, MUST land together)
`chain_params.jsonnet` + `topology.jsonnet` are copied from this repo into the devops overlay at deploy.
**15 `node.jsonnet` files** (6 mainnet + 6 sepolia-alpha + 3 sepolia-integration; mainnet IS present),
branch `main-v0.14.3`. Each does `local chain_params = import '../private_chain_params.jsonnet';` +
`local topology = import '../topology.jsonnet';` + `build.build({ chain_params: chain_params + { topology:
topology }, node_params })`.

Required edit, identical in all 15:
- Drop `local topology = import '../topology.jsonnet';` and change
  `build.build({ chain_params: chain_params + { topology: topology }, node_params })`
  → `build.build({ chain_params: chain_params, node_params })` (topology now inside the copied
  `chain_params.jsonnet`'s `mandatory` block via `private_chain_params.jsonnet`).
- `private_chain_params.jsonnet` **unchanged** — still `(import './chain_params.jsonnet') + { <flat
  bootstrap multiaddrs> }`; the copied `chain_params.jsonnet` now carries `mandatory.topology`.
- Node-params `std.split(chain_params.consensus_bootstrap_peer_multiaddr, ',')` keeps working (flat).

If the devops commit does not land with the sequencer change, **every prod deploy breaks**.

## 🔴 UNVERIFIED (audit) — deploy-time copy mechanism
The auditor searched both repos (`.sh/.py/.yaml/.yml`) and could NOT find the step that copies
`chain_params.jsonnet`/`topology.jsonnet` into the devops overlay — it likely lives in external CI/CD.
**Deleting `topology.jsonnet` is only deploy-safe if that mechanism does not *expect* the file.** Confirm
where the copy happens before these branches merge/deploy. (Harmless if the copy still runs but the file
is just no longer imported; breaks deploy only if a script errors on its absence.)

## Stale docs to update (schedule)
- `native.py:4` docstring `build({ chain_params: chain_params + { topology: topology }, ... })` →
  `mandatory` shape. Lands at `native-config-builder` (where native.py is introduced).
- `jsonnet_eval.rs:24` doc "layout folded into `chain_params.topology`" and `jsonnet_tests.rs:58`
  comment → `chain_params.mandatory.topology`. Lands at `harness-expose-evaluator`.

## Files, by introducing branch (reference)
- `lib/build.libsonnet` — build-node-config
- `testing/chain_params.libsonnet`, Rust build() eval — node-config-deserialization (+ re-authored at
  applicative-defaults-parity, harness-expose-evaluator)
- `overlays/hybrid/mainnet/chain_params.jsonnet` + topology.jsonnet — native-override-mainnet
- `overlays/hybrid/sepolia-alpha/*` — native-override-alpha
- `overlays/hybrid/sepolia-integration/*` — native-override-integration
- prod `node.jsonnet` ×3 — retire-preset-config-path
- `overlays/hybrid/testing/{node-0,all-constructs}/*` — testing-overlays-native-layers
- `overlays/hybrid/common/dummy_for_testing/topology.jsonnet` (delete) — dummy-for-testing-native-layer
- devops `node.jsonnet` ×N — **sequencer-devops repo**
