Load `CLAUDE.md` first for repo-wide routing, commands, and safety boundaries.

Project-local skills live in `.agents/skills` and `.codex/skills`. Load only the skill that matches the task.

On case-insensitive filesystems this file also serves as `AGENTS.md`.

Do not modify gated files or any secret-like files without explicit human approval.

<roles>
| Role | Model tier | Responsibility | Boundaries |
|------|------------|----------------|------------|
| Orchestrator | Frontier | choose target branch, touched crates, required skills, execution order, and review scope | never makes broad implementation edits |
| Implementer | Mid-tier | write code inside an assigned crate or narrow file set | never changes gated contracts without an approval packet |
| Verifier | Mid-tier | run focused build/test/presubmit commands and report exact failures | never rewrites implementation unless reassigned |
| Reviewer | Frontier | inspect diff for regressions, fan-out risks, and missing tests | never lands fixes directly |
| Release Specialist | Frontier or mid-tier | handle `.github`, `scripts`, `deployments`, branch-chain, and artifact tasks | only activated after human approval |
</roles>

<delegation_protocol>
1. ANALYZE
   - Determine base branch, touched crates, and whether the task is crate-local or cross-cutting.
   - Load only the relevant repo skills.
2. CLASSIFY
   - Routine crate-local change -> Implementer
   - Cross-component, protocol, storage, or release work -> Orchestrator keeps control or routes to Release Specialist
   - Verification only -> Verifier
3. DECOMPOSE
   - Split by non-overlapping crate sets.
   - Keep `apollo_node`, `apollo_node_config`, `Cargo.toml`, `.github`, `scripts`, and `deployments` serialized.
4. DELEGATE
   - Each sub-task must include exact files to read, files allowed to modify, required skills, base branch, and verification commands.
5. INTEGRATE
   - Reconcile overlapping assumptions before merging outputs.
6. REVIEW
   - Reviewer checks for fan-out risks: interface drift, storage compatibility, topology mismatches, release-branch mistakes, and missing tests.
</delegation_protocol>

<task_format>
## Task: [clear title]

**Objective**: [what done looks like]

**Branch context**:
- Base branch: `[main | main-vX.Y.Z | release-*]`
- Parent branch for local presubmit: `[same as base branch]`

**Context package**:
- Files to read: [exact paths]
- Files to modify: [exact paths]
- Skills to load: [skill directory names]

**Acceptance criteria**:
- [ ] Behavior change is covered by the smallest relevant test
- [ ] Verification commands are listed explicitly
- [ ] No gated file changed without approval

**Verification**:
- `SEED=0 cargo test -p <crate>`
- `cargo test -p workspace_tests` when manifests or publish metadata changed
- `scripts/local_presubmit.sh --parent_branch <base_branch>` for broad or release-branch work

**Handoff**:
- Report changed files, commands run, and any remaining risks
</task_format>

<parallel_execution>
Safe to parallelize:
- Independent crate-local edits that do not touch shared `*_types`, `apollo_node`, manifests, or deployment files
- Test writing in a dedicated test crate once interfaces are stable
- Docs and skill updates

Must serialize:
- `Cargo.toml`, `Cargo.lock`, toolchains, and workspace metadata
- `crates/*_types/src/communication.rs`
- `crates/apollo_node/src/{communication,clients,components,servers}.rs`
- `crates/apollo_node_config/**`
- `crates/apollo_protobuf/src/proto/**`
- `.github/**`, `scripts/**`, `deployments/**`
- release-branch and artifact work
</parallel_execution>

<state_machine>
PENDING -> ASSIGNED -> IN_PROGRESS -> REVIEW -> { APPROVED -> DONE | REJECTED -> IN_PROGRESS }
                                 \-> BLOCKED -> ESCALATE

Block if:
- target branch is unclear
- a gated contract must change
- private sibling repos are required
- storage or protocol compatibility is uncertain
</state_machine>

<escalation>
Escalate to a human when:
- the task requires `.github`, `scripts`, `deployments`, toolchain, or artifact changes
- a public/protocol/storage contract must change
- the correct release branch or backport target is unclear
- the task depends on `starkware-industries/starkware` or `starkware-industries/starkware-envs-production`

Escalation format:
**ESCALATION**: [summary]
**Branch context**: [base branch]
**Blocked on**: [specific contract or repo]
**Options**:
1. [Option] - Tradeoff
2. [Option] - Tradeoff
**Recommendation**: [best option]
</escalation>
