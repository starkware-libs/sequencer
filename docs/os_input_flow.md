---
marp: true
title: The os_input Flow
author: yoav@starkware.co
paginate: true
theme: uncover
class: invert
style: |
  section {
    font-size: 26px;
    justify-content: flex-start;
    text-align: left;
    padding: 48px 60px;
  }
  section.lead {
    justify-content: center;
    text-align: center;
  }
  h1 { font-size: 46px; color: #8ec1ff; }
  h2 { font-size: 34px; color: #8ec1ff; border-bottom: 2px solid #2d4a6b; padding-bottom: 6px; }
  h3 { font-size: 26px; color: #b7d3ff; }
  code { font-size: 0.78em; background: #11243b; }
  pre { font-size: 0.7em; line-height: 1.25; }
  table { font-size: 0.62em; }
  th { background: #1b3c5c; color: #cfe3ff; }
  strong { color: #ffd479; }
  blockquote { border-left: 4px solid #8ec1ff; color: #cdd9e5; font-size: 0.8em; }
  .small { font-size: 0.7em; }
  .ref { color: #7fa7cc; font-size: 0.65em; }
  .diagram { display: flex; flex-direction: column; align-items: center; margin-top: 18px; }
  .flow-row { display: flex; align-items: flex-start; justify-content: center; gap: 4px; }
  .node { background: #1b3c5c; color: #cfe3ff; border: 1px solid #2d4a6b; border-radius: 10px;
          padding: 10px 16px; font-size: 0.72em; font-weight: 600; white-space: nowrap;
          min-height: 22px; display: flex; align-items: center; justify-content: center; }
  .node-os { background: #1f5c3c; border-color: #2f7d52; color: #d6ffe6; }
  .node-committer { background: #5c4a1b; border-color: #7d6a2f; color: #ffe9b0; }
  .edge { height: 44px; display: flex; flex-direction: column; align-items: center; justify-content: center; }
  .edge .label { font-size: 0.52em; color: #8ec1ff; line-height: 1.2; text-align: center; margin-bottom: 2px; max-width: 120px; }
  .edge .glyph { font-size: 1.0em; color: #8ec1ff; line-height: 1; }
  .branch { display: flex; flex-direction: column; align-items: center; }
  .vedge { display: flex; flex-direction: column; align-items: center; padding: 6px 0; }
  .vedge .down { font-size: 0.52em; color: #8ec1ff; text-align: center; }
  .vedge .up { font-size: 0.52em; color: #8ec1ff; text-align: center; }
  .edge .group { display: block; line-height: 1.25; }
  .edge .group .tag { display: block; font-size: 0.85em; color: #ff9b9b; opacity: 0.9; }
  .edge .cur { color: #8ec1ff; }
  .edge .past { color: #8ec1ff; margin-top: 4px; }
  .stage { background: #1b3c5c; color: #cfe3ff; border: 1px solid #2d4a6b; border-radius: 10px;
           padding: 12px 18px; display: flex; flex-direction: column; align-items: center; gap: 5px; min-width: 150px; }
  .stage-committer { background: #5c4a1b; border-color: #7d6a2f; color: #ffe9b0; }
  .stage .name { font-size: 0.72em; font-weight: 600; }
  .stage .stored { font-size: 0.52em; text-align: center; line-height: 1.35; }
  .stage .stored.in { color: #8ec1ff; }
  .stage .stored.out { color: #ffd479; }
  .premise { display: inline-block; color: #ffd479; font-size: 0.62em; font-weight: 600;
             letter-spacing: 0.02em; border-left: 3px solid #8ec1ff; padding: 2px 0 2px 10px; margin-bottom: 6px; }
  .scope { display: flex; flex-direction: column; gap: 16px; margin-top: 24px; }
  .scope-item { border-left: 3px solid #8ec1ff; padding: 4px 0 4px 14px; font-size: 0.88em; }
  .scope-item.out { color: #7fa7cc; border-left-color: #4a637f; }
  .warn { color: #ff9b9b; font-weight: 600; }
  .accent { color: #8ec1ff; font-weight: 600; }
  .gold { color: #ffd479; }
  .stats { border-collapse: collapse; margin: 14px auto 0; font-size: 0.58em; }
  .stats th, .stats td { padding: 5px 18px; border-bottom: 1px solid #2d4a6b; text-align: right; }
  .stats th { background: transparent; color: #8ec1ff; border-bottom: 2px solid #2d4a6b; }
  .stats td:first-child, .stats th:first-child { text-align: left; color: #cfe3ff; }
  .stats .before { color: #7fa7cc; }
  .stats .after { color: #8ec1ff; }
  .stats .delta { color: #ffd479; font-weight: 600; }
  .stats.uniform td, .stats.uniform th { border: 0; border-bottom: 1px solid #2d4a6b; }
  .stats.uniform td:last-child, .stats.uniform th:last-child { text-align: left; }
  .fromtable { font-size: 26px; border-collapse: separate; border-spacing: 14px 16px; margin: 20px auto 0; }
  .fromtable, .fromtable tr, .fromtable td { border: none !important; background: transparent !important; }
  .fromtable td { text-align: center; vertical-align: middle; padding: 0; }
  .fromtable .arw { font-size: 1em; color: #8ec1ff; }
  .fromtable .node { margin: 0 auto; }
  .fromtable .node-os { min-height: 104px; }
  .exchange { display: flex; flex-direction: column; align-items: center; gap: 16px; margin-top: 24px; }
  .exchange-heads { display: flex; align-items: center; gap: 24px; }
  .exchange-heads .glyph { font-size: 1.1em; color: #8ec1ff; }
  .steps { display: flex; flex-direction: column; gap: 9px; }
  .step { font-size: 0.5em; padding: 8px 16px; border-radius: 8px; background: #1b3c5c;
          border: 1px solid #3a5a7c; white-space: nowrap; }
  .step .n { font-weight: 700; opacity: 0.8; margin-right: 6px; }
  .step.req { color: #8ec1ff; }
  .step.res { color: #ffd479; border-color: #5c4a1b; }
  .seq { position: relative; width: 980px; max-width: 100%; margin: 26px auto 0;
         background-image: linear-gradient(#34516f, #34516f), linear-gradient(#34516f, #34516f),
                           linear-gradient(#34516f, #34516f), linear-gradient(#34516f, #34516f),
                           linear-gradient(#34516f, #34516f);
         background-size: 1px 100%; background-repeat: no-repeat;
         background-position: 10% 0, 30% 0, 50% 0, 70% 0, 90% 0; }
  .seq-head { display: grid; grid-template-columns: repeat(10, 1fr); margin-bottom: 12px; }
  .seq-head .node { justify-self: center; text-align: center; line-height: 1.15; }
  .seq-msg { display: grid; grid-template-columns: repeat(10, 1fr); height: 42px; align-items: center; }
  .seq .arr { position: relative; height: 0; border-top: 2px solid #6f9bc4; }
  .seq .arr .lbl { position: absolute; left: 50%; transform: translateX(-50%); top: -1.7em;
                   font-size: 0.44em; white-space: nowrap; color: #8ec1ff; }
  .seq .arr .lbl.out { color: #ffd479; }
  .seq .arr.right::after { content: ''; position: absolute; right: -1px; top: -6px;
                           border: 6px solid transparent; border-left-color: #6f9bc4; }
  .seq .arr.left::before { content: ''; position: absolute; left: -1px; top: -6px;
                           border: 6px solid transparent; border-right-color: #6f9bc4; }
  .seq .arr.right::before { content: ''; position: absolute; left: -3px; top: -3px;
                            width: 7px; height: 7px; border-radius: 50%; background: #6f9bc4; }
  .seq .arr.left::after { content: ''; position: absolute; right: -3px; top: -3px;
                          width: 7px; height: 7px; border-radius: 50%; background: #6f9bc4; }
  .seq-head .h1 { grid-column: 1 / 3; }
  .seq-head .h2 { grid-column: 3 / 5; }
  .seq-head .h3 { grid-column: 5 / 7; }
  .seq-head .h4 { grid-column: 7 / 9; }
  .seq-head .h5 { grid-column: 9 / 11; }
  .seq-msg .s24 { grid-column: 2 / 4; }
  .seq-msg .s46 { grid-column: 4 / 6; }
  .seq-msg .s68 { grid-column: 6 / 8; }
  .seq-msg .s610 { grid-column: 6 / 10; }
  .seq-msg .s810 { grid-column: 8 / 10; }
  .seq-msg .s210 { grid-column: 2 / 10; }
---

<!-- _class: lead invert -->

# **Witnesses from Rust Committer**

## Design Review

---

## What the OS needs?

The **prev** state, the **new** state, and the **transition** between them.

<br>

A state is:
- **root hash**
- **leaves** (initial reads / state diff)
- **witnesses** - verify the data against the root

<div class="diagram">
<img src="diagrams/state_trees.svg" width="720" alt="prev and new state trees" />
</div>

---

## From where it comes from?

The **leaves data** comes from the blockifier, while the **root** and the **witnesses** come from the committer.

<br>

<table class="fromtable">
<tr>
  <td><div class="node" style="min-width: 120px;">blockifier</div></td>
  <td><span class="arw">→</span></td>
  <td><div class="node" style="min-width: 250px;">state_diff&nbsp;+&nbsp;<span class="gold">initial_reads</span></div></td>
  <td><span class="arw">→</span></td>
  <td rowspan="2"><div class="node node-os" style="min-width: 64px; min-height: 104px;">OS</div></td>
</tr>
<tr>
  <td><div class="node" style="min-width: 120px;">committer</div></td>
  <td><span class="arw">→</span></td>
  <td><div class="node" style="min-width: 250px;">state_root&nbsp;+&nbsp;<span class="gold">witnesses</span></div></td>
  <td><span class="arw">→</span></td>
</tr>
</table>

---

## The current flow

<!-- transition: fade -->

<div class="diagram">
<div style="display: flex; gap: 28px; align-items: stretch; justify-content: center; width: 100%;">
  <div style="flex: 1; display: flex; flex-direction: column; align-items: center; border: 1px solid #2d4a6b; border-radius: 12px; padding: 18px 20px; background: #12283f;">
    <div style="font-weight: 700; color: #ffd479; font-size: 0.9em; margin-bottom: 12px;">Rust side</div>
    <div class="node" style="view-transition-name: vt-produce;">produce block H</div>
    <div class="vedge"><span class="down">↓</span></div>
    <div class="node" style="view-transition-name: vt-ucs-l;"><span class="warn">update committer state</span></div>
    <div class="vedge"><span class="down">↓</span></div>
    <div class="node" style="view-transition-name: vt-calc-l;">calculate block hash</div>
    <div class="vedge"><span class="down">↓</span></div>
    <div class="node" style="view-transition-name: vt-spawn;">spawn next block producing</div>
    <div class="vedge"><span class="down">↓</span></div>
    <div class="node" style="view-transition-name: vt-send;"><span>send to <span class="gold">python side</span></span></div>
  </div>
  <div style="flex: 1; display: flex; flex-direction: column; align-items: center; border: 1px solid #2d4a6b; border-radius: 12px; padding: 18px 20px; background: #12283f;">
    <div style="font-weight: 700; color: #ffd479; font-size: 0.9em; margin-bottom: 12px;">Python side</div>
    <div class="node" style="view-transition-name: vt-ucs;"><span class="warn">update committer state</span></div>
    <div class="vedge"><span class="down">↓</span></div>
    <div class="node" style="view-transition-name: vt-vbh;">validate block hash</div>
    <div class="vedge"><span class="down">↓</span></div>
    <div class="node" style="view-transition-name: vt-allow;">allow producing block H-10</div>
    <div class="vedge"><span class="down">↓</span></div>
    <div class="node" style="view-transition-name: vt-calc;">collect witnesses</div>
    <div class="vedge"><span class="down">↓</span></div>
    <div class="node node-os" style="view-transition-name: vt-runos;">run the OS</div>
  </div>
</div>
</div>

---

## The general plan

<!-- transition: none -->

<div class="diagram">
<div style="display: flex; gap: 28px; align-items: stretch; justify-content: center; width: 100%;">
  <div style="flex: 1; display: flex; flex-direction: column; align-items: center; border: 1px solid #2d4a6b; border-radius: 12px; padding: 18px 20px; background: #12283f;">
    <div style="font-weight: 700; color: #ffd479; font-size: 0.9em; margin-bottom: 12px;">Rust side</div>
    <div class="node" style="view-transition-name: vt-produce;">produce block H</div>
    <div class="vedge"><span class="down">↓</span></div>
    <div class="node" style="view-transition-name: vt-ucs;"><span class="warn">update committer state</span></div>
    <div class="vedge"><span class="down">↓</span></div>
    <div class="node" style="view-transition-name: vt-calc;">calculate block hash and witnesses</div>
    <div class="vedge"><span class="down">↓</span></div>
    <div class="node" style="view-transition-name: vt-allow;">allow producing block H-10</div>
    <div class="vedge"><span class="down">↓</span></div>
    <div class="node" style="view-transition-name: vt-spawn;">spawn next block producing</div>
    <div class="vedge"><span class="down">↓</span></div>
    <div class="node" style="view-transition-name: vt-send;"><span>send to <span class="gold">python side</span></span></div>
  </div>
  <div style="flex: 1; display: flex; flex-direction: column; align-items: center; border: 1px solid #2d4a6b; border-radius: 12px; padding: 18px 20px; background: #12283f;">
    <div style="font-weight: 700; color: #ffd479; font-size: 0.9em; margin-bottom: 12px;">Python side</div>
    <div class="node node-os" style="view-transition-name: vt-runos;">run the OS</div>
  </div>
</div>
</div>

---

## Requirements - Storage

<div class="premise" style="font-size: 1.05em; margin-top: 8px; margin-bottom: 14px;">Committer storage stays proportional to the state size</div>

Store additional data keyed by block number, so the oldest blocks can be pruned.

---

## Requirements - Efficiency

**Reads per second**

<table class="stats uniform">
<tr><td>Lower bound</td><td class="before">5K</td><td>current producing time for big blocks</td></tr>
<tr><td>Benchmark</td><td class="before">9.2K</td><td>read random keys in a scattered contract storage</td></tr>
<tr><td>Upper bound</td><td class="before">13.5K</td><td>max reads within gas of a block for 1.5 secs</td></tr>
</table>

<br>

**Writes per second**

<table class="stats uniform">
<tr><td>Benchmark</td><td class="before">360</td><td>matches the ~1:20 write-to-read ratio on mainnet</td></tr>
</table>

<br>

<span class="ref">Benchmarks measured by the stress-test machines.</span>

---

## Design scope

<!-- transition: none -->

<div class="scope">
  <div class="small" style="margin: 2px 0 -4px 14px;">This design proposes:</div>
  <div class="scope-item"><span class="accent">◼</span>&nbsp; Pass witnesses from an <strong>Apollo node</strong> to Cende, a task currently handled by the Python committer.</div>
  <br>
  <div class="small" style="margin: 2px 0 -4px 14px;">The next design reviews:</div>
  <div class="scope-item out"><span style="color: #4a637f;">◻</span>&nbsp; Calculate witnesses in the <strong>Rust committer</strong>.</div>
  <div class="scope-item out"><span style="color: #4a637f;">◻</span>&nbsp; Handle the witnesses in the <strong>centralized</strong> side.</div>
</div>

---

## The state roots flow

<!-- transition: fade -->

<br>

<div class="diagram">
<div class="flow-row">
  <div class="node">blockifier</div>
  <div class="edge"><span class="label">state_diff</span><span class="glyph">→</span></div>
  <div class="branch">
    <div class="node">batcher</div>
    <div class="vedge">
      <span class="down">↓ state_diff</span>
    </div>
    <div class="node">committer</div>
    <div class="vedge">
      <span class="down">↓ state_root</span>
    </div>
    <div class="node">batcher</div>
  </div>
  <div class="edge"><span class="label"><span class="group cur"><span class="tag">current block:</span>state_diff</span><br><span class="group past"><span class="tag">ready recent 10 blocks:</span>state_root</span></span><span class="glyph">→</span></div>
  <div class="node">consensus</div>
  <div class="edge"><span class="label">blob</span><span class="glyph">→</span></div>
  <div class="node">cende</div>
  <div class="edge"><span class="label">input</span><span class="glyph">→</span></div>
  <div class="node node-os">OS</div>
</div>
</div>

---

## Naive approach: mimic the state roots flow

<!-- transition: none -->

<br>

<div class="diagram">
<div class="flow-row">
  <div class="node">blockifier</div>
  <div class="edge"><span class="label">state_diff<br><span class="gold">initial_reads</span></span><span class="glyph">→</span></div>
  <div class="branch">
    <div class="node">batcher</div>
    <div class="vedge">
      <span class="down">↓ state_diff, <br><span class="gold">accessed_keys</span></span>
    </div>
    <div class="node">committer</div>
    <div class="vedge">
      <span class="down">↓ state_root,<br><span class="gold">witnesses</span></span>
    </div>
    <div class="node">batcher</div>
  </div>
  <div class="edge"><span class="label"><span class="group cur"><span class="tag">current block:</span>state_diff, <span class="gold">initial_reads</span></span><br><span class="group past"><span class="tag">ready recent 10 blocks:</span>state_root, <span class="gold">witnesses</span></span></span><span class="glyph">→</span></div>
  <div class="node">consensus</div>
  <div class="edge"><span class="label">blob</span><span class="glyph">→</span></div>
  <div class="node">cende</div>
  <div class="edge"><span class="label">input</span><span class="glyph">→</span></div>
  <div class="node node-os">OS</div>
</div>
</div>

---

## Producing state roots vs. witnesses

1. Witnesses are **much larger**. pushing witnesses of 10 blocks is <span class="warn">huge</span>.

<div class="diagram" style="flex-direction: row; justify-content: center; align-items: center; gap: 28px;">
<table class="stats">
<tr><th>height</th><th>blob size</th><th>compressed</th></tr>
<tr><td>9.87M</td><th>(7K blocks)</th><th>(2.5K blocks)</th></tr>
<tr><th>median</th><td class="before">0.65 MB</td><td class="after">0.46 MB</td></tr>
<tr><th>mean</th><td class="before">1.87 MB</td><td class="after">1.10 MB</td></tr>
<tr><th>p95</th><td class="before">6.42 MB</td><td class="after">5.10 MB</td></tr>
<tr><th>p99</th><td class="before">24.22 MB</td><td class="after">15.79 MB</td></tr>
<tr><th>max</th><td class="before">109.90 MB</td><td class="after">27.10 MB</td></tr>
</table>
</div>

<br>

2. In a **sync flow** (unlike consensus flow) the blockifier doesn't executed
  → <span class="warn">no witnesses</span>.

---

## 1. Suggestion: Send only the required objects

Ask cende about the witness offset and send only the **missing witnesses**.

<br>

<div class="exchange">
  <div class="exchange-heads">
    <div class="node">consensus</div>
    <div class="glyph">⇄</div>
    <div class="node">cende</div>
  </div>
  <div class="steps">
    <div class="step req"><span class="n">①</span>offset = cende.get witness offset()</div>
    <div class="step req"><span class="n">②</span>witnesses_in_blob = witnesses[offset .. last_known_witnesses]</div>
  </div>
</div>

---

## 2. Suggestion: Cover up what sync is missing

At height **H**, if the witnesses of height **H-10** are missing - the round fails.

<br>

<span class="accent">1. Failing a round is rare</span>, only where:
- the proposer learned height **H-10** from sync, and
- no other node has already sent the witnesses for this height.

<br>

<span class="accent">2. There is always a node that has these witnesses:</span>
- Nodes that took part in consensus of **H-10** ran the blockifier and have the witnesses.
- At upgrades, use the `wait_for_last_commitment` flag to validate that the witnesses in cende are aligned (more downtime).

---

## Naive Persistency: mimic the state roots

- The committer **input** is stored in the batcher storage (and its hash is stored in the committer).
- The committer **output** is stored in the committer <span class="warn">and</span> in the batcher.

<div class="diagram">
<div class="flow-row">
  <div class="stage"><span class="name">batcher</span><span class="stored in">state_diff<br><span class="gold">accessed_keys</span></span></div>
  <div class="edge"><span class="glyph">→</span></div>
  <div class="stage"><span class="name">committer</span><span class="stored in">state_roots<br><span class="gold">witnesses</span></span></div>
  <div class="edge"><span class="glyph">→</span></div>
  <div class="stage"><span class="name">batcher</span><span class="stored in">state_roots<br><span class="gold">witnesses</span></span></div>
</div>
</div>

<div class="diagram">
<span style="color: #8ec1ff; font-size: 0.82em;">H(input) ≥ H(committer output) ≥ H(batcher output)</span>
</div>

<br>

<hr style="border: none; border-top: 2px solid #2d4a6b; width: 70%; margin: 20px auto;">

<span class="gold">Optimization:</span> persisting the witnesses only once.

---

## Persistency - fetch witnesses from the committer

**Suggestion:** avoid storing the witnesses on the **batcher** side.
Add **Committer Storage** component, so consensus doesn't get blocked by commit requests.

<div class="seq">
<div class="seq-head">
  <div class="node node-committer h1">committer<br>storage</div>
  <div class="node node-committer h2">committer</div>
  <div class="node h3">batcher</div>
  <div class="node h4">batcher<br>storage</div>
  <div class="node h5">consensus</div>
</div>
<div class="seq-msg"><div class="arr left s46"><span class="lbl"><span class="gold">accessed_keys</span>, state_diff</span></div></div>
<div class="seq-msg"><div class="arr right s46"><span class="lbl">state_roots</span></div><div class="arr left s24"><span class="lbl out">witnesses</span></div></div>
<div class="seq-msg"><div class="arr right s68"><span class="lbl">state_roots</span></div></div>
<div class="seq-msg"><div class="arr right s610"><span class="lbl">decision_reached</span></div></div>
<div class="seq-msg"><div class="arr right s810"><span class="lbl">recent state_roots</span></div></div>
<div class="seq-msg"><div class="arr right s210"><span class="lbl out">recent witnesses</span></div></div>
</div>

---

## Tests plan

- The committer gets all the required **accessed keys** - by the OS flow test.
- Batcher and committer **generate witnesses** in the consensus flow, and skip it in the sync flow.
- **Collecting witnesses:**
  - a round fails if witnesses for H-10 are missing.
  - sending only the missing witnesses.
- Witnesses **compression / decompression** test.
- **Regression tests** for witnesses - they are deserialized only on the Python side.

---

## Metrics and alerts

### <span class="gold">Metrics</span>

- Number of accessed keys / facts.
- Witness-height gap between cende and the batcher.
- Number of rounds failed due to missing witnesses.
- Last height learned from sync.

<br>

### <span class="warn">Alerts</span>

- Missing witnesses for height H-10.
- Oversized blob - size approaching the limit.
