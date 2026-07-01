# <Alert title in plain English>

<!--
Title: use the human-readable meaning, not the metric name.
  Good: "Cende write blob failure"
  Bad:  "cende_write_blob_failure alert"
-->

> **Status:** <One-sentence summary: which component failed, what it was
> trying to do, and the blast radius. State upfront what this alert does
> NOT mean if that's a common misconception.>
>
> Example: "The sequencer's consensus orchestrator failed to write the
> previous-height blob to the CENDE Recorder. Stuck writes on one or two
> sequencers do **not** halt consensus; a halt is when
> `consensus_block_number` stops growing across the cluster."

<!--
The Status block is the first thing a panicking on-call reads.
Scope the blast radius here: does this mean the system is down?
Or is it likely informational? If partial failure is tolerated
by the system, say so explicitly.
-->

## First question: <yes/no or A/B triage question>

<!--
This is the on-call's first action. Pick the single most important
observable that splits the investigation into two different paths.

Requirements:
- Tell the on-call exactly WHERE to look (which Grafana panel,
  which metric, which command to run) — don't make them guess.
- Each answer should name the scenario, link to it, and give a
  one-line urgency cue ("escalate immediately" vs "informational").
-->

**Open the `<panel_name>` panel** for the affected namespace.

- **<Answer 1>** → [Scenario A](#scenario-a--short-label). <Urgency
  cue and one-line meaning.>
- **<Answer 2>** → [Scenario B](#scenario-b--short-label). <Urgency
  cue and one-line meaning.>

---

## Scenario A — <short label for the critical path>

<!--
Convention: Scenario A is the critical path. Open with a short
paragraph that sets context:
- What other alerts are likely co-firing?
- Is this alert the likely CAUSE, a CO-SYMPTOM, or coincidental?
- What is the on-call's goal in this scenario?

Then list numbered sub-checks (A1, A2, …). Each sub-check should:
1. Name the system or component to inspect.
2. Provide a ready-to-paste GCP log query or a specific Grafana panel.
3. State what a POSITIVE match looks like ("Either string present →").
4. State the conclusion and next action ("This is the cause; needs
   X-side investigation, not a Y-side fix." or "Hand to Z codeowners.").

End with an "Otherwise" sub-check that either links to the other
scenario's case list or to escalation.
-->

### A1. <What to check — phrase as a question>

<Why this matters — one sentence.>

GCP logs:

```
<ready-to-paste query>
```

**<What a positive match looks like>** → <conclusion and action>.

### A2. <Next check>

<Why this matters.>

GCP logs (<which side>):

```
<ready-to-paste query>
```

Look for any of these signals:

- `<log fragment or metric label>` → <what it means and what to do>.
- `<another signal>` → <meaning and action>.

### A3. Otherwise

If none of the above match, <state what the on-call should conclude
and where to continue — typically the other scenario's case list but
with the urgency of this scenario>.

---

## Scenario B — <short label for the non-critical path>

<!--
Convention: Scenario B is the non-critical path.
Open with a framing sentence: is the system functional? What's the
real question now? (e.g., "will it resolve on its own, and if not,
which case am I in?")
-->

### Monitor for ~N minutes

<!--
If transient self-resolution is common for this alert, say so.
Tell the on-call exactly which panel to watch and what "resolving"
looks like. List the self-resolving patterns. For each pattern:
- The metric label or log signal that identifies it.
- The CONDITION that makes it self-resolving (e.g., "when
  `state_sync_lag` is decreasing").
-->

Most <alert> failures self-resolve. Open the `<success_panel>` —
if it resumes within ~N minutes, the failure was transient.
Self-resolving patterns:

- `<metric_label>` — <what happened, why it self-resolves>.
- `<metric_label>` — <what happened, why it self-resolves>.

### If the alert persists past ~N minutes

<!--
This is the core diagnostic catalog. List every known
non-self-resolving case as a bullet. For each case:
- **Bold title** that names the problem in plain English.
- Identification signal: which metric label, log line, or dashboard
  state confirms this case. Include conditions ("X, AND Y is flat").
- Disposition: what to do or who to hand it to.

Order cases from most common to least common.
-->

- **<Case name in plain English.>** <Identification signal —
  metric label + condition.> <Disposition.>

- **<Next case.>** <Signal.> <Disposition.>

- **Stale alert.** <How to identify that the alert is a false
  positive — e.g., success logs are present for the failing item.>
  Alert quality issue, not a production issue.

### Co-firing alert modifiers

<!--
Other alerts that, when firing IN PARALLEL with this one, change
the interpretation or suggest a different action. For each:
- Name the co-firing alert or condition.
- State how it changes the diagnosis or action.
-->

- **<Co-firing condition>** — <how it modifies the investigation>.
- **<Another condition>** — <effect>.

---

## Where to post results

<!--
On-call needs to know WHERE to communicate and WHAT to include.
Be specific about the channel and the checklist of information.
-->

The production env channel for the affected environment.

Include:

- Which scenario (A or B).
- Affected pods / sequencers.
- The specific case or cause identified.
- The decisive log line or metric observation that confirmed it.

---

## Logs

<!--
Collect ALL log queries referenced by the scenarios above into this
single section. The on-call should be able to scroll here and
copy-paste any query without hunting through scenario text.

Organize by source system (e.g., sequencer side, recorder side,
infrastructure side). For each:
- A ready-to-paste GCP query (use <placeholders> for namespace etc.).
- If log fragments map to metric labels, include a mapping table.
- Note what a "complete" or "healthy" log trail looks like.
-->

### <Source system 1> — <what these logs show>

```
resource.labels.namespace_name="<namespace-from-alert>"
resource.labels.container_name="<container>"
"<search string>" OR "<other search string>"
```

<!--
If log lines map to metric labels, include a table:

| Log fragment | Metric label |
|---|---|
| `<fragment>` | `<label>` |
-->

### <Source system 2> — <what these logs show>

```
resource.labels.namespace_name="<namespace>"
resource.labels.container_name="<container>"
"<search string>"
```

<Note what a complete/healthy log trail looks like for this source.>

---

## Grafana

<!--
Organize by dashboard. For each panel:
- Name the panel (use the exact Grafana panel title).
- One-line note on what to look for and what "healthy" vs "broken"
  looks like.
- If a panel has a relationship to another metric, state it
  (e.g., "should equal `consensus_block_number - 1` per pod").
-->

**Panels on the <primary dashboard>:**

1. `<panel_name>` — <what to look for>.
2. `<panel_name>` — <what healthy looks like; what broken looks like>.
3. `<panel_name>` — <relationship to other metrics if any>.

**On the <secondary dashboard>** — <what this dashboard covers>.
<Key panels and entry points.>

---

## Background

<!--
Architectural context for an on-call who hasn't seen this system
before. This section should answer:

1. WHERE does this component sit in the architecture? Name the
   crate, the side (e.g., DEC vs CEN), and its neighbors.
2. WHAT is the step-by-step flow? Name the functions and what each
   step does. Map each step to the failure reasons it can emit.
3. WHY doesn't partial failure cause a full outage? (Or: why DOES
   it?) Explain the system's tolerance model.
4. WHY is the alert threshold set where it is? What noise level
   is it designed to ignore?
-->

In the system architecture, **<component>** sits in
<location/crate>. <Its role in one sentence.>

The <flow name> has N steps; failures at each step emit a specific
`<failure_reason_label>`:

1. **<Step name>** (`<function_name>`) — <what it does>. May emit
   `<reason_1>`.
2. **<Step name>** (`<function_name>`) — <what it does>. May emit
   `<reason_2>` or `<reason_3>`.

**Why <partial failure doesn't cause X>:** <explanation of the
system's fault tolerance for this failure mode.>

**Why the threshold is `<value>`:** <what noise level it filters
and what sustained failure rate it catches.>

---

## Metadata

<!--
Structured fields for automation and cross-referencing.
-->

- Alert name: `<alert_name>`
- Severity: <per-environment placeholder, or fixed severity>.
- Defined: `crates/apollo_dashboard/src/alert_scenarios/<scenario>.rs`
- Metric: `<metric_name>` in `crates/<crate>/src/metrics.rs`
- Sibling alert: `<sibling_alert_name>` (<severity>; <when it fires
  relative to this alert>).
- Co-fires with: `<alert_1>`, `<alert_2>`, `<alert_3>`.

<!--
Keep this file lean. When in doubt, write what would have helped *you*
the first time you saw this alert at 3 AM.
-->
