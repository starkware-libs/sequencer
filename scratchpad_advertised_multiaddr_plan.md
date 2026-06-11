# Derive `advertised_multiaddr` from `nodes_at_same_cluster` + `node_index` — plan

## Goal
Remove `consensus_advertised_multiaddr` & `mempool_advertised_multiaddr` from `node_params`. Derive each
node's advertised multiaddr **inside the applicative components** from:
- `chain_params.nodes_at_same_cluster` (the boolean added by the mandatory refactor), and
- `chain_params.<proto>_bootstrap_peer_multiaddr`, split by a new per-node `node_params.node_index`.

Rule (user spec): `advertised = if nodes_at_same_cluster && bootstrap != null then std.split(bootstrap, ',')[node_index] else null`.

## Current data flow (verified at HEAD)
- `node_params` = `{ validator_id, consensus_advertised_multiaddr, mempool_advertised_multiaddr }`.
- Only **3** reads of `node_params` in the whole jsonnet tree:
  - `consensus_manager.libsonnet:40` → `validator_id` (unchanged)
  - `consensus_manager.libsonnet:94` → `consensus_advertised_multiaddr`
  - `mempool_p2p.libsonnet:7` → `mempool_advertised_multiaddr`
  - (`state_sync.libsonnet` hardcodes `advertised_multiaddr: null`; does NOT read `node_params`.)
- The per-node split currently lives in the **devops** `node.jsonnet`:
  `std.split(chain_params.<proto>_bootstrap_peer_multiaddr, ',')[N]`.
- `<proto>_bootstrap_peer_multiaddr` is NOT in `default_replacers`; every producer supplies it flat
  (devops `private_chain_params` = real; dummy/testing = `null`). The component already reads it
  unconditionally, so it is always present.

## The `node_params` reshape
`node_params` → `{ validator_id, node_index }`. The two `*_advertised_multiaddr` fields are deleted; a
single integer `node_index` replaces them. **Verified**: for all 15 devops nodes the consensus split
index == the mempool split index, so ONE `node_index` covers both protocols.

`node_index` per producer:
- testing / dummy / testing-overlays: `0` (single-node, and bootstrap is `null` so it is never read).
- devops: the node's position — mainnet/alpha `0..5`, integration `0..2` (matches the current `[N]`).

## The component change (both files)
`consensus_manager.libsonnet:94`:
```jsonnet
      advertised_multiaddr: advertisedMultiaddr(
        chain_params.consensus_bootstrap_peer_multiaddr, node_params.node_index, chain_params.nodes_at_same_cluster
      ),
```
`mempool_p2p.libsonnet:7`: same, with `mempool_bootstrap_peer_multiaddr`.

Shared helper (new `lib/applicative_config/advertised_multiaddr.libsonnet`, imported by both — DRY over the
two identical sites; inlining the ternary in each is an acceptable alternative):
```jsonnet
function(bootstrap_peer_multiaddr, node_index, nodes_at_same_cluster)
  if nodes_at_same_cluster && bootstrap_peer_multiaddr != null
  then std.split(bootstrap_peer_multiaddr, ',')[node_index]
  else null
```
`chain_params.nodes_at_same_cluster` is present by convention (every `mandatory` block has it; `build.libsonnet`
asserts `mandatory` + `mandatory.topology` but not this field) and `build.libsonnet:29` flattens `mandatory`
(`defaultReplacers + mandatory + chain_params`) so it reads flat inside the component.

**Path note (stack-relative):** on every branch I edit (`applicative-jsonnet` … `testing-overlays-native-layers`)
the jsonnet tree lives at `crates/apollo_deployments/jsonnet/`. The move to
`deployments/sequencer/configs/jsonnet/` happens later in the near-top `common_dir` commit; these edits ride
that move. Component files are at `crates/apollo_deployments/jsonnet/lib/applicative_config/components/`.

## 🔴 Load-bearing decision: `nodes_at_same_cluster` values (this refactor is NOT output-preserving under all-`false`)
The current advertised behavior **already differs by env**:

| env | current `advertised` | bootstrap | value needed to PRESERVE output |
|---|---|---|---|
| mainnet | `split[N]` (non-null) | real | **`true`** ← flip from `false` |
| sepolia-alpha | `split[N]` (non-null) | real | **`true`** ← flip from `false` |
| sepolia-integration | `null` | real | `false` (unchanged) |
| testing / dummy / testing-overlays | `null` | `null` | `false` (guard → `null` either way) |

