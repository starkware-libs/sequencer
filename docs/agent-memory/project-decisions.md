# Project Decisions

Living decision log for repo-wide agent context and workflow constraints.

Use this file before changing:
- repo-level agent instructions
- task-routing rules
- gated/autonomous boundaries
- branch-aware validation workflow

## Decisions

- 2026-03-11: Keep the repo context routing-first because the workspace is too large for default broad loading.
- 2026-03-11: Treat `apollo_node`, topology files, transport contracts, and storage/protocol changes as gated due to high fan-out.
- 2026-03-11: Changed-only validation commands must use the true base SHA, not `HEAD`.
