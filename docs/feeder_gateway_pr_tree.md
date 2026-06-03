# Feeder gateway PR tree

The feeder gateway stack is organized as a TREE of Graphite PRs grouped by purpose, so whole
concerns can be dropped, deferred, or landed independently per product decisions. Each
root-to-leaf path compiles and passes tests at every PR (verified per commit on 2026-06-03).

## The tree

```
main
├── DOCS (6 PRs, off main — no code dependency)
│     design docs + implementation plan → progress notes 1 → configuration reference
│     → progress notes 2 → progress notes 3 → progress notes 4 → (this document)
│
└── TRUNK — working apollo_fg (28 PRs)
    │  crates → config skeleton → component → health routes → node wiring
    │  → ChainDataReader + AppState → state-sync storage_reader → ReadExecutor
    │  → colocated reader → remote reader → backend selection by topology
    │  → spaced JSON serializer → legacy error envelope
    │  → get_contract_addresses → get_block_hash_by_id → get_public_key → get_signature
    │  → state-sync signature/number reads → remote get_signature → get_block_id_by_hash
    │  → out-of-range blockId semantics                                ← TRUNK TIP
    │
    ├── METRICS (3): request metric → dashboard row → recording middleware
    │
    ├── PARITY — byte-identical behaviour (17)
    │     felt JSON lock → BlockPostV0_13_1 reorder → builtin IndexMap
    │     → tx type-tag-last → deploy/l1_handler order → invoke order → declare order
    │     → deploy_account order → L1_DATA_GAS key → tx fixture byte locks
    │     → EIP-55 checksum module
    │        ├── receipt wire format + fixture byte locks
    │        └── EIP-55 network-variable contract addresses + live fixtures
    │     and, forking off the felt lock:
    │     MESSAGES: drop sanitization → BLOCK_NOT_FOUND texts → blockHash texts
    │               → legacy blockId/blockNumber parsing quirks
    │
    ├── TESTS (1): end-to-end route smoke test
    │
    └── GROUNDWORK (1): finalized block-status mapping (prep for get_block / tx status)
```

## What each group means for product decisions

- **TRUNK** is a complete working feeder gateway: both read backends, all five currently served
  endpoints with correct parameters, status codes, legacy error-code strings, and behaviors
  (including get_signature's missing/null→latest default... note: that behavior currently rides
  in the MESSAGES leaf, see "known fusions" below). Because the spaced serializer is in the
  trunk, the simple endpoints are byte-identical to the live Python feeder gateway already.
- **METRICS** is droppable: without it the FG serves traffic with no request metric/dashboard.
- **PARITY** is droppable as a whole, or by sub-branch:
  - the tx/receipt wire-format sub-chain matters only for the future tx-carrying endpoints
    (get_block, get_transaction, receipts);
  - the contract-addresses leaf fixes get_contract_addresses' data model AND its EIP-55 casing
    (see known fusions);
  - MESSAGES makes error texts byte-identical to live (Python KeyError/TypeError/json.loads
    echoes, value-bearing not-found messages, int-coercion quirks like `blockId=1.5` serving
    block 1 — all live-verified 2026-06-03).
- **TESTS** boots the real server over real storage and asserts the served routes end to end.
- **GROUNDWORK** is the block-status core needed by get_block/get_transaction_status later.

## Dependency facts that bound the tree shape

These were verified in the diffs; they explain why some things are in the trunk rather than in
a droppable branch (pure branch moves cannot change PR contents):

1. The backend-selection PR introduced `FeederGateway::new(config, reader)` and the `AppState`
   Extension layer. Every endpoint PR compile-depends on it, and it constructs the remote
   backend — so the **remote machinery is trunk-bound**. Extracting it would need content
   splits of ~4 PRs.
2. The legacy-envelope PR defines `fg_json` inside the serializer PR's file — so the **spaced
   serializer is trunk-bound** below the endpoints.

## Known fusions (flagged for future splits if product asks)

- `get_signature` latest-default behavior is fused into the MESSAGES leaf PR ("replicate live
  blockId and blockNumber parsing semantics"). Dropping MESSAGES loses that behavior.
- The contract-addresses leaf fuses the network-variable config model (data correctness:
  mainnet serves 4 L1 contracts, sepolia 8, different orders) with EIP-55 casing (byte
  parity). Dropping it leaves the old 2-felt-field model in the trunk.
- `apollo_starknet_client` as an FG dependency enters in the PARITY branch (tx fixture locks
  PR); the GROUNDWORK PR adds it independently on its own branch.

## Merge-time reconciliation notes

Sibling branches never see each other's changes until they land on main; whichever restacks
second resolves these (all small):

- **receipts leaf vs contract-addresses leaf**: `objects.rs`'s eip55 import must point at
  `apollo_starknet_client::eip55` once both land (the receipts PR moves the eip55 module from
  the FG crate to the client crate; the contract-addresses PR imports it from the FG crate).
- **MESSAGES vs remote reader (trunk)**: `map_client_error` constructs the value-carrying
  `BlockNotFound(block_number)` — already consistent because the error variants carry values.
- **TESTS vs MESSAGES**: the smoke test asserts trunk-era message texts; when MESSAGES lands
  on main, the smoke test's expectations need the live texts (or switch to code-only asserts).
- **TESTS vs PARITY (contract addresses)**: same for the get_contract_addresses body shape.
- **METRICS middleware vs trunk router**: the middleware reorders the route list; new routes
  added to the trunk later must stay above the metrics layer.

## How this tree was built

The original 56-PR linear stack was re-parented in place (branches MOVED, never re-created or
squashed) with `gt move --onto`, ordered so every PR rebased once against its final baseline
(trunk gap-closures bottom-up, then group roots onto the trunk tip, then sub-attachments).
Conflicts during the moves were of four kinds, all expected: sibling-content hunks dropped
(smoke-test edits in PARITY PRs), context-only collisions in `lib.rs`/`Cargo.toml` module and
dependency lists, docs-chain reassembly, and two add/add file recreations (metrics files,
parked subtrees). One dependency had to move between PRs: `apollo_starknet_client` in the FG
crate's `Cargo.toml` (originally introduced by the block-status PR, now a sibling) was amended
into the tx-fixture-locks PR where the PARITY branch first needs it.

Validation: all 56 PRs compile (`cargo check --tests` per commit), all 8 branch tips pass
their crates' test suites, REGEN is no-diff at the config-touching tips.
