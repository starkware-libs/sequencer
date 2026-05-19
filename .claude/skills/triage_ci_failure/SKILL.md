---
name: triage_ci_failure
description: Triage CI failures, flaky tests, and broken builds in the sequencer mono-repo. Auto-invoke when a user mentions a failing CI job, flaky test, red check, or pastes a GitHub Actions URL — context (PR link, CI job link, base branch) must be gathered BEFORE any code investigation begins.
---

# Triage CI Failure

When invoked (typically because someone tagged Claude in the mono-repo Slack channel about a CI failure or flaky test), follow this workflow to gather context before investigating.

## Step 1: Gather Required Context

Before starting any investigation, you MUST have the following information. Check if any of these are missing from the message or thread:

### Required Information

| Item | Why Needed | Example |
|------|------------|---------|
| **PR link** or **branch name** | To understand what code is being tested | `https://github.com/starkware-libs/sequencer/pull/12345` or `feature/my-branch` |
| **Failed CI job link** | To get a `details_url` you can open and ask the user to paste relevant log lines from | `https://github.com/starkware-libs/sequencer/actions/runs/123456/job/789` |
| **Base branch** | The branch this PR targets — check `scripts/parent_branch.txt` for the default, don't assume `main` | `main`, `release/v1.2`, `feature/epic-branch` |
| **Is this a new failure or flaky?** | Determines investigation approach | "Started failing today" vs "Fails ~10% of runs" |

### Nice to Have

- Error message snippet (the available GitHub MCP tools only expose check-run metadata, not raw Actions log output, so a pasted snippet often unblocks the fastest investigation)
- Whether this was working before a recent rebase
- Related PRs or recent merges that might have caused regression

---

## Step 2: If Missing Information, Ask First

If ANY required information is missing, reply in the thread (Slack or PR comment, wherever you were invoked) asking for it. Do NOT start investigating with incomplete context.

**Template response:**

> To investigate this properly, I need a bit more context:
>
> - [ ] **PR/Branch**: Which PR or branch is failing? (link preferred)
> - [ ] **CI Job**: Link to the failed job and, if convenient, paste the relevant error lines
> - [ ] **Base branch**: What branch is this targeting? (don't assume main)
> - [ ] **Failure pattern**: Is this a new failure or has it been flaky?
>
> Once I have these, I'll dig in!

Adapt this based on what's already provided — only ask for what's missing.

---

## Step 3: Verify the Context

Once you have the required information:

1. **Open the PR** — use `mcp__github__pull_request_read` with `method=get` to confirm the base branch, changed files, and any existing review comments
2. **Inspect the failed check** — use `method=get_check_runs` for status/conclusion and the `details_url`; for raw Actions logs you'll need the user to paste them (no MCP tool returns them directly)
3. **Check if known flaky** — search CLAUDE.md "Common Gotchas" and recent Slack history for known flaky tests
4. **Determine scope** — is this related to the PR's changes, or a pre-existing/infrastructure issue?

---

## Step 4: Investigate and Report

Only after completing steps 1-3, begin your investigation:

1. **If it's a code issue in the PR**: identify the root cause, propose a fix
2. **If it's a known flaky test**: link to prior discussions, explain the flakiness pattern
3. **If it's infrastructure/transient**: suggest a re-run and explain why
4. **If unclear**: share what you found and what you'd need to dig deeper

Always report back in the thread with:
- What you found
- Whether action is needed
- Proposed next steps (if any)

---

## Step 5: Commit and Push

When fixing the issue, create one commit per PR.

---

## Common Patterns in This Repo

From CLAUDE.md — these failures are often NOT code bugs:

- `blockifier_reexecution` — transient GCloud network issues; suggest re-run
- `merge-gatekeeper` / `merge-gatekeeper-new` — downstream failures (other checks failed first)
- Formatting failures — run `scripts/rust_fmt.sh` (uses pinned nightly toolchain), NOT `cargo fmt` directly
