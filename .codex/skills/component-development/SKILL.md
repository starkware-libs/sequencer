---
name: component-development
description: Use this skill when adding or modifying an Apollo component, editing `communication.rs`, changing client traits or request/response enums, wiring a component into `apollo_node`, or touching the 3-crate component pattern. Also use it for any task that might cross from component logic into node assembly.
---

# Component Development

<purpose>
Handle component-scoped implementation work without breaking the workspace's transport and wiring conventions.
</purpose>

<context>
- Most Apollo components follow `apollo_X` + `apollo_X_config` + `apollo_X_types`.
- `crates/apollo_X_types/src/communication.rs` defines the typed client trait and transport enums.
- `crates/apollo_node/src/communication.rs`, `clients.rs`, `components.rs`, and `servers.rs` wire local and remote execution.
- `ReactiveComponentExecutionMode` lives in `crates/apollo_node_config/src/component_execution_config.rs`.
</context>

<procedure>
1. Classify the change:
   - Logic-only in one component crate -> stay inside `crates/apollo_X/`.
   - Interface change -> update `apollo_X_types` and all callsites.
   - New wiring or execution-mode change -> update node assembly and topology files together.
2. For interface changes, update in this order:
   - client trait in `communication.rs`
   - request/response enums
   - blanket impl using `handle_all_response_variants!`
   - request handler in the logic crate
   - downstream callers and tests
3. For new components or remote/local behavior changes, inspect:
   - `crates/apollo_node/src/communication.rs`
   - `crates/apollo_node/src/clients.rs`
   - `crates/apollo_node/src/components.rs`
   - `crates/apollo_node/src/servers.rs`
4. If the change affects execution mode or service ownership, load `deployment-topology`.
5. Verify with the smallest relevant crate tests, then broader integration only if the interface or wiring changed.
</procedure>

<patterns>
<do>
- Keep transport contracts in `communication.rs`; keep business logic out of `apollo_node`.
- Preserve `#[cfg(any(feature = "testing", test))]` or `#[cfg_attr(any(test, feature = "testing"), automock)]` patterns for testability.
- Add config changes in the matching `apollo_X_config` crate and implement `SerializeConfig`.
</do>
<dont>
- Don't change `crates/apollo_node/src/*` for a pure logic fix inside one component.
- Don't touch `crates/*_types/src/communication.rs` without a ripple scan across callers, handlers, and tests.
- Don't remove `testing = []` if the crate has metrics or test-only API expansions.
</dont>
</patterns>

<examples>
Example: add a method to an existing component
```rust
#[cfg_attr(any(test, feature = "testing"), automock)]
#[async_trait]
pub trait XClient: Send + Sync {
    async fn new_method(&self, input: NewMethodInput) -> XClientResult<NewMethodOutput>;
}
```
</examples>

<troubleshooting>
| Symptom | Cause | Fix |
|---------|-------|-----|
| `Local client should be available` panic | node wiring missed a local client/server path | update `communication.rs`, `clients.rs`, `components.rs`, and `servers.rs` consistently |
| `check-cfg` error for `testing` | missing feature declaration | add `testing = []` in the crate's `[features]` |
| Remote calls deserialize incorrectly | request/response enum or handler mismatch | rebuild the full transport path from trait to handler |
</troubleshooting>

<references>
- `crates/apollo_node/src/communication.rs`: channel allocation for locally-running components
- `crates/apollo_node/src/clients.rs`: local/remote client creation
- `crates/apollo_node/src/components.rs`: component instantiation
- `crates/apollo_node/src/servers.rs`: local/remote/wrapper server startup
- `crates/apollo_node_config/src/component_execution_config.rs`: execution modes
</references>
