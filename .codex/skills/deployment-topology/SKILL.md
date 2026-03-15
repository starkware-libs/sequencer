---
name: deployment-topology
description: Use this skill for tasks involving execution modes, service layout, `apollo_deployments`, `apollo_node_config`, CDK8s manifests under `deployments/sequencer`, or any change that affects how components are split across processes. It should also trigger when a component moves between consolidated, hybrid, or distributed modes.
---

# Deployment Topology

<purpose>
Keep component ownership, execution mode, and deployment layout consistent across Rust topology code and deployment assets.
</purpose>

<context>
- Reactive execution modes are defined in `crates/apollo_node_config/src/component_execution_config.rs`.
- Topology definitions live in `crates/apollo_deployments/src/deployments/{consolidated,distributed,hybrid}.rs`.
- `deployments/sequencer` is a separate Python 3.10 + Pipenv + CDK8s app; CI installs Node 22 for the CDK8s CLI.
</context>

<procedure>
1. Decide which layer the change belongs to:
   - runtime mode or port contract -> `apollo_node_config`
   - service ownership / topology split -> `apollo_deployments`
   - Kubernetes synthesis / overlays -> `deployments/sequencer`
2. For a component move between services:
   - update execution mode and remote/local client expectations
   - update all affected topology definitions
   - update deployment assets if the service graph changed
3. Keep local/remote expectations aligned with node wiring.
4. Treat deployment-manifest edits as gated even when the Rust topology change is straightforward.
</procedure>

<patterns>
<do>
- Update topology and execution-mode code together.
- Check whether the change affects consolidated, hybrid, and distributed layouts, not just one.
- Preserve the distinction between reactive components and active components.
</do>
<dont>
- Don't move a component across services without checking its remote client/server path.
- Don't edit deployment secrets or environment-specific overlays without approval.
- Don't assume CDK8s synthesis is the source of truth; Rust topology code and deployment assets must agree.
</dont>
</patterns>

<examples>
Example: execution modes
```text
Disabled
Remote
LocalExecutionWithRemoteEnabled
LocalExecutionWithRemoteDisabled
```
</examples>

<troubleshooting>
| Symptom | Cause | Fix |
|---------|-------|-----|
| Remote server expected but missing | execution mode and node wiring disagree | inspect `component_execution_config.rs` and `apollo_node/src/*` together |
| Port allocation mismatch | topology requires more ports than the service layout provides | recheck `InfraPortAllocator` usage and required-port constants |
| Deployment synth drift | Rust topology changed but CDK8s overlays were not updated | review both `apollo_deployments` and `deployments/sequencer` |
</troubleshooting>

<references>
- `crates/apollo_node_config/src/component_execution_config.rs`: execution modes
- `crates/apollo_deployments/src/deployments/consolidated.rs`: single-process layout
- `crates/apollo_deployments/src/deployments/distributed.rs`: 12-service layout
- `crates/apollo_deployments/src/deployments/hybrid.rs`: mixed layout
- `deployments/sequencer/`: CDK8s deployment app
</references>
