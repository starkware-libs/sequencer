# Devops-side `mandatory` refactor — plan

Match the `sequencer-devops` jsonnet to the sequencer-repo `mandatory` refactor (topology now lives in
`chain_params.mandatory`; `topology.jsonnet` deleted from the sequencer repo).

## Repo facts (verified)
- Repo: `/home/nimrod/workspace/sequencer-devops`, branch `main-v0.14.3`.
- **15 `node.jsonnet`** (untracked WIP): `mainnet` ×6, `sepolia-alpha` ×6, `sepolia-integration` ×3, at
  `sequencer/configs/overlays/hybrid/<env>/apollo-<env>-<N>/node.jsonnet`.
- **3 `private_chain_params.jsonnet`** (untracked): `(import './chain_params.jsonnet') + { <flat
  consensus_/mempool_ bootstrap_peer_multiaddr> }`.
- `chain_params.jsonnet` / `topology.jsonnet` are **not in the devops repo** — copied from the sequencer
  repo overlay dir at deploy (per the node.jsonnet header comment).
- No devops Makefile/CI/test evaluates the jsonnet; only `scripts/format-fix.sh` exists.
- Every node.jsonnet is structurally identical except `validator_id` and the `std.split(...)[N]` index;
  the topology lines are byte-identical across all 15.

## Current node.jsonnet (all 15)
```jsonnet
// Full per-service SequencerNodeConfig for apollo-<env>-<N>.
// Evaluate with the jsonnet CLI, JPATH pointed at the sequencer jsonnet dir:
//   jsonnet -J <repo>/crates/apollo_deployments/jsonnet node.jsonnet
// `../chain_params.jsonnet` and `../topology.jsonnet` are copied from the sequencer repo at deploy
// time (exactly like private_chain_params.jsonnet composes ./chain_params.jsonnet).
local build = import 'lib/build.libsonnet';
local chain_params = import '../private_chain_params.jsonnet';
local topology = import '../topology.jsonnet';
local node_params = {
  validator_id: '0x…',
  consensus_advertised_multiaddr: std.split(chain_params.consensus_bootstrap_peer_multiaddr, ',')[N],
  mempool_advertised_multiaddr: std.split(chain_params.mempool_bootstrap_peer_multiaddr, ',')[N],
};
build.build({ chain_params: chain_params + { topology: topology }, node_params: node_params })
```

## The change (uniform across all 15 node.jsonnet)
1. **Delete** `local topology = import '../topology.jsonnet';`.
2. **Drop the topology merge:**
   `build.build({ chain_params: chain_params + { topology: topology }, node_params: node_params })`
   → `build.build({ chain_params: chain_params, node_params: node_params })`
   (topology now rides inside the copied `chain_params.jsonnet`'s `mandatory` block, via
   `private_chain_params.jsonnet`).
3. **Comment fixes** (the topology + path lines):
   - `-J <repo>/crates/apollo_deployments/jsonnet` → `-J <repo>/deployments/sequencer/configs/jsonnet`
     (stale after the sequencer move — fix while here).
   - "`../chain_params.jsonnet` and `../topology.jsonnet` are copied …" → only `../chain_params.jsonnet`
     is copied (it now carries the topology in `mandatory`).

`validator_id`, both `std.split(... bootstrap_peer_multiaddr ...)[N]` lines, and the `build`/`chain_params`
imports are **untouched** — the split still reads the flat `consensus_/mempool_bootstrap_peer_multiaddr`
from `private_chain_params.jsonnet` (multiaddrs stay flat, not under `mandatory`).

## Unchanged
- **`private_chain_params.jsonnet` ×3** — still `(import './chain_params.jsonnet') + { <flat multiaddrs> }`.
  The copied `chain_params.jsonnet` now has a `mandatory` block, but private_chain_params only adds the
  flat bootstrap multiaddrs on top; `build.libsonnet` flattens `mandatory` and reads them flat. No change.

## Deploy flow (corrected — `.github/workflows/sequencer_argocd_deployment.yaml`)
The `Copy cdk8s overlays` step (line 242) copies the **devops** overlays INTO the **public** sequencer repo:
```
cp -rf <devops>/sequencer/configs/overlays  <public>/deployments/sequencer/configs/
```
So the direction is devops → public. The devops overlays supply the per-node `apollo-<env>-<N>/node.jsonnet`,
`private_chain_params.jsonnet`, and `common_*.yaml`. **`chain_params.jsonnet` and `topology.jsonnet` are
NOT copied from devops — they live in the public repo** (the env overlay). The devops per-node node.jsonnet
composes them via the co-located `../private_chain_params.jsonnet` → `./chain_params.jsonnet` after the copy
lands them in the same env dir. cdk8s synth then runs in the public repo; `configmap.py` resolves the leaf
node.jsonnet under `configs/jsonnet/overlays/<dotted-overlay>/node.jsonnet` and imports `lib/` from
`configs/jsonnet`.

