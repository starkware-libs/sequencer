---
name: commit_discipline
description: Enforce commit and PR discipline for the sequencer repo — one commit per PR, stacked PRs for multiple concerns, proper commit message format. Auto-invoke when creating commits, PRs, or when fixing multiple issues.
---

# Commit and PR Discipline

Guidelines for commits and PRs in this repo.

## Prefer one commit per PR

Each PR should ideally contain a single logical commit. Before pushing:

- Squash intermediate/fixup commits with `git rebase -i`
- The commit message must follow the repo format: `scope: subject` (no `feat:`/`fix:` prefixes)
- Max 100 chars

## Multiple changes → stacked PRs

If the fix touches more than one logical concern (e.g. a bug fix AND a refactor, or a flaky test fix AND an infra change), split them into **separate PRs**, each with one commit, stacked on top of each other.

The repo uses Graphite Optimizer in CI (`withgraphite/graphite-ci-action`). Two ways to handle stacks:

**Plain git (always works):**

```bash
git switch -c feature/part-1 origin/main
# ... make first change, single commit ...
git push -u origin feature/part-1
# open PR #1 targeting main

git switch -c feature/part-2          # branched from part-1
# ... make second change, single commit ...
git push -u origin feature/part-2
# open PR #2 targeting feature/part-1
```

**Graphite CLI (if installed):**

```bash
gt branch create <branch-name>        # creates a branch stacked on current
# ... make changes, one commit ...
gt submit                              # pushes and opens PR respecting the stack
```

Verify `which gt` before suggesting `gt` commands — fall back to plain git if it's not available.

## Choosing the right base branch

The repo's default parent branch is stored in `scripts/parent_branch.txt`. Never assume `main` — always check:

```bash
cat scripts/parent_branch.txt         # repo's configured default parent
# Or use mcp__github__pull_request_read with method=get to read the PR's baseRefName
```
