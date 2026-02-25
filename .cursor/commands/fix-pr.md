---
description: "Fix a PR: check out the branch, triage CI failures, address review comments, and commit fixes."
---
# Fix PR

Given a PR link, follow these steps in order.

## 1. Understand the PR
- Sync and check out the PR branch:
  ```bash
  gt sync
  gt checkout <branch_name>
  ```
- Read the PR description and commits.
- Identify affected crates: `git diff --name-only <target-branch>..HEAD | grep '^crates/' | cut -d/ -f2 | sort -u`

## 2. Triage CI Failures
Open the PR's Checks tab. For each failure, classify:
- **PR-related**: Failure is in a crate touched by the PR (or its direct dependent), and the error references PR code. → Fix it.
- **Sporadic**: Failure is in an unrelated crate, is a timeout/OOM/infra error, or the same job fails on `main`. → Ignore it.

When in doubt, check if it reproduces locally with `cargo test -p <crate>`.

## 3. Address Review Comments
- Read all review comments (GitHub + Reviewable).
- Code change requests → implement them.
- Nits → fix them.
- Unclear intent → leave a `// TODO(agent): clarify with reviewer — <question>` rather than guessing.
- Do NOT silently ignore any comment.

## 4. Fix Issues
- Keep changes minimal and focused.
- If changing a public API, check all callers in the workspace.

## 5. Local Verification
Run on all affected crates:
```bash
cargo check -p <crate_name>
cargo test -p <crate_name>
cargo fmt -- --check
```

If tests fail from your changes, fix them. If unrelated, note it and move on.

## 6. Commit and Restack

Before committing, review what you're about to stage:
```bash
git diff --name-only
git diff --stat
```
Verify that **only files relevant to the PR** are modified. If you see unexpected files, investigate before proceeding. Do NOT commit unrelated changes.

Once verified:
```bash
git add -A
gt modify --all
gt restack
```

If `gt restack` has conflicts: resolve them, run `gt add .`, then `gt continue`. Re-run verification (step 5) on restacked branches.

**Do NOT run `gt submit`.** Instead, report back:
- What changes you made and in which files.
- Why each change was made (CI fix, review comment, etc.).
- Any unresolved issues, sporadic CI failures, or comments you weren't sure about.
