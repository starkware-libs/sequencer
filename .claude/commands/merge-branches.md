---
description: Merge a source branch into a destination branch via scripts/merge_branches.py, walk through each conflict interactively, commit as fix_conflicts, and post inline PR review comments tagging the authors who caused each conflict.
---

# Merge Branches

Run a branch-merge end-to-end: kick off `scripts/merge_branches.py`, resolve each conflict interactively with the user, build-verify, commit, push, and document every resolution as an inline review comment on the resulting PR. Follow the steps in order. Do not batch-skip steps — interactivity is the point.

## Background

- The merge script `scripts/merge_branches.py` creates a fresh branch off the destination, merges the source, and pushes a PR with conflict markers committed in-place. It also adds a comment to the PR linking the conflicting files on each side.
- This skill picks up from that point: it resolves the conflict markers, verifies the build, commits, and posts per-conflict inline review comments tagging the people whose changes collided.
- Graphite (`gt`) is the project's git wrapper — use it for commits and pushes.

---

## Step 1: Gather inputs

Ask the user (use `AskUserQuestion`):

1. **Source branch** (the branch being merged *from*, e.g. `main-v0.14.2`).
2. **Destination branch** (the branch being merged *into*, e.g. `main`). If the source has a default in `scripts/merge_paths.json`, offer it as a recommended option.
3. **Use a worktree?** Recommend yes — merges leave the local branch in an unusual state. If yes, use `EnterWorktree` (creating one based on `origin/<dst>` via `git worktree add` if a custom base ref is needed).

Confirm the choices in one sentence before proceeding.

---

## Step 2: Fetch and run the merge script

```bash
git fetch origin <src-branch> <dst-branch>
python3 scripts/merge_branches.py --src <src-branch> --dst <dst-branch>
```

Capture the script's output. From it, extract:
- The new merge branch name (`<user>/merge-<src>-into-<dst>-<timestamp>`).
- The PR URL / number created by `gh pr create`.
- The list of conflicted files (printed under `git status -s | grep -E ...`).

If the script ran with `gt`-untracked state, run `gt track --parent <dst-branch>` so subsequent `gt modify`/`gt submit` work.

**If the script errors out or `git status` shows entries that aren't `UU` (both modified)** — e.g. `DU` / `UD` (delete-vs-modify), `AU` / `UA` (rename-vs-modify), `AA` (both added) — the merge produced conflicts with no in-file markers. The marker-grep in Step 3 will miss them entirely. Surface these to the user explicitly, propose a resolution per file (keep / drop / port the change), and apply with `git rm` or `git add` once decided. Only then proceed to Step 3 for the remaining `UU` files.

---

## Step 3: Enumerate conflicts

**Prerequisite** — before doing anything in this step, have the user mark all files in the PR as viewed on GitHub (or Reviewable). Otherwise the `fix_conflicts` commit will not appear as a separate revision in review, and the per-conflict inline comments in Step 8 will land in a diff that mixes the merge markers with the resolution, making the review unreadable. Confirm with the user that this is done before continuing.

Use `grep` to find conflict markers in every reported file:

```bash
grep -n "<<<<<<\|>>>>>>\|||||||" <files>
```

For each conflict region, capture:
- The file and line range.
- The **HEAD** (destination) section.
- The **base** section (between `|||||||` and `=======`) — `git config merge.conflictstyle diff3` is set by the script, so the merge base is shown explicitly.
- The **incoming** (source) section.

Also count conflicts per file so the user knows how many decisions to expect.

---

## Step 4: Identify authors for each conflict

For each conflicting hunk, identify who introduced each side so they can be tagged later. Run on the merge-base:

```bash
MERGE_BASE=$(git merge-base origin/<src> origin/<dst>)
# Author of dst-side change:
git log --format='%h %an <%ae> %s' -S '<identifying-token>' "$MERGE_BASE..origin/<dst>" -- <file>
# Author of src-side change:
git log --format='%h %an <%ae> %s' -S '<identifying-token>' "$MERGE_BASE..origin/<src>" -- <file>
```

Pick a distinctive token from each side of the conflict (a new identifier, a removed crate name, etc.). Look up the GitHub handle of the author by reading the PR (`mcp__github__pull_request_read` with the `#NNNN` from the commit subject) and capturing `user.login`.

Record `{ file, line, side, author_handle, pr_number }` for each conflict — this drives Step 7.

---

## Step 5: Resolve conflicts one by one

For **each** conflict region (in file order, top to bottom within a file):

