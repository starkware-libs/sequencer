---
name: investigate-echonet-resync
description: Investigate why an Echonet resync triggered. Use when someone mentions an Echonet resync, a failing block in an `echonet-*` namespace, or a `block_hash_mismatch` / `transaction_commitment_mismatch` / `echonet_only_revert` / `gateway_error`. The skill sets up port-forwards, prefers GCP Logs Explorer for history, cross-checks the produced block against mainnet, and recognizes recurring failure patterns before reporting back.
---

# Investigate an Echonet Resync

Echonet is a Starknet replay layer. It pulls mainnet blocks from the public feeder gateway, forwards their txs into a local Apollo sequencer pod, and compares the locally-produced block against mainnet. When something diverges — a gateway reject, an echonet-only revert, a block-hash mismatch, a transaction-commitment mismatch — Echonet scales the sequencer to zero, rewinds state, and restarts. **Your job is to figure out what triggered this specific resync.**

You'll be given a namespace and a failure block number. Everything else you discover yourself.

## 1. Discover the environment

Echonet namespaces live in the cluster whose kubeconfig context contains `sequencer-dev` (or whatever the current dev cluster is called). The K8s objects use canonical names:

- **Service:** `echonet` (port `80`)
- **Deployment:** `echonet`
- **Apollo sequencer:** a separate deployment in the same namespace; find it with `kubectl -n <ns> get pods | grep -iE 'apollo|sequencer'`.

```bash
kubectl config current-context                              # confirm you're on the dev cluster
kubectl get ns | grep -i echonet                            # list all echonet-* namespaces
kubectl -n <ns> get deploy,svc,pods                         # what's actually running
```

There can be several echonet namespaces in flight at once (e.g. `echonet-committer3`, `echonet-14-2`); always confirm which one the user named.

## 2. Set up a port-forward (if one isn't already running)

Pick any free local port — the canonical local ports are just convention.

```bash
LOCAL=18083                                                 # or any free port
kubectl -n <ns> port-forward svc/echonet ${LOCAL}:80 \
  >/tmp/pf-${LOCAL}.log 2>&1 &
curl -fsS "http://127.0.0.1:${LOCAL}/echonet/report/ui" >/dev/null && echo OK
```

If the port is occupied by a stale process, find it (`ss -lntp | grep ${LOCAL}` or `lsof -iTCP:${LOCAL}`) and kill it before retrying. Don't reuse a port without confirming what's on it.

## 3. Echonet's HTTP surface

Useful endpoints (all under `http://127.0.0.1:${LOCAL}`):

**Reports / triage**
- `/echonet/report/ui` — interactive UI; lists past resyncs, lets you drill into pre-resync snapshots.
- `/echonet/report/ui/download` — same data as a static archive.
- `/echonet/report` — JSON form of the current live report (post-last-resync state).
- `/echonet/report/text` — plain-text version.
- `/echonet/block_dump?block_number=<N>&kind=<blob|block|state_update>` — the raw payload Echonet stored for block N. Use `blob` for the cende blob, `block` for the feeder-gateway-shaped block, `state_update` for the matching state update.
- `/echonet/get_block_metadata?block_number=<N>` — Echonet's view of block metadata.
- `/echonet/get_tx_block_metadata?tx_hash=<H>` — which mainnet block a given tx came from.
- `/echonet/get_starknet_version` — the version Echonet thinks it's on.

Echonet sometimes evicts old blocks from in-memory storage. If a `/feeder_gateway/get_block` query returns no result, the block may already be archived on the PVC (see §5) or, post-resync, replaced with the now-successful re-run. In that case, the snapshot taken **before** the resync (§5) is the source of truth.

## 4. Logs — prefer GCP, not kubectl

`kubectl logs` only retains a short window (often hours) and is wiped when a pod restarts. Since a resync intentionally restarts the sequencer pod, the logs you actually need are usually already gone from kubectl. **Default to GCP Logs Explorer.**

### GCP Logs Explorer

The cluster's GCP project is whatever project hosts the current kubectl context. Look it up rather than hardcoding:

```bash
# Project of the current cluster:
gcloud container clusters list \
  --filter="name=$(kubectl config current-context | awk -F_ '{print $NF}')" \
  --format='value(name,location,resourceLabels)'
# Or, if you know the context format `gke_<project>_<region>_<cluster>`:
kubectl config current-context | awk -F_ '{print "project="$2, "region="$3, "cluster="$4}'
```

