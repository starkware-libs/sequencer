---
name: triage_ci_failure
description: Triage CI failures, flaky tests, and broken builds in the sequencer mono-repo. Auto-invoke when a user mentions a failing CI job, flaky test, red check, or pastes a GitHub Actions URL — especially in Slack threads — so context is gathered (PR link, CI job link, base branch) BEFORE any code investigation begins.
---

# Triage CI Failure

When invoked (typically because someone tagged Claude in the mono-repo Slack channel about a CI failure or flaky test), follow this workflow to gather context before investigating.

## Step 1: Gather Required Context

Before starting any investigation, you MUST have the following information. Check if any of these are missing from the message or thread:

### Required Information

| Item | Why Needed | Example |
|------|------------|---------|
| **PR link** or **branch name** | To understand what code is being tested | `https://github.com/starkware-libs/sequencer/pull/12345` or `feature/my-branch` |
| **Failed CI job link** | To read the actual error logs | `https://github.com/starkware-libs/sequencer/actions/runs/123456/job/789` |
| **Base branch** | The branch this PR targets — do NOT assume `main` | `main`, `release/v1.2`, `feature/epic-branch` |
| **Is this a new failure or flaky?** | Determines investigation approach | "Started failing today" vs "Fails ~10% of runs" |

### Nice to Have

- Error message snippet (if not clicking the link)
- Whether this was working before a recent rebase
- Related PRs or recent merges that might have caused regression

---

## Step 2: If Missing Information, Ask First

If ANY required information is missing, reply in the thread asking for it. Do NOT start investigating with incomplete context.

**Template response:**

> To investigate this properly, I need a bit more context:
>
> - [ ] **PR/Branch**: Which PR or branch is failing? (link preferred)
> - [ ] **CI Job**: Link to the failed job so I can read the logs
> - [ ] **Base branch**: What branch is this targeting? (don't assume main)
> - [ ] **Failure pattern**: Is this a new failure or has it been flaky?
>
> Once I have these, I'll dig in!

Adapt this based on what's already provided — only ask for what's missing.

---

## Step 3: Verify the Context

Once you have the required information:

1. **Open the PR** — check the base branch, changed files, and any existing review comments
2. **Read the CI logs** — find the actual error, not just the job name
3. **Check if known flaky** — search CLAUDE.md "Common Gotchas" and recent Slack history for known flaky tests
4. **Determine scope** — is this related to the PR's changes, or a pre-existing/infrastructure issue?

---

## Step 4: Investigate and Report

Only after completing steps 1-3, begin your investigation:

1. **If it's a code issue in the PR**: identify the root cause, propose a fix
2. **If it's a known flaky test**: link to prior discussions, explain the flakiness pattern
3. **If it's infrastructure/transient**: suggest a re-run and explain why
4. **If unclear**: share what you found and what you'd need to dig deeper

Always report back in the Slack thread with:
- What you found
- Whether action is needed
- Proposed next steps (if any)

---

## Step 5: Commit and PR Discipline

### One commit per PR

Each PR must contain exactly **one logical commit**. Before pushing:

- Squash intermediate/fixup commits with `git rebase -i`
- The commit message must follow the repo format: `scope: subject` (no `feat:`/`fix:` prefixes)
- Max 100 chars

### Multiple changes → stacked PRs with Graphite

If the fix touches more than one logical concern (e.g. a bug fix AND a refactor, or a flaky test fix AND an infra change), split them into **separate PRs stacked with Graphite**:

```bash
# Each branch gets one commit, stacked on the previous
gt branch create <branch-name>   # create a new branch on top of current
# ... make changes, one commit ...
gt submit                         # push and open PR; respects the stack
```

Key rules:
- Each `gt branch` = one PR = one commit
- Do not mix unrelated fixes in the same branch
- Use `gt submit --stack` to push the entire stack at once

### Choosing the right base branch

Always confirm the base branch from the PR/CI context. Never assume `main`. Check:

```bash
git log --oneline origin/main..HEAD       # see what's on your branch vs main
# Use mcp__github__pull_request_read with method=get to confirm base
```

---

## Common Patterns in This Repo

From CLAUDE.md — these failures are often NOT code bugs:

- `blockifier_reexecution` — transient GCloud network issues; suggest re-run
- `merge-gatekeeper` — downstream failure (other checks failed first)
- Formatting failures — run `scripts/rust_fmt.sh` (nightly toolchain), NOT `cargo fmt` directly

---

## Anti-Patterns to Avoid

- Starting to fix code before understanding which branch/PR is affected
- Assuming the base branch is `main` without checking
- Using `cargo fmt` instead of `scripts/rust_fmt.sh`
- Proposing fixes without reading the actual CI error logs
- Committing directly without confirming the target branch
- Mixing unrelated fixes into one PR instead of using stacked PRs
- Multiple commits in a single PR