1. Show the user a tight summary:
   - **File and line.**
   - **What the destination side did** (and which PR / who).
   - **What the source side did** (and which PR / who).
   - **Proposed resolution** with reasoning. Common patterns:
     - *Orthogonal changes* → keep both (e.g. two independent params added at the same position).
     - *One side removed something now unused, the other added something now used* → drop the removed item, keep the added one. Verify "unused" with `grep` before claiming it.
     - *Same semantic change in different terms* → pick one, ensure all call sites match.
2. Use `AskUserQuestion` with options:
   - **Accept** the proposed resolution.
   - **Edit** / give feedback (free text via "Other").
   - **Skip** — leave the conflict markers, come back later.
3. On accept, apply the change with `Edit` (replace the entire `<<<<<<<` … `>>>>>>>` block, including markers, with the resolved text).
4. After all conflicts in a file are resolved, verify no markers remain in that file.

Do NOT batch all proposals before asking — propose, ask, apply, then move on. The user wants to think through each one.

---

## Step 6: Build-verify and catch silent merge skews

Run `cargo check` on every crate that owns a conflicted file:

```bash
cargo check -p <crate>
```

Textual conflicts are only half the story — the merge driver doesn't catch **silent API skews** where one side renamed a type or changed a signature and the other side's text merged cleanly but no longer typechecks. If `cargo check` fails:

1. Read the error.
2. Identify which side introduced the breaking change (usually `main`-side refactors).
3. Propose a fix to the user with the same Accept / Edit / Skip flow as Step 5.
4. Record it as an additional "silent skew" entry for Step 7.

Repeat until the build is clean.

---

## Step 7: Commit and push

Stage the resolved files and create the `fix_conflicts` commit as a **new commit on top of the merge commit** (do not amend — the resolution needs to stay as its own diff in review). Then push.

The commit message must satisfy the project's commitlint config (`commitlint.config.js` — `scope: subject` format, scope must be in `AllowedScopes`). For a generic merge resolution use `workspace: fix merge conflicts`; if the conflicts were limited to one crate, narrow the scope (e.g. `blockifier: fix merge conflicts`).

Examples (pick what fits the user's workflow):

```bash
# Graphite (project default — gt modify -c adds a new commit, doesn't amend)
git add <resolved-files>
gt modify -cam "workspace: fix merge conflicts"
gt submit

# Plain git
git add <resolved-files>
git commit -m "workspace: fix merge conflicts"
git push
```

Confirm the PR was updated by checking the push output or `gh pr view <PR#>`.

---

## Step 8: Post per-file inline review comments

Build a single PR review with one inline comment per conflict (and per silent-skew fix). Use `gh api` to POST to `/repos/{owner}/{repo}/pulls/{PR#}/reviews`. Each comment object needs:

- `path` — repo-relative file path.
- `line` — the line number of the resolved hunk in the **head** commit (the `fix_conflicts` commit). Use `git rev-parse HEAD` for `commit_id`.
- `side: "RIGHT"`.
- `body` — explain the conflict, the resolution, and cc the authors with `@<handle>`.

Suggested comment template:

```markdown
**Conflict:** <one-line description>.
- `<dst-branch>` (@<dst-author>, #<dst-pr>) <what they did>.
- `<src-branch>` (@<src-author>, #<src-pr>) <what they did>.

**Resolution:** <what we did and why>.

cc @<dst-author> @<src-author>
```

For silent skews (Step 6), label them as **"Silent merge skew (not a textual conflict)"** so reviewers know why a comment landed somewhere with no conflict markers.

Write the review payload to a tmp JSON file (because the bodies contain newlines and markdown that don't survive `-F` flags cleanly) and submit:

```bash
gh api repos/<owner>/<repo>/pulls/<PR#>/reviews \
  --method POST --input /tmp/pr_review.json
```

The top-level `body` of the review should be a short header: *"Per-file conflict resolution notes (commit `fix_conflicts`). Source side: @<src-author>. Destination side: @<dst-author>. Inline notes below."* Set `event: "COMMENT"` (not `APPROVE` or `REQUEST_CHANGES`).

Clean up `/tmp/pr_review.json` afterward.

---

## Step 9: Report back

Tell the user:
- PR URL.
- Commit SHA of `fix_conflicts`.
- Number of conflicts resolved + number of silent skews fixed.
- Link to the posted review.

Then stop. Do not merge the PR — that's a separate decision.

---

## Notes

- **Never** mask a failing `cargo check` with `#[ignore]` or by deleting code — investigate the skew (Step 6).
- **Never** amend the merge commit — keep `fix_conflicts` as a separate commit so reviewers can see the resolution diff cleanly.
- If the user says **Skip** on any conflict, do not commit until they come back and resolve it — leaving conflict markers in a commit will break CI.
- If `mcp__github__pull_request_read` is unavailable, fall back to `gh pr view <PR#> --json author` to fetch the GitHub handle.
