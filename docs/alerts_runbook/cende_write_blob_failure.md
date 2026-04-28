# Cende write blob failure

> **Status:** The sequencer's consensus orchestrator
> (`apollo_consensus_orchestrator` crate) failed to write the
> previous-height block proposal blob to the **CENDE Recorder**
> (CEN-side service, persisted in Aerospike). Stuck cende writes on
> one or two sequencers do **not** halt consensus; a halt is when
> `consensus_block_number` stops growing across the cluster.

## First question: is consensus stuck?

**Open the `consensus_block_number` panel** for the affected
namespace.

- **Rising** → [Scenario B](#scenario-b--consensus-is-rising) (no
  halt). The alert is informational; most cases self-resolve.
- **Flat across the cluster** →
  [Scenario A](#scenario-a--consensus-is-stuck) (halt). Block
  production has stopped; **escalate immediately** in the env
  channel and follow Scenario A.

---

## Scenario A — Consensus is stuck

Block production has stopped. Other halt-detection alerts (such as
`consensus_block_number_stuck` and
`consensus_block_number_progress_is_slow`) are firing too — you are
not the only signal. The cende failure may be the cause, a
co-symptom, or coincidental. The two checks below identify
real-issue causes; if neither matches, the cende failures are likely
a co-symptom and you continue with Scenario B's case identification
under halt urgency.

### A1. Is Aerospike unresponsive?

The recorder writes blobs to Aerospike. If Aerospike is gone, the
recorder cannot function for any sequencer — that's a halt cause.

Run the [Aerospike-health query](#aerospike-health). `socket=30000`
ms = 30s. **Either string present** → Aerospike outage. This is the
cause; needs Aerospike-side investigation, not a sequencer-side
fix.

### A2. Is there a state-corruption indicator?

Mismatched hashes or heights mean the chain can't move forward
until something is manually reconciled — these are not
wait-and-watch cases.

Run the [sequencer-side `CENDE_FAILURE` query](#sequencer-side-cende_failure-lines)
and look for any of these signals:

- `Highly unlikely: Cende behind prev for height ...` line →
  indicates a code bug. Debug what happened in the recorder.
- Dominant `height_mismatch` label on `cende_write_blob_failure` →
  sequencer-internal race after a state-sync handoff. Sequencer-side
  bug.
- `consensus_retrospective_block_hash_mismatch` alert co-firing →
  state-sync vs batcher hash divergence. Different runbook.

### A3. Otherwise

If neither A1 nor A2 matches, the cende failures are likely a
co-symptom of the halt, not its cause. Continue with
[Scenario B's case identification](#if-the-alert-persists-past-5-minutes)
— but with halt urgency.

---

## Scenario B — Consensus is rising

Block production is fine; the alert is informational. The question
is: **will it resolve on its own, and if not, which case am I in?**

### Default: wait ~5 minutes

Most cende failures self-resolve. Open the
`cende_write_blob_success` panel — if it resumes within ~5 minutes,
the failure was transient and you can close. Self-resolving cases:

- `communication_error` — network blip or recorder pod restart.
- `no_latest_block_from_recorder` — recorder restart in progress.
- `recorder_ahead_of_proposal_height` or 400 lagging-threshold,
  **when `state_sync_lag` is decreasing on the affected
  sequencer** — catching up via state-sync.

### If the alert persists past ~5 minutes

You are in one of these non-self-resolving cases. Each has a
specific identification signal. Run the GCP queries in
[Logs](#logs) as needed to confirm.

- **Sequencer is far behind and sync is failing.**
  Lagging-threshold or `recorder_ahead_of_proposal_height`, **and**
  `state_sync_lag` is flat or growing. Sync investigation needed.

- **Recorder is rejecting valid writes (recorder-side bug).**
  `cende_recorder_error` with a non-2xx other than the 400
  lagging-threshold body — read the response body via the
  [sequencer-side `CENDE_FAILURE` query](#sequencer-side-cende_failure-lines).
  Hand to recorder codeowners.

- **Request never reaches the recorder.** `communication_error`
  persists, and the [recorder-side query](#recorder-side) shows
  nothing for the failing block number. Either network or the
  recorder is rejecting connections.

- **Recorder hung mid-write.** [Recorder-side query](#recorder-side)
  shows `HTTP request: URL=/cende_recorder/write_blob` and
  `Handling a blob with block number` but no
  `Status=200. Body=write blob success!`. Aerospike-side trouble
  brewing — also run the [Aerospike-health query](#aerospike-health)
  to confirm.

- **`height_mismatch` race.** Dominant label is `height_mismatch`.
  Documented race in the consensus orchestrator's `decision_reached`
  ambassador update; hand to sequencer codeowners.

### Co-firing alert modifiers

- **An upgrade is in progress** (check the env channel) — alerts
  during the upgrade window are expected; revisit after rollout.
- **`gps inconsistent batch` is firing** — Jenkins job automatically
  shuts down the cende-recorder to reduce the potential reorg length.
  **Do not manually scale the cende-recorder back up** until the GPS
  issue has been debugged — doing so before the underlying issue is
  resolved can extend the reorg.
- **`cende_write_prev_height_blob_latency_too_high` is firing** —
  write latency is elevated. Recorder is overloaded or Aerospike is
  slow.

---

## Where to post results

The production env channel for the affected environment.

Include:

- Whether consensus is still growing (Scenario A or B).
- Affected sequencers (which pods).
- The case from Scenario B's list, or the cause from Scenario A.
- The decisive log line that identified the case.

---

## Logs

GCP queries used by the scenarios above.

### Sequencer-side `CENDE_FAILURE` lines

```
resource.labels.namespace_name="<sequencer-namespace-from-alert>"
resource.labels.container_name="sequencer-core"
"CENDE_FAILURE"
```

`CENDE_FAILURE` lines map to `cende_write_failure_reason` labels:

| Sequencer log fragment | Reason label |
|---|---|
| `Cende does not have previous block for height` | `no_latest_block_from_recorder` |
| `Cende ahead of proposal height` | `recorder_ahead_of_proposal_height` |
| `Mismatch blob block number and height` | `height_mismatch` |
| `The recorder failed to write blob with block number ... Status code:` | `cende_recorder_error` |
| `Failed to send a request to the recorder` | `communication_error` |

The `cende_recorder_error` 400 body usually reads `Block number X
was already written and is lower than the acceptable lagging
threshold by N blocks`.

### Recorder side

```
resource.labels.namespace_name="<cen-namespace-for-env>"
resource.labels.container_name="cende-recorder"
"HTTP request: URL=/cende_recorder/write_blob" OR "Handling a blob with block number" OR "HTTP response: Status=200. Body=write blob success!"
```

A complete successful write logs all three. Followed in
`batch-committer` logs by `<N> transactions was written to
Aerospike`.

### Aerospike health

```
resource.labels.container_name="batch-committer"
"High latency in writing to storage" OR "Client timeout: socket=30000"
```

### Co-firing consensus impact

```
resource.labels.namespace_name="<sequencer-namespace-from-alert>"
resource.labels.container_name="sequencer-core"
"Applying TimeoutPrecommit for round" OR "Proposal failed as proposer"
```

Standard signature when cende failures localize to the proposer
sequencer — the round times out, the next sequencer retries the
height. Confirms cende failures are upstream of `consensus_round_above_zero`
alerts firing in parallel.

---

## Grafana

Two stacks: the **CEN dashboard** (centralized Starknet, recorder
and Aerospike side) and the **DEC dashboard** (per-sequencer
metrics).

**Panels on the DEC dashboard:**

1. `cende_write_blob_failure` — observe the dominant
   `cende_write_failure_reason` label per pod.
2. `cende_write_blob_success` — should be ticking; if at `0` across
   the cluster, the recorder is fully blocked.
3. `cende_write_prev_height_blob_latency` — companion alert
   `cende_write_prev_height_blob_latency_too_high` fires on average
   latency `>3s` over a 20-minute window.
4. `cende_last_prepared_blob_block_number` — should equal
   `consensus_block_number - 1` per pod; gap means the sequencer is
   not keeping up locally.
5. `cende_prepare_blob_for_next_height_latency` — pre-write prep.
6. `consensus_block_number` + `consensus_round_above_zero` — bursts
   in the latter typically follow cende failures on the proposer
   sequencer (TimeoutPrecommit handoffs).

**On the CEN dashboard** — the recorder/Aerospike side. The "Batch
backlog" panel and the `batch-committer`
BatchCreated-vs-BatchCommitted diff graph are the main entry points.

---

## Background

In the system architecture, the **CENDE Recorder** sits on the CEN
side (centralized Starknet) alongside the Feeder Gateway. The
sequencer (DEC side) contains the Consensus Manager. The Consensus
Manager's orchestrator component (`apollo_consensus_orchestrator`
crate) writes each block proposal as a blob to the recorder.

The cende write flow has three steps; failures at each step emit a
specific `cende_write_failure_reason`:

1. **Prepare** (`prepare_blob_for_next_height`) — builds the blob
   locally. Sets `cende_last_prepared_blob_block_number`. May emit
   `height_mismatch` later when the prepared blob is consumed.
2. **Probe** (`previous_height_exists_at_cende_recorder`) — queries
   the recorder's `get_latest_received_block` to confirm it can
   proceed. May emit `no_latest_block_from_recorder` or
   `recorder_ahead_of_proposal_height`.
3. **Write** (`send_write_blob`) — POSTs the blob; recorder writes
   to Aerospike. May emit `cende_recorder_error` (recorder
   responded non-2xx) or `communication_error` (HTTP transport
   failed).

**Why one or two stuck sequencers do not halt consensus:** consensus
continues as long as some sequencer can take the proposer slot. A
proposer sequencer that can't write its blob causes a
TimeoutPrecommit round handoff to the next proposer. Consensus only
halts (`consensus_block_number` flatlines) when proposers
consistently fail to write across the cluster.

**Why the threshold is `>10/h`:** transient communication errors and
brief recorder restarts are common and self-recover. The threshold
is high enough to ignore that noise but low enough to catch
sustained failures within an hour. The `_once` sibling alert
(Informational) fires on any single failure for visibility.

---

## Metadata

- Alert name: `cende_write_blob_failure`
- Severity: per-environment placeholder.
- Defined: `crates/apollo_dashboard/src/alert_scenarios/block_production_delay.rs`
- Metric: `cende_write_blob_failure` in
  `crates/apollo_consensus_orchestrator/src/metrics.rs`
- Sibling alert: `cende_write_blob_failure_once` (Informational;
  fires on any single failure in the last hour).
- Co-fires with: `consensus_round_above_zero`,
  `consensus_round_above_zero_multiple_times`,
  `consensus_block_number_stuck`, `state_sync_stuck`,
  `cende_write_prev_height_blob_latency_too_high`.