The mandatory refactor set `nodes_at_same_cluster: false` **everywhere** as an unused placeholder. Wiring it
up means **mainnet + sepolia-alpha must become `true`** or those nodes stop advertising (real prod behavior
change). This is the one decision that changes the plan; the recommendation is the output-preserving
mapping above (`true`/`true`/`false`).

## Where each change lands in the stack
The components are pure functions introduced in the first PR (`applicative-jsonnet`) and first **evaluated**
at `node-config-deserialization` (`5eff0bc9d8`) — that branch introduces `testing/chain_params.libsonnet` +
`testing/node_params.libsonnet`, the only inputs to any `build()`/`applicative()` evaluation in the suite.
Below it there is no `build_service_configs` (nothing to import), so the infra-parity test evaluates the
layout jsonnet directly and never touches the components. So the edited components sit unevaluated until
`node-config-deserialization`, where their inputs (`node_index`, `mandatory.nodes_at_same_cluster`) exist:

1. **`applicative-jsonnet`** (`5aafa73228`, FIRST PR) — the two component edits, per user directive:
   - Edit `consensus_manager.libsonnet:94` + `mempool_p2p.libsonnet:7` (the derivation).
   - Add the `lib/applicative_config/advertised_multiaddr.libsonnet` helper (imported by both).
   - Lazily safe: nothing evaluates the components in this PR or the next two branches.
2. **`node-config-deserialization`** (`5eff0bc9d8`) — first evaluation point:
   - `testing/node_params.libsonnet`: drop the 2 advertised fields, add `node_index: 0`.
   - (`testing/chain_params.libsonnet` already has `mandatory.nodes_at_same_cluster: false`; bootstrap
     `null` ⇒ advertised `null` — output preserved, no change needed.)
3. **`native-override-mainnet`** (`6320e58fbe`) — `mainnet/chain_params.jsonnet`:
   `mandatory.nodes_at_same_cluster: false → true`.
4. **`native-override-alpha`** — `sepolia-alpha/chain_params.jsonnet`: `false → true`.
5. **`native-override-integration`** — `sepolia-integration/chain_params.jsonnet`: **no change** (`false`).
6. **`dummy-for-testing-native-layer`** (`79629dbaec`) — `dummy_for_testing/node_params.jsonnet`: drop 2
   advertised, add `node_index: 0`. (Optional cleanup: remove the dead flat `*_advertised_multiaddr` keys
   from `dummy_for_testing/chain_params.jsonnet` — nothing reads them; output-neutral.)
7. **`testing-overlays-native-layers`** (`9b5e759a1b`) — inline `node_params` in
   `testing/node-0/node.jsonnet` and `testing/all-constructs/node.jsonnet`: drop 2 advertised, add
   `node_index: 0`. (Optional cleanup: drop the dead flat `*_advertised_multiaddr` keys from their
   `chain_params.jsonnet`.)
8. **Devops repo (WIP, `main-v0.14.3`)** — all 15 `apollo-*/node.jsonnet`: replace the two
   `*_advertised_multiaddr: std.split(...)[N]` lines with `node_index: N`.

Nothing on the Rust side changes: `node_params` is jsonnet-only build() input; the output
`network_config.advertised_multiaddr` stays `Option<Multiaddr>` (null or a valid multiaddr string). The
`build_*_deserializes_into_node_config` tests keep passing.

## Verification
- **Sequencer overlays**: byte-parity vs the pre-refactor tip for all 5 overlays (`jsonnet -J` + `jq -S`
  diff). Under `true`/`true`/`false`, every evaluated producer has `null` bootstrap ⇒ advertised `null`
  ⇒ identical. (mainnet/alpha CI stand-in uses dummy `null` bootstrap, so `true` doesn't change it.)
- **Testing producer**: `SEED=0 cargo test -p apollo_deployments` (the `build_*_deserializes` trio).
- **Devops composition** (the real parity target): reuse `scratchpad/devops_verify.py`, extended to
  reconstruct the OLD node.jsonnet (two `std.split(...)[N]` lines) from the NEW (`node_index: N`) and diff
  — must be 15/15 byte-identical. mainnet/alpha: `true` + real bootstrap ⇒ `split[N]` (same). integration:
  `false` ⇒ `null` (same). This is where the flip actually preserves prod output.
- `unset CI && scripts/rust_fmt.sh`; jsonnet fmt if a formatter is wired.

## Landing / lockstep
- Sequencer: amend into the branches above via `gt ca -a` + `gt restack`; submit with the stack when ready.
- Devops: single commit on `main-v0.14.3`, **lockstep with the sequencer merge** (the devops
  `node_index` form only produces the right advertised value once the public `chain_params.jsonnet` carries
  the `true`/`true`/`false` `nodes_at_same_cluster` values via the deploy copy).
