---
name: debugging
description: Use this skill when diagnosing panics, test failures, regressions, timeouts, CI-only breakages, or unexplained behavior in the sequencer workspace. It should also trigger for cross-component request tracing, storage lock issues, and changed-only validation mistakes.
---

# Debugging

<purpose>
Find the root cause before editing code, especially across component, transport, and topology boundaries.
</purpose>

<context>
- Component calls cross crate boundaries through typed client traits and request/response enums.
- Local/remote transport is selected by execution config, not by different business logic.
- Known flaky areas exist: `blockifier_reexecution` can fail on transient GCloud input fetches; `merge-gatekeeper` is downstream noise when earlier checks fail.
</context>

<procedure>
1. Reproduce with the smallest failing command or test.
2. Verify the diff base and branch context. Many false failures come from wrong changed-only inputs.
3. Trace the path:
   - trait method -> `communication.rs` enum -> handler -> logic method
4. If the issue crosses runtime boundaries, inspect execution mode and node wiring.
5. If the issue touches storage, use per-test temp directories and confirm no lock reuse.
6. Only after a clear hypothesis, patch the code and rerun the minimal reproducer.
</procedure>

<patterns>
<do>
- Use `rg` on trait names, request enums, and error variants.
- Check whether the same logic works locally vs remotely before changing business code.
- Confirm HEAD or the base branch passes before blaming your diff for a long-standing failure.
</do>
<dont>
- Don't guess-and-check across multiple files without a failure model.
- Don't classify a failure as "CI flake" unless it matches a known external dependency issue.
- Don't patch the symptom in `apollo_node` if the actual bug is inside the component crate.
</dont>
</patterns>

<examples>
Example: trace a request path
```text
BatcherClient::start_height
-> apollo_batcher_types/src/communication.rs
-> BatcherRequest::StartHeight
-> ComponentRequestHandler::handle_request()
-> apollo_batcher::batcher::Batcher::start_height(...)
```
</examples>

<troubleshooting>
| Symptom | Cause | Fix |
|---------|-------|-----|
| `Channel closed` / request timeout | missing component server or dropped receiver | inspect `apollo_node` server lifecycle and execution mode |
| Storage open failure in tests | shared MDBX path reused | use `TempDir` and keep it alive for the whole test |
| `merge-gatekeeper` failed | earlier CI job failed | fix the upstream job, not the gatekeeper |
</troubleshooting>

<references>
- `crates/apollo_node/src/servers.rs`: component server lifecycle
- `crates/apollo_node/src/clients.rs`: local vs remote client path
- `crates/apollo_node_config/src/component_execution_config.rs`: runtime mode contract
- `scripts/run_tests.py`: changed-only execution behavior
- `.claude/rules/code-style.md`: repo-specific review and edge-case rules
</references>