Then either open the Logs Explorer UI in the browser, or query from the CLI:

```bash
PROJECT=<gcp-project>
NS=<namespace>                                              # e.g. echonet-committer3
START="2026-06-07T08:00:00Z"                                # window around the resync
END="2026-06-07T10:00:00Z"

# Echonet (Flask) logs
gcloud logging read \
  "resource.type=\"k8s_container\"
   resource.labels.namespace_name=\"${NS}\"
   resource.labels.container_name=\"echonet\"
   timestamp>=\"${START}\" timestamp<=\"${END}\"" \
  --project="${PROJECT}" --limit=2000 --format='value(timestamp,jsonPayload.message,textPayload)' \
  > /tmp/echonet-${NS}.log

# Apollo sequencer logs — container name varies; check `kubectl -n <ns> get pod <p> -o yaml | grep -A1 'containers:'`
gcloud logging read \
  "resource.type=\"k8s_container\"
   resource.labels.namespace_name=\"${NS}\"
   resource.labels.container_name=~\"apollo|sequencer\"
   timestamp>=\"${START}\" timestamp<=\"${END}\"" \
  --project="${PROJECT}" --limit=5000 --format='value(timestamp,jsonPayload.message,textPayload)' \
  > /tmp/sequencer-${NS}.log
```

Useful filters to layer on:
- Resync triggers: `textPayload=~"Resync triggered|record_resync_cause|gateway_error|Forward failed|mismatch|429"`
- Block builder: `textPayload=~"block_builder|propose_block|consensus|batcher|cende_recorder"`
- Specific tx: `textPayload=~"<tx_hash>"`
- Specific block: `textPayload=~"block.{0,8}<block_number>"`

A useful UI link template (substitute the four bracketed values):
```
https://console.cloud.google.com/logs/query;query=resource.type%3D%22k8s_container%22%0Aresource.labels.namespace_name%3D%22<NS>%22%0Aresource.labels.container_name%3D%22echonet%22?project=<PROJECT>
```

### kubectl logs (fallback only)

Only useful if the resync is very recent and the pod hasn't restarted:

```bash
kubectl -n <ns> logs deploy/echonet --since=2h | \
  grep -E 'Resync triggered|gateway_error|Forward failed|mismatch|429'
kubectl -n <ns> logs <sequencer-pod> --since=2h
```

## 5. Pre-resync report files (PVC)

Before every resync, Echonet writes a snapshot of its live state to disk. These are the most trustworthy record of what the system observed right before it gave up — they survive the resync itself, while in-memory state does not.

```bash
POD=$(kubectl -n <ns> get pod -l app.kubernetes.io/name=echonet -o jsonpath='{.items[0].metadata.name}')
kubectl -n <ns> exec "${POD}" -- ls -la /data/echonet/reports/        # find the snapshot near your timestamp
kubectl -n <ns> cp "${POD}:/data/echonet/reports/<file>" /tmp/        # pull it locally
```

Archived blocks evicted from memory live under the same PVC; if `/feeder_gateway/get_block` returns nothing, check there:

```bash
kubectl -n <ns> exec "${POD}" -- ls /data/echonet/                    # discover the archive dir name
```

## 6. Compare Echonet's block to real mainnet

```bash
N=<failure_block_number>
PORT=<your_local_port>

curl "https://feeder.alpha-mainnet.starknet.io/feeder_gateway/get_block?blockNumber=${N}"        > /tmp/mainnet_block_${N}.json
curl "https://feeder.alpha-mainnet.starknet.io/feeder_gateway/get_state_update?blockNumber=${N}" > /tmp/mainnet_su_${N}.json
curl "http://127.0.0.1:${PORT}/feeder_gateway/get_block?blockNumber=${N}"                        > /tmp/echo_block_${N}.json
curl "http://127.0.0.1:${PORT}/echonet/block_dump?block_number=${N}&kind=block"                  > /tmp/echo_dump_block_${N}.json
curl "http://127.0.0.1:${PORT}/echonet/block_dump?block_number=${N}&kind=state_update"           > /tmp/echo_dump_su_${N}.json
```

