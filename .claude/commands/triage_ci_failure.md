# Triage CI Failure

When tagged in the mono-repo Slack channel about a CI failure or flaky test, follow this workflow to gather context before investigating.

## Step 1: Gather Required Context

Before starting any investigation, you MUST have the following information. Check if any of these are missing from the message or thread:

### Required Information

| Item | Why Needed | Example |
|------|------------|---------|
| **PR link** or **branch name** | To understand what code is being tested | `https://github.com/starkware-libs/sequencer/pull/12345` or `feature/my-branch` |
| **Failed CI job link** | To read the actual error logs | `https://github.com/starkware-libs/sequencer/actions/runs/123456/job/789` |
| **Base branch** | The branch this PR targets (not always `main`) | `main`, `release/v1.2`, `feature/epic-branch` |
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
> - [ ] **Base branch**: What branch is this targeting? (main? a release branch?)
> - [ ] **Failure pattern**: Is this a new failure or has it been flaky?
>
> Once I have these, I'll dig in!

Adapt this based on what's already provided - only ask for what's missing.

---

## Step 3: Verify the Context

Once you have the required information:

1. **Open the PR** - Check the base branch, changed files, and any existing review comments
2. **Read the CI logs** - Find the actual error, not just the job name
3. **Check if known flaky** - Search CLAUDE.md "Common Gotchas" and recent Slack history for known flaky tests
4. **Determine scope** - Is this related to the PR's changes, or a pre-existing/infrastructure issue?

---

## Step 4: Investigate and Report

Only after completing steps 1-3, begin your investigation:

1. **If it's a code issue in the PR**: Identify the root cause, propose a fix
2. **If it's a known flaky test**: Link to prior discussions, explain the flakiness pattern
3. **If it's infrastructure/transient**: Suggest a re-run and explain why
4. **If unclear**: Share what you found and what you'd need to dig deeper

Always report back in the Slack thread with:
- What you found
- Whether action is needed
- Proposed next steps (if any)

---

## Common Patterns in This Repo

From CLAUDE.md - these failures are often NOT code bugs:

- `blockifier_reexecution` - Transient GCloud network issues
- `merge-gatekeeper` - Downstream failure (other checks failed first)
- Formatting failures - Run `scripts/rust_fmt.sh` with nightly toolchain

---

## Anti-Patterns to Avoid

- Starting to fix code before understanding which branch/PR is affected
- Assuming the base branch is `main` without checking
- Using `cargo fmt` instead of `scripts/rust_fmt.sh`
- Proposing fixes without reading the actual CI error logs
- Committing directly without confirming the target branch