**Consequence for the mandatory refactor:** the copy step needs **no topology-specific change**.
`topology.jsonnet` is public-repo-side and is deleted by the sequencer refactor; the devops node.jsonnet
just stops importing it, and its composed `chain_params.jsonnet` (public, co-located) now carries
`mandatory.topology`. Nothing in the workflow references `topology.jsonnet` by name.

## 🔴 Move-consequence in the SAME workflow (separate from mandatory, but blocks deploy)
The sequencer move split the public overlays: YAML stayed at `configs/overlays/`, jsonnet went to
`configs/jsonnet/overlays/`, and `configmap.py` now resolves node.jsonnet under **`configs/jsonnet/overlays`**.
But `Copy cdk8s overlays` still copies the devops overlays to **`configs/overlays`** (line 242 target, plus
the path echoes at lines 126–127, 143–145, 240, 245–256). Post-move, the copied per-node `apollo-*/node.jsonnet`
would land at `configs/overlays/…` while synth reads `configs/jsonnet/overlays/…`, and the node.jsonnet's
`../chain_params.jsonnet` would not resolve to the public chain_params. **The copy target must move to
`deployments/sequencer/configs/jsonnet/overlays` (i.e. `cp … <public>/deployments/sequencer/configs/jsonnet/`)**
so the devops per-node overlays co-locate with the public `chain_params.jsonnet`.
- This is a **move** consequence, not the mandatory change — but the mandatory refactor won't deploy until it's
  fixed. Confirm whether the move's ArgoCD-workflow rewiring is already tracked; if not, it belongs with the
  move rollout. (Note: the workflow lives on devops branch `main-v0.14.3`, currently the pre-move target path.)
- Also update the node.jsonnet header `-J` comment path (`crates/apollo_deployments/jsonnet` →
  `deployments/sequencer/configs/jsonnet`) — cosmetic, done as part of the per-file edit below.

## Verification (per env — simulate the deploy composition)
For each of the 3 envs, reproduce what deploy assembles and prove parity:
1. Copy the **new** sequencer `deployments/sequencer/configs/jsonnet/overlays/hybrid/<env>/chain_params.jsonnet`
   → `sequencer-devops/…/overlays/hybrid/<env>/chain_params.jsonnet`.
2. Eval each node.jsonnet: `jsonnet -J <sequencer>/deployments/sequencer/configs/jsonnet <devops>/…/apollo-<env>-<N>/node.jsonnet` → must produce the per-service map (6 services, hybrid).
3. **Parity:** compare against the OLD composition (pre-refactor sequencer chain_params.jsonnet +
   topology.jsonnet + old node.jsonnet) — output must be **byte-identical** per node.
   (The node_params `advertised_multiaddr` derivation is unchanged, so per-node output should match exactly.)

## Landing
- Single devops commit on `main-v0.14.3` editing the 15 node.jsonnet (these are currently untracked WIP —
  they'd be committed as part of your devops jsonnet-migration commit). No graphite stack (separate repo).
- **Lockstep with the sequencer merge:** the copied `chain_params.jsonnet` only carries `mandatory.topology`
  once the sequencer change is merged, and `topology.jsonnet` only disappears then. Land together (or the
  devops side first would still import a `../topology.jsonnet` that the copy step must still provide until
  cutover — so cut over both at once).

## Risks
1. **Move-consequence copy target (HIGH, but move-scope not mandatory-scope)** — `Copy cdk8s overlays`
   target `configs/overlays` → `configs/jsonnet/overlays`. Without it the copied per-node node.jsonnet
   won't be found by synth / won't resolve `../chain_params.jsonnet`. Confirm it's tracked with the move.
2. **Lockstep timing (HIGH)** — devops node.jsonnet without the topology import needs the (public,
   co-located) `chain_params.jsonnet` to already carry `mandatory.topology`, i.e. the sequencer change must
   be merged. Cut over both sides together.
3. **Low** — no workflow change is needed for topology itself (it was never devops-copied); the `-J`
   comment path and the split-index/validator_id lines are untouched by the topology edit.
