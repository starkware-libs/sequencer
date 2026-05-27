# GitHub REST API reference for CI triage

All endpoints below are reachable **without authentication** on `starkware-libs/sequencer` (public repo) unless noted. Substitute `O=starkware-libs`, `R=sequencer`. Use `WebFetch` (claude.ai web), `curl`, or `mcp__github__*` tools.

## Contents
- [Around the PR](#around-the-pr)
- [Around the commit](#around-the-commit)
- [Around the workflow run](#around-the-workflow-run)
- [Around the check-run](#around-the-check-run)
- [Pagination](#pagination)
- [filter=latest vs filter=all](#filterlatest-vs-filterall)
- [Rate limit](#rate-limit)
- [Where raw logs live](#where-raw-logs-live)
- [Tool-tier cheat sheet](#tool-tier-cheat-sheet)
- [Patterns to recognize](#patterns-to-recognize)

## Around the PR

| Endpoint | What you learn |
|---|---|
| `GET /repos/{O}/{R}/pulls/{pr}` | `head.sha`, `head.ref`, `base.ref`, mergeable/draft state |
| `GET /repos/{O}/{R}/pulls/{pr}/files?per_page=100` | All changed files — judge whether the failing test/crate is plausibly affected by this PR |
| `GET /repos/{O}/{R}/issues/{pr}/comments?per_page=100` | Earlier triage discussion, prior re-run requests |
| `GET /repos/{O}/{R}/issues/{pr}/events?per_page=100` | Re-runs, force-pushes, base-branch changes |

Also check `scripts/parent_branch.txt` in the checked-out repo: the base branch isn't always `main` (stacked PRs target feature branches), and assuming `main` misleads your "is this on the base branch too?" check.

## Around the commit

| Endpoint | What you learn |
|---|---|
| `GET /repos/{O}/{R}/commits/{sha}` | Author, message, files touched |
| `GET /repos/{O}/{R}/commits/{sha}/check-runs?filter=all&per_page=100` | Every check-run at this SHA, including re-runs |
| `GET /repos/{O}/{R}/commits/{sha}/status` | Legacy combined-status checks (some third-party integrations live here, not under `/check-runs`) |
| `GET /repos/{O}/{R}/actions/runs?head_sha={sha}&per_page=100` | All workflow runs triggered by this commit |

## Around the workflow run

| Endpoint | What you learn |
|---|---|
| `GET /repos/{O}/{R}/actions/runs/{run_id}` | `name`, `conclusion`, `head_sha`, `head_branch`, `run_attempt`, `previous_attempt_url` |
| `GET /repos/{O}/{R}/actions/runs/{run_id}/jobs?filter=all&per_page=100` | Every job + each step's name and conclusion → which *step* failed without log access. **Always pass `filter=all`**: the default returns only the latest attempt, so on a re-run the original failing job is hidden and you'll see all-green. |
| `GET /repos/{O}/{R}/actions/runs/{run_id}/attempts/{n}/jobs?per_page=100` | Jobs from a specific prior attempt — compare across re-runs |
| `GET /repos/{O}/{R}/actions/runs/{run_id}/artifacts?per_page=100` | Artifact names + URLs (artifact *download* needs auth) |
| `GET /repos/{O}/{R}/actions/runs/{run_id}/timing` | Billable time per job — for "CI got slower" triage |
| `GET /repos/{O}/{R}/actions/jobs/{job_id}` | Single job's status + `check_run_url` (when you have a job id but not a check id) |

## Around the check-run

`job.id == check_run.id` in GitHub Actions — the same numeric id works in both the Actions and Checks APIs.

| Endpoint | Notes |
|---|---|
| `GET /repos/{O}/{R}/check-runs/{check_id}/annotations?per_page=100` | **Primary failure signal.** Returns a bare JSON array (no `total_count`); paginate via `?page=N` + the `Link` header. |
| `GET /repos/{O}/{R}/check-runs/{check_id}` | `output.title`/`summary`/`text` — useful when set, but `null` on most Actions check-runs. Treat as fallback. |

## Pagination

Default page size is 30; pass `per_page=100`. Most list endpoints return `{"total_count": N, "<items>": [...]}` — paginate until you've covered `total_count`. The annotations endpoint is the exception: it returns a bare array, so use `?page=N` + the `Link` header.

## filter=latest vs filter=all

On `/commits/{sha}/check-runs` and `/actions/runs/{id}/jobs`, `filter=latest` (the default) collapses re-runs to the latest attempt. Use `filter=all` to see re-run history and to catch a failing earlier attempt that the default hides — essential for flakiness diagnosis and for job links that point at an old attempt.

## Rate limit

Unauthenticated requests share a **60-per-hour-per-IP** quota. A thorough triage (PR + files + comments + check-runs + jobs + annotations + history) can run into it. If authed (`gh` locally or GitHub MCP token), you get 5000/hr — prefer that. If unauth, prioritize the few high-signal calls (re-run check + fast path) and only pull wider context when needed.

## Where raw logs live

`GET /actions/jobs/{job_id}/logs` and `GET /actions/runs/{run_id}/logs` return **403 without auth**. Raw log text is only reachable via:

1. **`gh` CLI, authed** — `gh run view --repo {O}/{R} --job {job_id} --log-failed` (add `--attempt {N}` for a specific attempt). Works on this public repo when `gh auth status` is logged in.
2. **`mcp__github__*`** — exposes check-run metadata, not raw Actions logs (verify in-session; the surface evolves).
3. **Ask the user to paste** — last resort.

## Tool-tier cheat sheet

| Environment | Primary fetch path | Raw logs? |
|---|---|---|
| Claude.ai web (no `gh`, no GitHub MCP) | `WebFetch` against the unauth endpoints above | No — ask user to paste if annotations didn't cover it |
| Claude Code locally with `gh` authed | `gh` CLI for run/job/logs; `WebFetch`/`mcp__github__*` for the rest | Yes — `gh run view --log-failed` |
| Claude Code, GitHub MCP only (no `gh`) | `mcp__github__*` for PR/check metadata; `WebFetch` for annotations + run/job lists | Verify if your MCP exposes Actions logs; if not, ask |

The metadata endpoints work in all three — they're the portable baseline.

## Patterns to recognize

- `head_branch` like `gh-readonly-queue/main/pr-NNNN-...` → a merge-queue run, not a regular PR run. The PR number is in the branch name.
- `run_attempt > 1` or a non-null `previous_attempt_url` → someone re-ran it; comparing attempts is a quick flakiness check.
