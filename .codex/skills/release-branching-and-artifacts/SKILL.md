---
name: release-branching-and-artifacts
description: Use this skill for release-branch work, backports, branch promotion, artifact uploads, Docker publish workflows, benchmark publication, or any task touching `.github`, `scripts/merge_branches.py`, `scripts/merge_paths.json`, or branch-specific presubmit behavior. It should also trigger when private sibling repos are part of the real change but missing from the local checkout.
---

# Release, Branching, and Artifacts

<purpose>
Keep release-aware work aligned with the repo's multi-branch merge chain and artifact pipelines.
</purpose>

<context>
- Active remote branches include `main` and multiple `main-v*` release branches.
- The promotion chain is encoded in `scripts/merge_paths.json`.
- `scripts/local_presubmit.sh` defaults to `scripts/parent_branch.txt` (`main`) unless `--parent_branch` is passed.
- Artifact and publish workflows live in `.github/workflows/` and use GCS, GHCR, GitHub Pages, or GitHub CLI.
- [human][verify] Related private repos exist but are not present in this checkout: `starkware-industries/starkware` and `starkware-industries/starkware-envs-production`.
</context>

<procedure>
1. Confirm the base branch before editing anything.
2. For release-branch local validation, always pass the explicit parent branch:
   - `scripts/local_presubmit.sh --parent_branch <main-vX.Y.Z>`
3. Use `scripts/merge_paths.json` to understand promotion order.
4. Run `scripts/merge_branches.py` only with approval; it creates branches, merges, and PRs.
5. Treat `.github`, artifact upload logic, Docker publish, and benchmark publication as gated work.
6. If the real task spans the private sibling repos, stop and surface that dependency clearly.
</procedure>

<patterns>
<do>
- State the target branch and promotion path in task summaries.
- Use repo scripts for branch-promotion logic instead of inventing a manual flow.
- Keep artifact, benchmark, and publish edits scoped and reviewed.
</do>
<dont>
- Don't assume `main` is the only base branch.
- Don't run merge or publish scripts casually.
- Don't touch secrets, GCS paths, or GHCR publish logic without approval.
</dont>
</patterns>

<examples>
Example: promotion chain anchors
```text
main-v0.13.2 -> main-v0.13.4 -> main-v0.13.5 -> main-v0.13.6 -> main-v0.14.0 -> main-v0.14.1 -> main-v0.14.1-committer -> main-v0.14.2 -> main
```
</examples>

<troubleshooting>
| Symptom | Cause | Fix |
|---------|-------|-----|
| Local presubmit targets the wrong diff base | default parent branch is still `main` | pass `--parent_branch <actual base branch>` |
| `gh auth status` or `gh --version` failure | merge script prerequisites missing | authenticate/install before using merge automation |
| Artifact or benchmark workflow needs external auth | GCS/GitHub credentials are not available locally | stop and ask for supervised handling |
</troubleshooting>

<references>
- `scripts/merge_paths.json`: branch promotion chain
- `scripts/merge_branches.py`: automated merge-and-PR flow
- `scripts/local_presubmit.sh`: base-branch-aware local validation
- `.github/workflows/committer_ci.yml`: benchmark publishing and branch-aware workflow
- `.github/workflows/upload_artifacts_workflow.yml`: native blockifier artifact upload flow
</references>
