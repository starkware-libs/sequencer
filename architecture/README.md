# Apollo Sequencer — Architecture Documentation

This folder is a living guide to the Apollo sequencer codebase. It is structured to take you from high-level concepts down to implementation details, one step at a time.

## Reading Order

| File | What you will learn |
|------|---------------------|
| [01-overview.md](01-overview.md) | What the sequencer does, all components at a glance, the three data flows |
| [02-component-model.md](02-component-model.md) | How components communicate (the infra pattern), execution modes, server types |
| [03-components.md](03-components.md) | What each component is responsible for |
| [04-data-flows.md](04-data-flows.md) | Step-by-step walkthroughs of key flows (tx submission, block production, finalization) |

## Deep Dives

| File | What you will learn |
|------|---------------------|
| [deep-dives/05-transaction-lifecycle.md](deep-dives/05-transaction-lifecycle.md) | Full transaction lifecycle: HTTP ingestion → Gateway validation → Mempool → Blockifier execution → commit |
| [deep-dives/06-block-production.md](deep-dives/06-block-production.md) | Block production: Tendermint rounds, proposer/validator roles, timeouts, sync fallback |
| [deep-dives/07-integration-tests.md](deep-dives/07-integration-tests.md) | Integration test infrastructure: FlowTestSetup, IntegrationTestManager, deployment configs, node/resource provisioning |

## Ground Rules

- Start with [01-overview.md](01-overview.md) and answer the check questions before moving on.
- Each file ends with comprehension questions. Answer them before asking to go deeper.
- When you want to dive deeper into a specific component or flow, say so and new detail pages will be added under `architecture/deep-dives/`.
