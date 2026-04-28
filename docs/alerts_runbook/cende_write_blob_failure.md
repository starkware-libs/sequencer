# Cende write blob failure

> **Status:** The sequencer's consensus orchestrator
> (`apollo_consensus_orchestrator` crate) failed to write the
> previous-height block proposal blob to the **CENDE Recorder** (CEN-side
> service, persisted in Aerospike). One or two sequencers stuck on cende
> writes does **not** halt consensus — block production continues as long
> as some sequencer can take the proposer slot. A halt is only when
> `consensus_block_number` stops growing across the cluster.

## Action checklist

Top-to-bottom. Each step explains what you're checking and what the
result means. Don't skip — the value is in the cross-reference.

### 1. Is consensus stuck?

**Why first:** a halt — no new blocks being produced — is the most
serious state. Other alerts (such as `consensus_block_number_stuck`
and `consensus_block_number_progress_is_slow`) also catch a halt, so
this check is not your primary halt-detection mechanism; it is here
because halt vs no-halt changes how urgent the rest of the
investigation is.

**Check:** the `consensus_block_number` panel for the affected
namespace.

- **Rising** → no halt. Cende failures are localized to individual
  sequencers; consensus continues with other proposers. This is the
  more common case. Continue.
- **Flat across the cluster** → halt; **escalate immediately** in
  the env channel and continue this checklist — the cende failures
  may be the cause, a co-symptom, or unrelated, and we need to
  know which.

### 2. Which step of the cende write flow is failing?

**Why:** the `cende_write_blob_failure` counter carries a
`cende_write_failure_reason` label. Each label corresponds to a
specific step in the consensus orchestrator's cende write flow, so
the dominant label tells you exactly *what* is failing without you
having to read logs first.

**Check:** the `cende_write_blob_failure` panel, broken down by
`cende_write_failure_reason` and pod. If many sequencers carry the
same label, the recorder side is the natural suspect (give step 4
extra weight). If only one or two sequencers carry it, the failure
is more likely sequencer-local.

What each label means and what to do:

- **`communication_error`** — the HTTP request from the sequencer to
  the recorder errored at the transport layer (network blip,
  recorder pod restart, brief outage). Watch
  `cende_write_blob_success` for ~5 minutes. If it resumes, the
  failure was transient and you can close. If it persists, the
  recorder side is the suspect — continue to step 4.

- **`cende_recorder_error`** — the recorder accepted the connection
  but rejected the write (non-2xx). The response body tells you why,
  so you must read it (step 3).
  - If the 400 body says *"lower than the acceptable lagging
    threshold by N blocks"*: the affected sequencer is far behind
    the chain. The recorder only accepts writes within a sliding
    window, and this sequencer fell off it. Check `state_sync_lag`
    on the affected sequencer; the alert closes when state-sync
    catches up.
  - Other 4xx, or any 5xx: a recorder bug or a contract drift.
    Continue to step 4 to confirm against recorder logs.

- **`no_latest_block_from_recorder`** — the sequencer's pre-write
  probe asked the recorder for its current height and got no usable
  answer (unreachable, malformed response, or recorder has no blocks
  yet). Wait ~2 minutes for any recorder restart to complete. If it
  persists, continue to step 4.

- **`recorder_ahead_of_proposal_height`** — the same pre-write probe
  reported a recorder height greater than or equal to the height
  the sequencer wants to write. Same root cause as the 400 lagging
  case above (this sequencer is behind), caught earlier in the
  flow. Check `state_sync_lag`; alert closes when state-sync
  catches up.

- **`height_mismatch`** — the sequencer's locally prepared blob has
  a block number that doesn't line up with the current consensus
  height. The code comment in `cende/mod.rs` documents this as a
  known race: when consensus reaches a height via state-sync, the
  cende ambassador's prepared blob is not invalidated, and the next
  height tries to write a misaligned blob. This is a sequencer-side
  bug; hand to sequencer codeowners.

