---
name: triage_ci_failure
description: Triage CI failures, flaky tests, and broken builds in the sequencer mono-repo. Use when a user mentions a failing CI job, flaky test, red check, or shares a GitHub Actions / PR URL — the skill pulls failure context directly from the public GitHub REST API so you can usually diagnose and report a verdict without asking the user any follow-up questions.
---

# Triage CI Failure

Usually invoked when someone tags Claude in the mono-repo Slack channel about a CI failure. The repo (`starkware-libs/sequencer`) is **public**, so most of the GitHub REST API is reachable without auth. Your goal: diagnose and report a verdict **without asking follow-up questions**. Pull what failed, on which step, with which annotation, on which attempts — then report. Only fall back to "please paste the logs" when the public API genuinely can't get you there; never ask to confirm context you can already fetch.

**Endpoint catalog, pagination, rate limits, and which tools to use in each environment live in `references/github_api.md`.** Read it when you need an exact URL; this file is the decision flow. Substitute `O=starkware-libs`, `R=sequencer` throughout.

## Step 1: Resolve the input, then check it isn't stale

From the message, extract a PR URL, run URL (`/actions/runs/{run_id}`), job URL (`.../job/{job_id}`), check-run id, commit SHA, or branch name. Key fact: **`job.id == check_run.id`** — one numeric id bridges "I have a job link" and "I want its annotations." With only a branch name, list recent failed runs on it before asking the user anything.

**Triage the pasted link — it's usually accurate for the failure they want explained.** Diagnose that run/job even if the user re-ran it afterward (a common flow: paste a link, then re-run assuming it's flaky). As a *complementary* check, note whether the run is stale: this repo uses Graphite stacks, so a run's `head_sha` can lag the PR's current `head.sha` (`GET /pulls/{pr}`). If they differ, also report the current head's status — so a "merge-gatekeeper noise, harmless" verdict on an old SHA doesn't mask a genuinely red live head. Add that as context; don't discard the pasted run in favor of the current head.

## Step 2: Did it already go green on a re-run?

`GET /actions/runs/{run_id}` → check `run_attempt`, `previous_attempt_url`, `conclusion`. If it was re-run (`run_attempt > 1` or `previous_attempt_url` set) **and** the latest `conclusion` is `success`, the workflow already passed on retry — **lead with "the PR isn't blocked."**

But don't wave it away: a fail-then-pass with no code change is a **flaky test**, a real signal worth understanding. So still:
1. Find the flaky job/step — `GET /actions/runs/{run_id}/jobs?filter=all` (the `filter=all` matters; the default hides the failed earlier attempt). Pull that job's annotations.
2. Judge whether it's a known flake (see Step 4's flakiness note).
3. Report a recommendation, e.g. *"Passed on re-run (attempt 2), PR not blocked. Attempt-1 failure was `run-integration-tests` — flaky; worth a tracking issue rather than per-PR re-runs."*

Corollary trap: a pasted *job* link can point at a failed earlier attempt while the run is now green. Reconcile the job's `run_attempt` against the run's current one (via `filter=all`) before calling anything broken.

**Diagnose the sporadic failure either way.** A green-on-latest run doesn't end the triage — if a failure happened (even once, even already re-run away), root-cause it via Steps 3–4. Continue below regardless; the only thing the green status changes is the "is the PR blocked?" answer.

## Step 3: Fast path — PR to root cause in a few calls

1. `GET /pulls/{pr}` → `head.sha`, `base.ref`
2. `GET /commits/{head.sha}/check-runs?filter=all&per_page=100` → every check-run at that SHA
3. Keep `conclusion in ('failure','timed_out')`; **skip `cancelled`/`skipped`/`neutral`** — a `cancelled` job usually means a sibling failed first, so the cause is elsewhere
4. For each failing check, `GET /check-runs/{check_id}/annotations` → the inline error (file + line + text) is usually all you need

**merge-gatekeeper / merge-gatekeeper-new** failing alone is a downstream alarm — something else failed first. Look at sibling check-runs at the same SHA or the previous attempt. Second mode: gatekeeper also fails by **timing out** waiting on a required check that never reached `success` (e.g. a `cancelled` sibling) — then there's *no* failed sibling at this SHA; the real red is usually on a newer SHA, i.e. the pasted run is stale (Step 1).

## Step 4: When annotations aren't enough

Annotations are the primary signal, but for test/build jobs they're often just `"Process completed with exit code 1"`. **Treat a generic exit-code annotation (or an empty one, or null `output.*`) as no signal** — the real assertion/panic is only in the raw step log, which needs auth (`/logs` is 403 unauthenticated; reach it via `gh run view --job {id} --log-failed` when authed — see `references/github_api.md`).

When logs are unreachable, **don't go silent — narrow it from the diff.** Pull `pulls/{pr}/files`; if the failing job is `run-tests` and the PR edits `crates/foo/.../bar_test.rs` or a fixture, report a *scoped hypothesis* ("likely a `foo` test or stale fixture from this rename") plus the one confirming command. That beats both a bare "can't see logs" and a fabricated test name.

**Flakiness check:** to tell flaky from newly-broken, see whether the same job fails in unrelated runs. Note many jobs here (`run-integration-tests`, `run-tests`) run only on `pull_request`, never `push` to `main` — so `branch=main&status=failure` won't show them and you'd wrongly conclude "not a known flake." For those, judge by (a) whether this run went green on re-run (Step 2, strongest signal) and (b) scanning recent failed runs of the same workflow across other PRs. Say which signal you used.

## Step 5: Report — ask only when genuinely blocked

You usually have enough to classify the failure yourself. Report directly; don't tack on reflexive questions — every needless "is this flaky for you?" trains the user to expect noise. Answer these yourself rather than asking:
- **New or flaky?** → flakiness check above.
- **Caused by this PR?** → diff `pulls/{pr}/files` against the failing crate/test path.
- **Known pattern?** → see Common patterns below.

Ask the user *only* when a tool genuinely can't close the gap:
- annotation empty/generic AND `output.text` empty AND no `gh`/MCP raw-log access → ask for a paste;
- the cause hinges on something only they know (e.g. "did your last rebase pick up commit X?") → ask that.

Otherwise don't ask — report and move on.

## Step 6: Classify and report

1. **Code issue in the PR** — name the file/line, propose a fix
2. **Known flaky test** — link prior discussion, suggest re-run
3. **Infrastructure / transient** (network, action-download, GCloud) — suggest re-run, explain why
4. **Pre-existing on the base branch** — call it out; the PR didn't cause it

State what you found, whether action is needed, and the next step.

## Step 7: Fix only if asked

Apply a fix and commit **only** if the user explicitly asks. A triage request isn't an implicit "go patch it." Commit convention: `scope: subject` (no `feat:`/`fix:` prefix), one commit per PR.

## Common patterns in this repo

- `blockifier_reexecution` — transient GCloud network issues; suggest re-run
- `merge-gatekeeper` / `merge-gatekeeper-new` — downstream/timeout failure; find the upstream cause (Step 3)
- Formatting failures — run `scripts/rust_fmt.sh` (pinned nightly), NOT `cargo fmt` directly
- Action-download failures from `codeload.github.com` (404/503) — GitHub-side flake; re-run