Diff `transaction_commitment`, `event_commitment`, `receipt_commitment`, `state_diff_commitment`, `block_hash`, then drill into receipts (`revert_error`, `actual_fee`, `events`, `messages_sent`) and the state diff itself. The differing commitment narrows down which subsystem produced the divergence.

## 7. Code map (anchors in this repo)

- `echonet/transaction_sender.py` — pulls feeder blocks, forwards txs to the local sequencer, evaluates resync policy.
- `echonet/resync.py` — `ResyncPolicy.evaluate` (trigger logic), `ResyncExecutor.execute` (scale-down → wipe → scale-up).
- `echonet/shared_context.py` — all shared mutable state: resync causes, mismatch tracking, block storage.
- `echonet/echo_center.py` — Flask handlers: cende write_blob, `/l1` RPC mock, `/feeder_gateway/*`, report UI.
- `echonet/l1_logic/l1_manager.py`, `l1_blocks.py`, `l1_client.py` — L1_HANDLER lookup against Alchemy.
- Rust sequencer side: `crates/apollo_mempool/`, `crates/apollo_batcher/`, `crates/apollo_consensus_*/`, `crates/apollo_gateway/`.

Look at recent commits on the active echonet branch for context:

```bash
git log --oneline -30 --all -- echonet/
```

## 8. Known failure patterns — check these first

If the symptom matches one of these, name the pattern explicitly in the report and confirm the deployed image actually contains the fix:

```bash
kubectl -n <ns> get pods -o jsonpath='{.items[*].spec.containers[*].image}' | tr ' ' '\n' | sort -u
git log --all --oneline -- echonet/ crates/apollo_mempool/ crates/apollo_batcher/      # cross-reference SHAs
```

Each namespace can run a different image; "fix already merged" doesn't mean "fix already deployed here."

1. **Cairo-native vs CASM revert traces.** `echonet_only_revert` whose `revert_error` differs from mainnet's only by the topmost VM frame (typically missing or extra `pc=…`). The resync clears the cairo-native cache so the second pass falls back to CASM and matches. Not actually a bug in Echonet — a Blockifier divergence.
2. **Alchemy 429 → L1_HANDLER drop.** `transaction_commitment` mismatch on a block that contains an `L1_HANDLER` tx. Look for HTTP `429` in Echonet logs near the failure window. Root cause: `l1_manager.set_new_tx` silently returns when `find_l1_block_for_tx` fails, so the L1_HANDLER never reaches the local block.

## 9. Investigation workflow

1. Open the UI report, find the entry for the failing block. Note the **trigger tx_hash** and **reason** (`gateway_error`, `echonet_only_revert`, `block_hash_mismatch`, `transaction_commitment_mismatch`).
2. Branch on the reason:
   - **`block_hash_mismatch` / `transaction_commitment_mismatch`** — diff Echonet's block vs mainnet's (§6). Isolate the differing field; drill into the txs / events / state diff that produced it.
   - **`gateway_error`** — pull the tx, grep Echonet + sequencer GCP logs (§4) for the tx_hash, identify the gateway response code and message.
   - **`echonet_only_revert`** — fetch `revert_error` from both Echonet (block dump) and mainnet (feeder); diff them. Often the cairo-native pattern (§8.1).
3. Verify the deployed image actually contains any fix you intend to invoke as "explained" (§8).
4. Cross-check against known patterns before assuming a new bug.
5. If the resync already replayed cleanly, that's still worth noting — but a successful retry doesn't make the original divergence "harmless"; it just means it's non-deterministic.

## 10. Safety constraints (hard rules)

- **Never read** secret files: `*secret*`, `*keys.json`, `.env*`, `echonet/k8s/echonet/secret.yaml`.
- **Don't restart pods, scale deployments, redeploy, or trigger resyncs** without explicit approval. The whole point of investigating a resync is preserving the evidence of what caused it.
- Read-only `kubectl logs`, `kubectl exec -- cat|ls|stat`, `kubectl cp` out of the pod, port-forwards, `gcloud logging read`, and curling mainnet's public feeder are all fine without asking.

## 11. Reporting back

Give the user:
- **Trigger tx hash and reason.**
- **Which field/commitment differed**, with both values (if applicable).
- **Whether it matches a known pattern** (cite the section above) or appears novel.
- **Where the bug lives** (`file:line`) and a concrete fix suggestion if you have one, including whether the namespace in question is running an image that already contains a known fix.