### 3. Read the actual sequencer log lines

**Why:** the metric label is broad. The log line gives you the
concrete block number, the exact recorder response body, and the
timing — needed to tell a transient blip from a real issue, and to
write a useful post in the env channel.

```
resource.labels.namespace_name="<sequencer-namespace-from-alert>"
resource.labels.container_name="sequencer-core"
"CENDE_FAILURE"
```

Each log line corresponds to one of the labels in step 2 — see the
mapping table in [Reason labels](#reason-labels).

**Watch for one specific log line that does not fire the alert:**
`Highly unlikely: Cende behind prev for height ... (cende
latest=...)`. This is logged when the recorder is more than one
block behind the chain. The code marks the case as "highly unlikely"
and does **not** emit a metric, so it cannot be the cause of the
current alert — but when you see it during investigation, it
indicates a worse state than the labelled failures. Historically
(observed at least once) this came from a reorg leaving a block ID
without a hash in Aerospike, blocking subsequent writes. The fix in
that incident was to write the missing hash to Aerospike directly;
coordinate with recorder codeowners if you see it.

### 4. Read the recorder side

**Why:** the sequencer log tells you what the sequencer saw. The
recorder log tells you whether the recorder ever saw the request and
what it did with it. The two together pin down where the failure
lives — sequencer side, between sequencer and recorder, the recorder
service itself, or Aerospike.

```
resource.labels.namespace_name="<cen-namespace-for-env>"
resource.labels.container_name="cende-recorder"
"HTTP request: URL=/cende_recorder/write_blob" OR "Handling a blob with block number" OR "HTTP response: Status=200. Body=write blob success!"
```

A complete successful write logs all three. A successful write also
shows up in `batch-committer` logs as `<N> transactions was written
to Aerospike`. Interpretations:

- **No log lines for the failing block number** → the request never
  reached the recorder. Either the network dropped it (transient,
  retries should clear it) or the recorder is rejecting connections
  (recorder service unhealthy).
- **Request and "Handling a blob…" present, but no 200 success** →
  the recorder accepted the request but hung mid-write. Aerospike
  is the prime suspect; continue to step 5.
- **All three logged with a 200 success** → the write actually
  succeeded; the alert may be stale, or the metric is reporting on
  a different code path. Investigate further before assuming
  mystery.

### 5. Aerospike health (only if step 4 implicates it)

**Why:** the recorder writes blobs to Aerospike. If Aerospike is
unresponsive or slow, the recorder will hang mid-write. The
`batch-committer` logs the most direct symptom — a 30-second socket
timeout.

```
resource.labels.container_name="batch-committer"
"High latency in writing to storage" OR "Client timeout: socket=30000"
```

- **Either string present** → Aerospike isn't responding within 30s.
  **Escalate immediately** — this needs Aerospike-side
  investigation, not a sequencer-side fix.

### 6. Co-firing alert checks

Apply these alongside the above; they refine your conclusion:

- **`gps inconsistent batch` is also firing** — in one observed
  incident, scaling the cende-recorder down and back up via a
  pre-existing Jenkins job was the recovery path. The causal
  relationship between GPS-inconsistent-batch and cende-write
  failures is not documented in this guide, so do not treat the
  rescale as a default action. Consider it only if step 4 shows the
  recorder is hung **and** the GPS alert is firing in parallel.
- **`cende_write_prev_height_blob_latency_too_high` is also firing**
  → write latency is elevated. The recorder is overloaded or
  Aerospike is slow. Steps 4 and 5 narrow which.
- **An upgrade is in progress** (check the env channel) → alerts
  during the upgrade window are expected. No action unless the
  failures persist after the upgrade completes.

### 7. Post results

In the production env channel for the affected environment, post:

- Whether consensus is still growing.
- Affected sequencers (which pods).
- Dominant `cende_write_failure_reason`.
- The decisive log line from step 3 or step 4.
- The outcome category from [Possible outcomes](#possible-outcomes),
  if you confidently identified one.

## Grafana

Two relevant Grafana stacks: the **CEN dashboard** (centralized
Starknet, including the recorder/Aerospike side) and the **DEC
dashboard** (per-sequencer metrics).

**Panels to look at on the DEC dashboard, in order:**

1. **`cende_write_blob_failure`** — observe the dominant
   `cende_write_failure_reason` label. The alert fires on `>10` increase
   in 1h. The sibling `cende_write_blob_failure_once` fires on any
   single increment.
2. **`cende_write_blob_success`** — should still be ticking. If it is at
   `0` across the cluster, the recorder is fully blocked.
3. **`cende_write_prev_height_blob_latency`** — the companion alert
   `cende_write_prev_height_blob_latency_too_high` fires on average
   latency `>3s` over a 20-minute window.
4. **`cende_last_prepared_blob_block_number`** — the consensus
   orchestrator's view of the next blob. For each pod it should equal
   `consensus_block_number - 1`. A gap means the sequencer is not
   keeping up locally.
5. **`cende_prepare_blob_for_next_height_latency`** — pre-write prep.
6. **`consensus_block_number`** + **`consensus_round_above_zero`** —
   bursts in `consensus_round_above_zero` typically follow cende
   failures on the proposer sequencer (TimeoutPrecommit handoffs).

**On the CEN dashboard** — the recorder/Aerospike side. The "Batch
backlog" panel and the `batch-committer` BatchCreated-vs-BatchCommitted
diff graph are the main entry points for the recorder side.

## Logs

The `cende-recorder` runs in a CEN-side namespace. Sequencer nodes run
in DEC-side namespaces; the consensus orchestrator runs inside the
`sequencer-core` container.

### 1. Sequencer-side (consensus orchestrator) — confirm the failure and read the reason

```
resource.labels.namespace_name="<sequencer-namespace-from-alert>"
resource.labels.container_name="sequencer-core"
"CENDE_FAILURE"
```

Match the result against:

| Consensus orchestrator log fragment | Reason label |
|---|---|
| `Cende does not have previous block for height` | `no_latest_block_from_recorder` |
| `Cende ahead of proposal height` | `recorder_ahead_of_proposal_height` |
| `Mismatch blob block number and height` | `height_mismatch` |
| `The recorder failed to write blob with block number ... Status code:` | `cende_recorder_error` |
| `Failed to send a request to the recorder` | `communication_error` |

The `cende_recorder_error` 400 body usually reads `Block number X was
already written and is lower than the acceptable lagging threshold by N
blocks` — this is the recorder rejecting because the affected sequencer
is far behind.

### 2. Cende-recorder — confirm whether the write was received and processed

```
resource.labels.namespace_name="<cen-namespace-for-env>"
resource.labels.container_name="cende-recorder"
"HTTP request: URL=/cende_recorder/write_blob" OR "Handling a blob with block number" OR "HTTP response: Status=200. Body=write blob success!"
```

A complete successful write logs all three. Followed in the
`batch-committer` logs by `<N> transactions was written to Aerospike`.
If the recorder received the request but never logged the 200 success,
the recorder hung mid-write.

### 3. Aerospike health — check whether the recorder can write to storage

```
resource.labels.container_name="batch-committer"
"High latency in writing to storage" OR "Client timeout: socket=30000"
```

`socket=30000` ms = 30s. If you see this, Aerospike isn't responding to
the recorder; **escalate immediately** and post results.

### 4. Co-firing consensus impact — confirm round handoffs

```
resource.labels.namespace_name="<sequencer-namespace-from-alert>"
resource.labels.container_name="sequencer-core"
"Applying TimeoutPrecommit for round" OR "Proposal failed as proposer"
```

This is the standard signature when cende failures localize to the
proposer sequencer — the round times out, the next sequencer retries
the height. If you see this clustered with cende failures, the cende
issue is the upstream cause.

## Reason labels

The `cende_write_blob_failure` counter is labeled
`cende_write_failure_reason`. Five values, each emitted from a specific
step in the consensus orchestrator's cende write flow
(`crates/apollo_consensus_orchestrator/src/cende/mod.rs`). The
**Outcomes** column points to the most likely categories in
[Possible outcomes](#possible-outcomes).

| Label | Where it fires | What it means | Outcomes |
|---|---|---|---|
| `communication_error` | write step | HTTP request to recorder errored (network/transport). | 1, 5 |
| `cende_recorder_error` | write step | Recorder returned non-2xx. The 400 body usually means the affected sequencer is far behind chain. | 2 (400 lagging-threshold body), 5 (rescale), 9 (recorder bug) |
| `no_latest_block_from_recorder` | pre-write probe | Recorder didn't return a usable latest block (request error, parse failure, or no blocks recorded yet). | 1, 9 |
| `recorder_ahead_of_proposal_height` | pre-write probe | Recorder reports a height ≥ what the consensus orchestrator wants to write. The write is aborted. | 2 |
| `height_mismatch` | sequencer-internal | The blob prepared by the consensus orchestrator has a block number that doesn't line up with the current consensus height. Typically when consensus advanced via state-sync. | 7 |

## Possible outcomes

When investigating, classify what you found into one of these outcome
categories. Use the category number and name when posting in the env
channel.

### Not serious — observe and close

**1. Self-resolving transient.** *Trigger:* network blip, recorder pod
restart, brief Aerospike load spike. *Looks like:* mostly
`communication_error` or short bursts of `cende_recorder_error`;
`cende_write_blob_success` resumes within minutes. *Action:* monitor
~5 minutes; post a one-line note; close.

**2. Single sequencer lagging the chain.** *Trigger:* one sequencer is
behind; recorder rejects writes for old heights. *Looks like:*
`recorder_ahead_of_proposal_height`, or `cende_recorder_error` with the
400 "lagging threshold by N blocks" body; `state_sync_lag` is non-zero
on the affected pod. *Action:* monitor state-sync; alert closes when
the sequencer rejoins.

**3. Expected during a planned operation.** *Trigger:* active upgrade,
rolling restart, or deliberate recorder cycle. *Action:* confirm in env
channel; close.

### Mildly serious — operator action, no sequencer code change

**4. Threshold or severity tuning.** *Trigger:* same alert fires
repeatedly without correlating to real impact. *Action:* PR adjusting
the threshold or severity in
`crates/apollo_dashboard/src/alert_scenarios/block_production_delay.rs`,
or a per-env silence.

**5. Operational recovery (recorder rescale).** *Trigger:*
`cende_recorder_error` in cluster, often paired with a
`gps inconsistent batch` alert. *Action:* trigger the recorder-rescale
Jenkins job. No code change.

**6. Manual one-shot data fix in Aerospike.** *Trigger:* a reorg leaves
a block ID without a hash, blocking subsequent writes. *Looks like:*
often co-fires with `cende_write_prev_height_blob_latency_too_high`;
the "`Highly unlikely: Cende behind prev for height`" log line in the
sequencer logs is a strong tell. *Action:* write the missing hash to
Aerospike directly. Optionally: change BHC config for that environment
to skip verification of the bad-block range until the chain settles.

### Serious — code change required

**7. Race in `decision_reached` ambassador update.** *Trigger:*
consensus reaches a height via state-sync; the ambassador's prepared
blob is not invalidated; the next height tries to write a misaligned
blob. *Looks like:* `height_mismatch`, often after a sync event.
*Code site:* `write_prev_height_blob` in `cende/mod.rs`, the path with
the inline comment about `decision_reached`. *Action:* PR in
`apollo_consensus_orchestrator`. This is the bug class the in-flight
workstream "*protection against working on a block that's written to
cende*" addresses.

**8. Defensive panic hit (`blob.block_number >= current_height`).**
*Trigger:* an upstream invariant violation in blob preparation timing
caused the panic. *Looks like:* `pod_state_crashloopbackoff` on the
affected sequencer, **not** this alert directly. If
`cende_write_blob_failure` fires on a sequencer that is also
crashlooping, this is a candidate. *Action:* PR in
`apollo_consensus_orchestrator`. The panic stays (it correctly catches
a real invariant violation); the bug is upstream of it.

**9. Recorder-side bug.** *Trigger:* recorder returns 4xx/5xx on
requests it should accept, or returns malformed
`get_latest_received_block` responses. *Looks like:*
`cende_recorder_error` or `no_latest_block_from_recorder` at high rate
**without** sequencer-side lag explaining it. *Action:* hand to the
recorder-side codeowners; PR in the recorder repo.

### Architectural — recurring class, design change

**10. Architectural change to the cende write flow.** *Trigger:* a class
of failure recurs even after individual bug fixes; it points to a
design issue. *Examples merged or in-flight:* "skip cende write based
on cende height" (merged); "larger epoch" (PR #13368); "protection
against working on a block that's written to cende" (in development).
*Action:* design + PR; not closeable from one alert.

### Critical — cluster halt

**Halt scenario.** `consensus_block_number` flatlines across the
cluster. **Escalate immediately**, post results. Halt resolution is a
separate runbook (reorg, restart).

## Where to post results

The production env channel for the affected environment.

Include: outcome category, dominant `cende_write_failure_reason`,
which sequencers are affected, whether consensus is still growing, and
any relevant log snippets from the queries above.

## Background

In the system architecture, the **CENDE Recorder** sits on the CEN side
(centralized Starknet) alongside the Feeder Gateway. The sequencer (DEC
side) contains the Consensus Manager. The Consensus Manager's
orchestrator component (`apollo_consensus_orchestrator` crate) writes
each block proposal as a blob to the recorder.

The cende write flow has three steps; failures at each step emit a
specific `cende_write_failure_reason`:

1. **Prepare** (`prepare_blob_for_next_height`) — builds the blob
   locally. Sets `cende_last_prepared_blob_block_number`. May emit
   `height_mismatch` later when the prepared blob is consumed.
2. **Probe** (`previous_height_exists_at_cende_recorder`) — queries the
   recorder's `get_latest_received_block` to confirm it can proceed. May
   emit `no_latest_block_from_recorder` or
   `recorder_ahead_of_proposal_height`.
3. **Write** (`send_write_blob`) — POSTs the blob; recorder writes to
   Aerospike. May emit `cende_recorder_error` (recorder responded
   non-2xx) or `communication_error` (HTTP transport failed).

**Why one or two stuck sequencers do not halt consensus:** consensus
continues as long as some sequencer can take the proposer slot. A
proposer sequencer that can't write its blob causes a TimeoutPrecommit
round handoff to the next proposer. Consensus only halts (i.e.,
`consensus_block_number` flatlines) when proposers consistently fail to
write across the cluster.

**Why the threshold is `>10/h`:** transient communication errors and
brief recorder restarts are common and self-recover. The threshold is
high enough to ignore that noise but low enough to catch sustained
failures within an hour. The `_once` sibling alert (Informational) fires
on any single failure for visibility.

## Metadata

- Alert name: `cende_write_blob_failure`
- Severity: per-environment placeholder.
- Defined: `crates/apollo_dashboard/src/alert_scenarios/block_production_delay.rs`
- Metric: `cende_write_blob_failure` in `crates/apollo_consensus_orchestrator/src/metrics.rs`
- Sibling alert: `cende_write_blob_failure_once` (Informational; fires on
  any single failure in the last hour).
- Co-fires with: `consensus_round_above_zero`,
  `consensus_round_above_zero_multiple_times`,
  `consensus_block_number_stuck`, `state_sync_stuck`,
  `cende_write_prev_height_blob_latency_too_high`.
