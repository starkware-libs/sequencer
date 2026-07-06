---
marp: true
title: Committer os_input — Commit with Witnesses
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
           padding: 10px 18px; display: flex; flex-direction: column; align-items: center; gap: 5px; min-width: 360px; }
  .stage-committer { background: #5c4a1b; border-color: #7d6a2f; color: #ffe9b0; }
  .stage.mark { border-width: 2px; }
  .parallel { display: flex; gap: 20px; justify-content: center; }
  .parallel .stage { min-width: 300px; }
  .small-block { font-size: 0.7em; margin-top: 18px; }
  .row { display: flex; align-items: center; justify-content: center; gap: 64px; }
  .row .diagram { margin-top: 0; }
  .agenda { text-align: left; }
  .agenda .head { display: block; font-size: 1.2em; margin-bottom: 16px; }
  .agenda ol { margin: 0; padding-left: 24px; line-height: 2.2; font-size: 1.05em; }
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

# **Commit and Fetch Witnesses**

## Core Committer Logic · Design Review

---

## Committing a state diff

<div class="diagram">
  <div class="stage">
    <div class="name">1 · Fetch nodes</div>
    <div class="stored out">original root → W leaves</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage">
    <div class="name">2 · Updated tree</div>
    <div class="stored in">determine structure and fill hashes</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage">
    <div class="name">3 · Write to storage</div>
    <div class="stored out">persist the updated forest</div>
  </div>
</div>

<div class="small-block" style="text-align: center"><span class="gold">IO</span> &nbsp;·&nbsp; <span class="accent">CPU</span></div>
<div class="small-block ref">This is the current committer flow.</div>

---

## Recap: What the OS needs?

The **prev** state, the **new** state, and the **transition** between them.

<br>

A state is:
- **root hash**
- **leaves** (initial reads / state diff)
- **witnesses** - authentication pathes against the root

<div class="diagram">
<img src="diagrams/state_trees.svg" width="720" alt="prev and new state trees" />
</div>

---

## Basic suggestion - Fetch witnesses twice

<div class="diagram">
  <div class="stage stage-committer">
    <div class="name">Fetch witnesses</div>
    <div class="stored out"><span class="gold">original</span> root → R/W leaves</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage">
    <div class="name">1 · Fetch nodes</div>
    <div class="stored out">original root → W leaves</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage">
    <div class="name">2 · Updated tree</div>
    <div class="stored in">determine structure and fill hashes</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage stage-committer">
    <div class="name">Fetch witnesses</div>
    <div class="stored out"><span class="gold">updated</span> root → R/W leaves</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage">
    <div class="name">3 · Write to storage</div>
    <div class="stored out">persist the updated forest</div>
  </div>
</div>

<div class="small-block" style="text-align: center"><span class="gold">IO</span> &nbsp;·&nbsp; <span class="accent">CPU</span></div>

---

## Agenda

- <span class="gold">First</span> fetch optimizations.
- <span class="gold">Second</span> fetch optimizations.

---

## <span class="gold">First</span> fetch optimization (1) - concurrency

<div class="diagram">
  <div class="stage">
    <div class="name">1 · Fetch nodes</div>
    <div class="stored out">original root → W leaves</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="parallel">
    <div class="stage">
    <div class="name">2 · Updated tree</div>
    <div class="stored in">determine structure and fill hashes</div>
  </div>
    <div class="stage stage-committer">
    <div class="name">Fetch witnesses</div>
    <div class="stored out"><span class="gold">original</span> root → R/W leaves</div>
  </div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage">
    <div class="name">Fetch witnesses</div>
    <div class="stored out"><span class="gold">updated</span> root → leaves</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage">
    <div class="name">3 · Write to storage</div>
    <div class="stored out">persist the updated forest</div>
  </div>
</div>

<div class="small-block" style="text-align: center"><span class="gold">IO</span> &nbsp;·&nbsp; <span class="accent">CPU</span></div>

<div class="small-block ref">The benchmarks were run on this version.</div>
<div class="small-block ref">The W nodes are already in the cache.</div>

---

## Fetching rules

<div class="diagram">
<img src="diagrams/node_types.svg" width="620" alt="binary and edge node collection" />
</div>

- **Binary node** — collect both children, and keep collecting its touched children.
- **Edge node** — collect its bottom with the bottom's children, and keep fetching the bottom if it's touched.

---

## <span class="gold">First</span> fetch optimization (2) - a single scan

<div class="diagram">
  <div class="stage stage-committer">
    <div class="name">1 · Fetch nodes and witnesses</div>
    <div class="stored out"><span class="gold">original</span> root → R/W leaves</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage">
    <div class="name">2 · Updated tree</div>
    <div class="stored in">determine structure and fill hashes</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage">
    <div class="name">Fetch witnesses</div>
    <div class="stored out"><span class="gold">updated</span> root → leaves</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage">
    <div class="name">3 · Write to storage</div>
    <div class="stored out">persist the updated forest</div>
  </div>
</div>

<div class="small-block ref">A complex code change - merging two scanning logics.</div>
<div class="small-block ref">Delaying the start of the hash calculation.</div>

---

## <span class="gold">First</span> fetch optimization (3) - combined

<div class="diagram">
  <div class="stage stage-committer">
    <div class="name">1 · Fetch nodes and witnesses</div>
    <div class="stored out"><span class="gold">original</span> root → W leaves</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="parallel">
    <div class="stage">
    <div class="name">2 · Updated tree</div>
    <div class="stored in">determine structure and fill hashes</div>
  </div>
    <div class="stage stage-committer">
    <div class="name">Fetch witnesses</div>
    <div class="stored out"><span class="gold">original</span> root → R leaves</div>
  </div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage">
    <div class="name">Fetch witnesses</div>
    <div class="stored out"><span class="gold">updated</span> root → leaves</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage">
    <div class="name">3 · Write to storage</div>
    <div class="stored out">persist the updated forest</div>
  </div>
</div>

<div class="small-block ref">Suggestion: implement if there is a performance requirement.</div>

---

## <span class="gold">Second</span> fetch optimization - <span class="warn">invalid</span>

<div class="diagram">
  <div class="stage">
    <div class="name">1 · Fetch nodes and witnesses</div>
    <div class="stored out"><span class="gold">original</span> root → R/W leaves</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage">
    <div class="name">2 · Updated tree</div>
    <div class="stored in">determine structure and fill hashes</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage stage-committer">
    <div class="name">Collect updated nodes</div>
    <div class="stored in">fetch the nodes from step 2</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage">
    <div class="name">3 · Write to storage</div>
    <div class="stored out">persist the updated forest</div>
  </div>
</div>

---

## Edge case

<div class="diagram">
<img src="diagrams/collapse.svg" width="680" alt="binary node collapse: original to updated" />
</div>

<div class="small-block">

- **UC** is an unchanged binary node.
- The OS proves that the new edge **R → UC** forms a valid Patricia structure, so **UC** is not an edge node.
- The children of **UC** are <span class="warn">not fetched</span> when the tree is updated.

</div>

---

## <span class="gold">Second</span> fetch optimization - collect the edge-case nodes

<div class="diagram">
  <div class="stage">
    <div class="name">1 · Fetch nodes and witnesses</div>
    <div class="stored out"><span class="gold">original</span> root → R/W leaves</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage">
    <div class="name">2 · Updated tree</div>
    <div class="stored in">determine structure and fill hashes</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage stage-committer mark">
    <div class="name">Fetch edge-case nodes</div>
    <div class="stored out">preimage of unchanged binary nodes<br>with a deleted binary parent</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage stage-committer">
    <div class="name">Collect updated nodes</div>
    <div class="stored in">fetch the nodes from step 2</div>
  </div>
  <div class="vedge"><span class="down">↓</span></div>
  <div class="stage">
    <div class="name">3 · Write to storage</div>
    <div class="stored out">persist the updated forest</div>
  </div>
</div>

<div class="small-block ref">The change is feasible but the improvement is limited, as most of the nodes are cached.</div>
