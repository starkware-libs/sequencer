---
name: testing-and-presubmit
description: Use this skill for any code change in the sequencer repo. It tells you which verification command is correct for crate-local work, changed-only checks, integration flows, workspace invariants, and local presubmit. It should also trigger when CI parity or `scripts/run_tests.py` behavior matters.
---

# Testing and Presubmit

<purpose>
Choose the smallest correct verification command and avoid false confidence from wrong changed-only inputs.
</purpose>

<context>
- Main CI uses `scripts/run_tests.py` for changed-only test, clippy, doc, and integration coverage.
- `scripts/run_tests.py --changes_only` must diff against a real base commit. `--commit_id HEAD` is wrong.
- `workspace_tests` enforces workspace manifest, lint, and publish invariants.
- Integration flows live in `crates/apollo_integration_tests` and run as binaries, not only as unit tests.
- `scripts/local_presubmit.sh` creates its own venv and mirrors main CI.
</context>

<procedure>
1. Start with the narrowest scope:
   - one crate change -> `SEED=0 cargo test -p <crate>`
   - manifest or workspace metadata change -> also run `cargo test -p workspace_tests`
   - interface, wiring, or multi-crate change -> use `scripts/run_tests.py` with the true base SHA
2. Compute the base SHA when needed:
   - `git merge-base HEAD origin/<base_branch>`
3. Use changed-only runners only with that base SHA:
   - test: `SEED=0 python3 scripts/run_tests.py --command test --changes_only --include_dependencies --commit_id <base_sha>`
   - clippy: `python3 scripts/run_tests.py --command clippy --changes_only --commit_id <base_sha>`
   - doc: `python3 scripts/run_tests.py --command doc --changes_only --commit_id <base_sha>`
   - integration: `SEED=0 python3 scripts/run_tests.py --command integration --changes_only --include_dependencies --commit_id <base_sha>`
4. For broad or release-branch work, run `scripts/local_presubmit.sh [--parent_branch <base_branch>]`.
5. Finish with formatting and any domain-specific checks.
</procedure>

<patterns>
<do>
- Prefix Rust tests with `SEED=0`.
- Use `tempfile::TempDir` for storage tests.
- Run `cargo test -p workspace_tests` whenever `Cargo.toml`, `Cargo.lock`, publish metadata, or workspace member definitions change.
</do>
<dont>
- Don't use `--commit_id HEAD` for changed-only commands.
- Don't hide failing tests with `#[ignore]`.
- Don't skip integration flows when the change crosses `communication.rs`, `apollo_node`, topology files, or protobuf contracts.
</dont>
</patterns>

<examples>
Example: release-branch local presubmit
```bash
scripts/local_presubmit.sh --parent_branch main-v0.14.2
```
</examples>

<troubleshooting>
| Symptom | Cause | Fix |
|---------|-------|-----|
| `No changes detected.` | wrong commit diff base | use PR base SHA or `git merge-base` output |
| Integration binary missing | integration flow was not built first | rerun via `scripts/run_tests.py --command integration ...` |
| Local script fails on Python import | missing helper deps | `python3 -m pip install -r scripts/requirements.txt` or use a venv |
</troubleshooting>

<references>
- `scripts/run_tests.py`: changed-only command builder
- `scripts/tests_utils.py`: package diff logic
- `scripts/local_presubmit.sh`: local CI parity flow
- `.github/workflows/main.yml`: authoritative CI job matrix
- `workspace_tests/`: workspace invariant checks
</references>
