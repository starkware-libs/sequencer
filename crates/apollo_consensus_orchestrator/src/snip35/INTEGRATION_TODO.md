# SNIP-35 — centralized integration tasks

> **Status:** Working checklist. Capture of the plan to wire `fee_proposal_fri` from the centralized Python feeder gateway into the Rust sequencer.
>
> **Verified against:** `starkware/` synced as of 2026-05-05; `sequencer/` Rust side as of the SNIP-35 stack head. All file paths and line numbers in this document have been re-checked against the actual code.

## Verification status (re-checked against the code)

| Claim | Status |
|---|---|
| `FeeMarketInfo` exists at `shared_objects.py:757` with the expected shape | ✓ |
| `BatchCreated.get_fee_market_info` exists at `shared_objects.py:453` and follows the `additional_object_keys` pattern | ✓ |
| `FEE_MARKET_INFO_KEY = "fee_market_info"` at `shared_objects.py:60` | ✓ |
| `StarknetBlock` at `response_objects.py:919-1101`, with `next_l2_gas_price` at line 987 and `dump_without_fee_market_info` factored to `remove_fee_market_info_from_block_json` at line 1097 | ✓ |
| Two `StarknetBlock.create()` callsites: `feeder_gateway_impl.py:1315` and `sequencer_internal_api_utils.py:521` | ✓ |
| `feeder_gateway.py:109, 392, 398, 415` for the toggle wiring locations | ✓ |
| `BlockHashHeader.new()` at `block_hash_calculator/shared_objects.py:332-364` does NOT include `fee_proposal_fri` (hash invariant safe) | ✓ |
| Rust `FeeMarketInfo` at `apollo_consensus_orchestrator/src/fee_market/mod.rs:29` | ✓ |
| Rust `CentralFeeMarketInfo` type alias at `cende/central_objects.rs:88` | ✓ |
| Rust `BlobParameters.fee_market_info` at `cende/mod.rs:364`, `AerospikeBlob.fee_market_info` at line 82 | ✓ |
| Rust `FeeMarketInfo` construction site at `sequencer_consensus_context.rs:548` | ✓ |
| `gas_price_metadata_default_1` exists in `fields.py:593` — but defaults to `1`, not `None` | ⚠ corrected: PR 1 needs a NEW metadata helper with `load_default=None` |
| `additional_object_keys` plumbing in `cende_recorder.py:1004` requires adding `SNIP35_INFO_KEY` | ⚠ added: PR 2 |
| `applicative_backup.py:259-269` enumerates every known key and asserts `len(additional_object_keys) == 0` | ⚠ added: PR 2 must update this consumer too, otherwise it crashes |
| Cende `Blob` dataclass at `cende_recorder.py:108-141` (not `:82` as initially noted) | ⚠ corrected line number |
| `_write_fee_market_info` at `cende_recorder.py:703` (not `:338`) and writer driver at line 446 (not `:215`) | ⚠ corrected line numbers |
| `Blob.proposal_commitment: Optional[int]` already exists at `cende_recorder.py:139` — top-level on the blob, mirroring how a single value can be plumbed without its own IndexedDBObject | ✓ informational; we still recommend the IndexedDBObject pattern for `Snip35Info` since it's symmetric with `FeeMarketInfo`. |

## Context

The Rust SNIP-35 stack already adds `fee_proposal_fri: Option<GasPrice>` everywhere it needs to live (block header, storage, sync proto, consensus proto, sliding window). Today the centralized feeder gateway doesn't emit this field, so the Rust client at `apollo_starknet_client/src/reader/objects/block.rs:345` hardcodes `fee_proposal_fri: None` on the conversion from gateway JSON to `BlockHeaderWithoutHash`.

To close the loop, the centralized Python feeder gateway needs to start emitting `fee_proposal_fri` in its block JSON, and the Rust client needs to start reading it.

**Hash invariant:** `fee_proposal_fri` must NOT enter `BlockHash` or `PartialBlockHash`. It only enters `ProposalCommitment` (the consensus-voted hash) via `proposal_commitment_from`. On the centralized side, this means the field must NOT be propagated into `BlockHashHeader`.

**Backward-compat invariant:** existing requests to the feeder gateway must continue to receive the exact same JSON shape they receive today. New SNIP-35 data is gated behind a new opt-in toggle `withSnip35Info`. Existing callers see no change unless they explicitly opt in.

**Architectural decision: independence.** SNIP-35 data is stored, served, and toggled **independently** of `FeeMarketInfo`. They're related (SNIP-35 uses `next_l2_gas_price` as a floor, etc.) but they're produced by different mechanisms (EIP-1559 measurement vs. proposer declaration) and consumed by different flows. Keeping them separate avoids touching `FeeMarketInfo`'s production schema and keeps strip-logic clean (two independent toggles, no chained logic). A new `Snip35Info` dataclass holds `fee_proposal_fri`; `FeeMarketInfo` is left untouched.

| Query params | Response includes |
|---|---|
| (default — no flags) | none of the new data (existing default, unchanged) |
| `withFeeMarketInfo=true` | `l2_gas_consumed`, `next_l2_gas_price` (existing behavior, unchanged) |
| `withSnip35Info=true` | `fee_proposal_fri` |
| both `=true` | all three |

## Where the value comes from

`fee_proposal_fri` is *stated* by the proposer at the moment of proposing a block — not independently derived from chain state. The SNIP-35 clamp formula (`compute_fee_proposal` in `crates/apollo_consensus_orchestrator/src/snip35/mod.rs`) runs only in the active proposer.

The proposer is **always an Apollo (Rust consensus) node** — round-robin selected. The centralized Python is never the proposer; it's record-only. So the centralized Python side **does not compute anything**. It just receives the stated value through the cende-blob pipeline and serves it via the feeder gateway.

End-to-end data flow:

```
Apollo (Rust): compute_fee_proposal → states fee_proposal_fri in ProposalInit
            └─ also writes Snip35Info { fee_proposal_fri } into the cende blob
                ↓
[centralized storage]
                ↓
CENDE recorder (Python): reads blob, persists Snip35Info to its own storage
                ↓
Feeder gateway (Python): on withSnip35Info=true, includes fee_proposal_fri in JSON
                ↓
apollo_starknet_client (Rust consumer): reads JSON into BlockHeaderWithoutHash
```

Independence means there are now two parallel data structures (`FeeMarketInfo`, `Snip35Info`) with parallel writers, parallel storage, and parallel response handling. Not more complex per-thing — just symmetric copies of the existing fee-market plumbing for the new data.

## The two repos

### 1. `starkware/` — centralized Python monorepo

Path: `/home/andrew/workspace/starkware/`. Files that need to change:

| Path | What |
|---|---|
| `src/starkware/starknet/services/batcher/shared_objects.py` (near line 757) | New `Snip35Info` dataclass alongside `FeeMarketInfo`. New `IndexedDBObject` with its own storage namespace. `FeeMarketInfo` itself is **unchanged**. |
| `src/starkware/starknet/services/batcher/shared_objects.py` (near line 361) | New `BatchCreated.get_snip35_info(...)` method mirroring the existing `get_fee_market_info`. |
| `src/starkware/starknet/services/api/feeder_gateway/response_objects.py` (near line 988) | Add `fee_proposal_fri: Optional[int]` field to `StarknetBlock` (top-level, parallel to `next_l2_gas_price`). Thread through `StarknetBlock.create()`. |
| `src/starkware/starknet/services/api/feeder_gateway/response_objects.py` (new function) | New `remove_snip35_info_from_block_json(block_json)` that pops `fee_proposal_fri`. The existing `remove_fee_market_info_from_block_json` is **not modified**. |
| `src/starkware/starknet/services/feeder_gateway/feeder_gateway.py` | Add `WITH_SNIP35_INFO = "withSnip35Info"` constant + allowlist + parse + apply strip. Two independent toggles (no chained `elif`). |
| `src/starkware/starknet/services/feeder_gateway/feeder_gateway_impl.py:1315` | Pending block path — also read `Snip35Info` and pass `fee_proposal_fri` to `StarknetBlock.create()`. |
| `src/starkware/starknet/services/utils/sequencer_internal_api_utils.py:521` | Same plumbing for the non-pending path. |
| `src/starkware/starknet/services/cende/cende_recorder.py` | Update the `Blob` dataclass to include `snip35_info`. Add `_write_snip35_info` mirror of `_write_fee_market_info`. Wire into the writer driver. |
| `src/starkware/starknet/services/block_hash_calculator/shared_objects.py:332-364` | `BlockHashHeader` and its `new()` — **do NOT touch**. Block hash must remain unaffected by SNIP-35. |

### 2. `sequencer/` — Rust repo (this one)

Path: `/home/andrew/workspace/sequencer/`. Two independent edits:

**Outbound (write side, Apollo → cende blob):**

| Path | What |
|---|---|
| `crates/apollo_consensus_orchestrator/src/snip35/mod.rs` (or sibling module) | Define `Snip35Info { fee_proposal_fri: Option<GasPrice> }`. |
| `crates/apollo_consensus_orchestrator/src/cende/mod.rs:364` | Add `snip35_info: Snip35Info` to `BlobParameters`. |
| `crates/apollo_consensus_orchestrator/src/cende/central_objects.rs:88` | Add `CentralSnip35Info` type alias. |
| `crates/apollo_consensus_orchestrator/src/sequencer_consensus_context.rs:548` | Construct `Snip35Info` from `init.fee_proposal_fri` and pass it via `BlobParameters`. |

**Inbound (read side, feeder gateway JSON → Rust types):**

| Path | What |
|---|---|
| `crates/apollo_starknet_client/src/reader/objects/block.rs` | Add `fee_proposal_fri: Option<GasPrice>` to `BlockPostV0_13_1` (~line 89), accessor on `Block` enum (~line 200), change converter at line 345. |
| `crates/apollo_starknet_client/src/reader/mod.rs:157, 239` | Add `SNIP35_INFO_QUERY` constant; append `withSnip35Info=true` to the get_block URL. |
| `crates/apollo_starknet_client/resources/reader/block_post_0_14_3.json` | New JSON test fixture with the field populated. |
| `crates/apollo_starknet_client/src/reader/objects/block_test.rs` | New tests + register fixture. |
| `crates/apollo_starknet_client/tests/feeder_gateway_integration_test.rs` | New live integration test, gated until Python deploys. |

## Task list (in dependency order)

### Prerequisite

- [ ] **Task #1 — Confirm scope with manager.** Validate the design (independent `Snip35Info`, opt-in toggle, no Python-side computation).

### Phase A — Python (`starkware/`)

- [ ] **Task #2 — Add new `Snip35Info` dataclass.** New `IndexedDBObject` in `shared_objects.py`. Don't touch `FeeMarketInfo`. Also add `BatchCreated.get_snip35_info(...)` as a sibling of `get_fee_market_info`.

- [ ] **Task #3 — Confirm centralized side is record-only.** No SNIP-35 computation in Python; just plumbing. Verify with manager that no Python-as-proposer mode exists for V0_14_3+ blocks.

- [ ] **Task #4 — Add `fee_proposal_fri` to `StarknetBlock` response object** (top-level field, parallel to `next_l2_gas_price`). Thread through `StarknetBlock.create()`. Add `remove_snip35_info_from_block_json` helper. **Do NOT touch** `remove_fee_market_info_from_block_json`.

- [ ] **Task #15 — Add `withSnip35Info` query toggle in `feeder_gateway.py`.** Independent of `withFeeMarketInfo`. Two parallel strip operations, no chained logic. Backward-compat: existing `withFeeMarketInfo=true` callers see no change in JSON shape.

- [ ] **Task #5 — Update two `StarknetBlock.create()` call sites** (`feeder_gateway_impl.py:1315`, `sequencer_internal_api_utils.py:521`) to also read `Snip35Info` and pass `fee_proposal_fri`. Do NOT change `BlockHashHeader.new()`.

- [ ] **Task #16 — Add CENDE recorder writer for `Snip35Info` blobs.** Update `Blob` dataclass, add `_write_snip35_info`, wire into the writer driver. Backward-compat: tolerate missing `snip35_info` in old blobs.

- [ ] **Task #6 — Update Python tests.** Tests for the new `Snip35Info`, the new toggle, and CENDE recorder writes. `FeeMarketInfo` tests are unchanged.

- [ ] **Task #7 — Land Python PR and deploy to staging feeder gateway.** Coordinate with the deployment-owning team.

### Phase B — Rust (`sequencer/`) — runs in parallel with Phase A

- [ ] **Task #14 — Add `Snip35Info` struct (independent of `FeeMarketInfo`) and wire into cende blob.** Outbound write side. Define `Snip35Info { fee_proposal_fri }`, add to `BlobParameters` alongside the existing `fee_market_info`, populate at `sequencer_consensus_context.rs:548`. Add a serialization round-trip test.

- [ ] **Task #8 — Insert PR: `apollo_starknet_client: read fee_proposal_fri from feeder gateway`.** Inbound read side. Adds the field to `BlockPostV0_13_1`, the accessor, the converter call, and `withSnip35Info=true` to the request URL. **Blocked by Task #7** — cannot be deployed until Python knows about the new toggle.

- [ ] **Task #9 — Add JSON test fixture `block_post_0_14_3.json`.**

- [ ] **Task #10 — Add unit/conversion/round-trip tests.**

- [ ] **Task #11 — Add live integration test, marked `#[ignore]`.** Remove the ignore once Task #7 is on staging.

### Phase C — coordination

- [ ] **Task 12 — Investigate `pending_data.rs:214` TODO.** Pending blocks don't currently include fee market info. If proposers need `fee_proposal_fri` for blocks built on top of pending state, this needs the same treatment.

- [ ] **Task #13 — Verify cross-repo wire compatibility and roll out.** Confirm Python emits the new field, Rust reads it, backward compat holds (old gateway responses still parse), `BlockHash` is unchanged.

## When to do what

```
   ┌─ Task #1 — Confirm scope ─┐
   │                           │
   ↓                           ↓
[Python track]              [Rust track]
   │                           │
   ├─ Task #2 (Snip35Info)     ├─ Task #14 (outbound: cende blob)
   ├─ Task #4 (response)       ├─ Task #8 (inbound: starknet_client) ← blocked by Task #7
   ├─ Task #15 (toggle)        ├─ Tasks #9, #10
   ├─ Task #5 (callsites)      ├─ Task #11 (gated integration test)
   ├─ Task #16 (CENDE)         └─ Task #12 (followup)
   ├─ Task #6 (tests)
   ├─ Task #7 (deploy staging)
   │                           │
   └────── Task #13 (verify integration) ──────┘
```

The two tracks are mostly independent. The exception: Task #8 (Rust inbound) cannot deploy against any environment until Task #7 (Python) has reached that environment, because sending `withSnip35Info=true` to a Python that doesn't allowlist the flag causes the request to be rejected.

## What about the existing Rust stack

**Stack stays as-is.** No PR in the current stack needs to be amended for this work.

- PR 1 (`starknet_api: add fee_proposal_fri to BlockHeaderWithoutHash`) and everything above continues unchanged. The hardcoded `fee_proposal_fri: None` at `block.rs:345` is correct *until* the new PR (Task #8) replaces it with the real accessor.
- Task #8 inserts above PR 1 in the stack, separating "introduce the type field" from "wire the source."
- Task #14 is a separate Rust PR (write side) that can land anywhere in the stack — it's independent of the inbound read path.
- Until Python deploys, the field defaults to `None` from gateway responses (because the JSON doesn't carry it and `#[serde(default)]` resolves to `None`). This means **the entire SNIP-35 stack can land in this Rust repo on its own schedule.**

## Centralized (`starkware/`) PR stack

The Python work splits cleanly into a stack of **4 PRs**, each building on the previous, each independently deployable, each preserving the backward-compat invariant ("existing requests get the same JSON shape they get today").

### Why this ordering matters

The load-bearing constraint is: **the response shape for `withFeeMarketInfo=true` (without `withSnip35Info`) must remain byte-identical throughout the rollout.** Any PR that exposes `fee_proposal_fri` in the response must land *after* the toggle that hides it from default callers.

```
PR 1 (foundation)
  ↓
PR 2 (CENDE storage writer) ─── independent of PRs 3 & 4
  ↓
PR 3 (toggle scaffolding) ─── must land before PR 4
  ↓
PR 4 (response field + populate) ─── completes the feature
```

Each PR's section below has: **what changes**, **why this PR is safe to land alone**, and **what's still missing after it lands**.

### Prerequisites (non-coding)

- **Task #1** — Confirm SNIP-35 scope with manager (port vs. stub vs. record-only). Resolves whether the centralized side computes anything; expected resolution per current understanding: **record-only**, no computation.
- **Task #3** — Confirm centralized Python is never the proposer for V0_14_3+ blocks. Apollo round-robin proposers are the source of truth.

These should happen before opening PR 1.

---

### PR 1 — Foundation: define `Snip35Info` dataclass and storage accessor

**Goal:** introduce the new dataclass type without changing any runtime behavior. Pure type addition.

**Files:**

| File | Change |
|---|---|
| `src/starkware/starknet/definitions/fields.py` (near line 593, alongside `gas_price_metadata` / `gas_price_metadata_default_1`) | **Define a new metadata helper:** `optional_fee_proposal_fri_metadata = GasPriceField.metadata(required=False, load_default=None)`. We can't reuse `gas_price_metadata_default_1` because it defaults to `1`, not `None` — and we need missing-from-blob to mean "the proposer didn't state a value," which must be `None`, not `1`. |
| `src/starkware/starknet/services/batcher/shared_objects.py` (near `FEE_MARKET_INFO_KEY = "fee_market_info"` at line 60) | Add a new key constant: `SNIP35_INFO_KEY = "snip35_info"`. |
| `src/starkware/starknet/services/batcher/shared_objects.py` (near line 757, alongside `FeeMarketInfo`) | Define `Snip35Info` as a new `marshmallow_dataclass.dataclass` subclassing `ValidatedMarshmallowDataclass` and `IndexedDBObject`. Single field `fee_proposal_fri: Optional[int]` using the new `optional_fee_proposal_fri_metadata`. Add `default()` classmethod returning `Snip35Info(fee_proposal_fri=None)`. Add `set_raw_obj` classmethod mirroring `FeeMarketInfo`. |
| `src/starkware/starknet/services/batcher/shared_objects.py` (near line 453, alongside `BatchCreated.get_fee_market_info`) | Add `BatchCreated.get_snip35_info(self, storage: Storage) -> "Snip35Info"`. Mirror `get_fee_market_info`'s pattern: look up `SNIP35_INFO_KEY` from `additional_object_keys`, return `Snip35Info.default()` if missing, else `Snip35Info.get_obj_or_fail(storage=storage, index=key)`. |
| Tests near the existing `FeeMarketInfo` tests | Tests for: `Snip35Info.default()` returns `fee_proposal_fri=None`; Marshmallow schema round-trip (dump/load); `IndexedDBObject` set/get against a fake storage; `get_snip35_info` returns default when `SNIP35_INFO_KEY` is missing from `additional_object_keys`. |

**Why this PR is safe alone:**
- Adds a new type. Doesn't touch any existing code paths.
- No existing storage blob format changes — `Snip35Info` is a new namespace.
- No response object changes.
- No CENDE writer wiring yet, so the type is unused in production.

**What's still missing after this PR:**
- Nothing writes `Snip35Info` to storage (PR 2).
- Nothing reads it into responses (PR 4).
- The `withSnip35Info` toggle doesn't exist (PR 3).

---

### PR 2 — CENDE recorder writes `Snip35Info` to storage

**Goal:** start persisting `Snip35Info` blobs to storage. From this PR onward, when Apollo eventually starts emitting `snip35_info` in its cende blobs (Rust Task #14), CENDE captures it.

**Files:**

| File | Change |
|---|---|
| `src/starkware/starknet/services/cende/cende_recorder.py` (line 108, the `Blob` dataclass) | Add `snip35_info: Optional[str] = None` (default `None` for backward-compat with old Apollo blobs that don't carry the field). |
| Same file (line 144, `Blob.from_dict`) | Conditionally extract `snip35_info=json.dumps(data["snip35_info"]) if "snip35_info" in data else None`. Tolerate missing key. |
| Same file (line 446, the `asyncio.gather` block in the writer driver) | After the existing `self._write_fee_market_info(...)` line, conditionally call `self._write_snip35_info(batch_id=next_batch_info.batch_id, snip35_info=blob.snip35_info)` only when `blob.snip35_info is not None`. |
| Same file (after line 703, mirroring `_write_fee_market_info`) | Add `async def _write_snip35_info(self, batch_id: int, snip35_info: str)` that writes to `Snip35Info.db_key(suffix=str(batch_id).encode("ascii"))`. |
| Same file (line 1004, `additional_object_keys` dict construction in `_create_batch_created`) | When the snip35_info write succeeded, add `SNIP35_INFO_KEY: batch_id` to the dict (mirror the `if compressed_state_diff_written` pattern at line 1010). When it didn't (old Apollo blob), do NOT add the key — that way `BatchCreated.get_snip35_info()` correctly returns the default. |
| `src/starkware/starknet/services/applicative_backup/applicative_backup.py` (line 259, the pop block) | After the existing `fee_market_index = additional_object_keys.pop(FEE_MARKET_INFO_KEY, None)` line, add `snip35_info_index = additional_object_keys.pop(SNIP35_INFO_KEY, None)`. **Required** because the assertion at line 267 (`assert len(additional_object_keys) == 0`) explicitly enumerates every known key and rejects unexpected ones. Without this update, applicative_backup crashes on any batch that has `SNIP35_INFO_KEY` in its `additional_object_keys`. Then plumb the index through to wherever the other indices are used downstream (see lines 272–277). |
| `src/starkware/starknet/services/applicative_backup/conftest.py` (line 457) and `applicative_backup/test_utils.py` (line 211) | Mirror updates: add `SNIP35_INFO_KEY` to the test-fixture `additional_object_keys` dict and the corresponding pop call. |
| Tests | Test that the writer correctly persists a blob with `snip35_info`. Test backward-compat: a blob *without* `snip35_info` still loads and writes everything else without crashing. Test that `applicative_backup` handles a batch with `SNIP35_INFO_KEY` (no assertion crash). |

**Why this PR is safe alone:**
- Backward-compat with old Apollo blobs: missing `snip35_info` = no write, no error.
- Nothing reads `Snip35Info` from storage yet (PR 4 does), so even if writes succeed, behavior is unobservable.
- No response shape changes.
- No new query parameters.

**What's still missing after this PR:**
- Apollo isn't yet writing `snip35_info` to its cende blobs — that's Rust Task #14. Until that lands, this writer is a no-op (because every blob has `snip35_info=None`).
- Once Apollo's update ships AND a block is committed AND CENDE writes the blob → `Snip35Info` will start appearing in storage. But it still doesn't show up in responses until PR 4.

---

### PR 3 — Add `withSnip35Info` query toggle and strip helper

**Goal:** introduce the opt-in toggle and the strip helper *before* the field is exposed, so backward compat is preserved when PR 4 lands. Until PR 4, the toggle is a no-op (the field doesn't exist in the response yet).

**Files:**

| File | Change |
|---|---|
| `src/starkware/starknet/services/api/feeder_gateway/response_objects.py` (after `remove_fee_market_info_from_block_json` at line 1097) | New helper: `def remove_snip35_info_from_block_json(block_json: dict) -> dict:`. Body: `block_json = block_json.copy(); block_json.pop("fee_proposal_fri", None); return block_json`. Use `pop` *with default* (`None`) so it's a safe no-op when the key isn't present yet. |
| `src/starkware/starknet/services/feeder_gateway/feeder_gateway.py` (near line 109, alongside `WITH_FEE_MARKET_INFO`) | Add `WITH_SNIP35_INFO = "withSnip35Info"` constant. |
| Same file (line 392, `validate_request_field_names`) | Extend the allowlist: `field_names=self.BLOCK_IDENTIFIERS \| {self.HEADER_ONLY, self.WITH_FEE_MARKET_INFO, self.WITH_SNIP35_INFO}`. Without this, requests carrying the new param get rejected with "unknown field." |
| Same file (around line 398, alongside `with_fee_market_info` parsing) | Parse the flag: `with_snip35_info = self._parse_flag(request=request, flag_name=self.WITH_SNIP35_INFO, default=False)`. |
| Same file (around line 415, after the existing `if not with_fee_market_info:` strip) | Add a parallel strip: `if not with_snip35_info: block_json = remove_snip35_info_from_block_json(block_json=block_json)`. Two **independent** strip operations (no chained `elif`) — each toggle controls its own data. |
| Same file (lines 366–368, docstring) | Document the new `withSnip35Info` flag. |
| Tests | Test that `withSnip35Info=true` is allowlisted and parses correctly. Test that the strip helper is a safe no-op when the response doesn't contain `fee_proposal_fri`. Test that the response shape without the toggle is unchanged from current behavior. |

**Why this PR is safe alone:**
- The strip helper uses `pop("fee_proposal_fri", None)` — won't crash when the key is absent (which it is, until PR 4).
- The toggle exists but has no observable effect on response shape: the strip is a no-op, and the field isn't in the response yet.
- Existing requests (no toggle) behave identically to today.
- Existing requests with `withFeeMarketInfo=true` behave identically to today.
- Even requests with `withSnip35Info=true` produce the same response as today, because the field doesn't exist yet.

**What's still missing after this PR:**
- The response object has no `fee_proposal_fri` field (PR 4 adds it).
- The two `StarknetBlock.create()` callsites don't pass `fee_proposal_fri` (PR 4 updates them).

---

### PR 4 — Expose `fee_proposal_fri` in `StarknetBlock` response

**Goal:** complete the feature. Add the response field, update `StarknetBlock.create()`, update both callsites to read `Snip35Info` from storage and pass the value through. The toggle from PR 3 starts gating the field naturally.

**Files:**

| File | Change |
|---|---|
| `src/starkware/starknet/services/api/feeder_gateway/response_objects.py` (around line 988, after `next_l2_gas_price`) | Add `fee_proposal_fri: Optional[int] = field(metadata=fields.optional_fee_proposal_fri_metadata)` (the new metadata helper from PR 1, which uses `load_default=None`). Top-level field on `StarknetBlock`, parallel in shape to `next_l2_gas_price`. **Do NOT use `gas_price_metadata_default_1`** — that helper has `load_default=1`, which would cause missing/old-block values to surface as `1` instead of `None`. |
| Same file (around lines 996–1049, `StarknetBlock.create()`) | Add `fee_proposal_fri: Optional[int]` to the params and pass it through to the constructor. Required (typed `Optional`), matching the `next_l2_gas_price` pattern — both callsites must be updated atomically. |
| `src/starkware/starknet/services/feeder_gateway/feeder_gateway_impl.py` (line 1315, pending block path) | Add `snip35_info = await batch.get_snip35_info(storage=self.storage)` near the existing `fee_market_info = await batch.get_fee_market_info(...)` (line 1314). Pass `fee_proposal_fri=snip35_info.fee_proposal_fri` to `StarknetBlock.create()`. |
| `src/starkware/starknet/services/utils/sequencer_internal_api_utils.py` (line 521, non-pending path) | Same: read `Snip35Info` from storage, pass `fee_proposal_fri` to `StarknetBlock.create()`. |
| Tests | Test that `StarknetBlock` includes `fee_proposal_fri` in the schema. Test that `withSnip35Info=true` requests get the field populated when storage has data. Test that `withSnip35Info=true` requests get `fee_proposal_fri=None` (or omitted) when storage is empty (old block). Test that `withSnip35Info=false` (default) requests have no `fee_proposal_fri` in the JSON. **Backward-compat verification:** `withFeeMarketInfo=true` (without snip35) returns byte-identical JSON to the pre-PR shape. |

**Crucial: do NOT touch:**
- `block_hash_calculator/shared_objects.py:332-364` (`BlockHashHeader.new()`) — the block hash struct must NOT learn about `fee_proposal_fri`. The `FeeMarketInfo` parameter passed there is consumed but only `l2_gas_consumed` and `next_l2_gas_price` are propagated. Don't add `Snip35Info` as a parameter to that path.

**Why this PR is safe alone:**
- The toggle from PR 3 is in place: requests without `withSnip35Info=true` have the field stripped by `remove_snip35_info_from_block_json`. Existing callers see no shape change.
- For old blocks (no `Snip35Info` in storage), `get_snip35_info` returns `Snip35Info.default()` → `fee_proposal_fri=None` → either serializes as `null` or is omitted depending on the Marshmallow metadata. Either way, `serde(default)` on the Rust client resolves to `Option::None`.
- For new blocks (with Apollo's Rust Task #14 deployed and `Snip35Info` in storage), the actual proposer-stated value flows through.
- `BlockHash` is unchanged because `BlockHashHeader` is untouched.

**What's still missing after this PR:**
- Nothing on the Python side. Feature is complete.
- The Rust-side cende blob writer (Task #14) and the Rust-side reader (Task #8) are independent of this stack — they progress in parallel.

---

### Backward-compat verification per PR

| After PR | Default request shape | `withFeeMarketInfo=true` shape | `withSnip35Info=true` shape | `withFeeMarketInfo=true&withSnip35Info=true` shape |
|---|---|---|---|---|
| (today) | strips fee market | includes fee market | (param rejected — unknown) | (param rejected) |
| PR 1 | unchanged | unchanged | (rejected) | (rejected) |
| PR 2 | unchanged | unchanged | (rejected) | (rejected) |
| PR 3 | unchanged | unchanged | unchanged (field doesn't exist) | unchanged |
| PR 4 | strips fee market and `fee_proposal_fri` | includes fee market only (no `fee_proposal_fri`) | includes `fee_proposal_fri` only | includes all three |

The "unchanged" entries in the `withFeeMarketInfo=true` column for every PR are the load-bearing invariant. Any PR that breaks this column has gone wrong.

### Known limitations & rollout-time checks

These don't block the PR stack but should be flagged for review and verified at rollout.

- **Pending block exposes `null` for `fee_proposal_fri`.** The pending block path (`feeder_gateway_impl.py:1295` → `_get_pending_block`) builds `StarknetBlock` from a `BatchCreated` that hasn't been committed yet, so the cende write hasn't happened, so `BatchCreated.get_snip35_info()` returns the default. SNIP-35 era pending blocks will report `null` even though the proposer did state a value. This is symmetric with the Rust-side `pending_data.rs:214` TODO (Task #12). Fix would require plumbing `fee_proposal_fri` through a path that doesn't depend on the cende blob — e.g., directly from the in-memory batch state. Out of scope for this stack.

- **Cross-language wire-shape match (Apollo serialization vs CENDE Marshmallow).** Apollo's `Snip35Info` uses `#[derive(Serialize)]`; Python's schema uses `GasPriceField` (which expects hex strings). The wire shapes must agree, exactly as they already do for `FeeMarketInfo` today. PR 2's tests (and Rust Task #14's serialization round-trip test) must explicitly cover Apollo → JSON → Python load. If the existing `FeeMarketInfo` produces hex strings on the Apollo side, the same approach works for `Snip35Info`; if it produces integers and Python coerces, follow that. Either way: confirm before deploy, don't assume.

- **`null` in JSON for old blocks.** With `load_default=None`, Marshmallow dumps a `None` value as JSON `null`. A consumer requesting `withSnip35Info=true` for an old block gets `"fee_proposal_fri": null` in the response. The Rust client's `#[serde(default)]` accepts this. Other downstream consumers (Pathfinder, Juno, RPC frontends) likely handle `null` fine, but verify on rollout — this is the kind of thing that surfaces only in CI when a new client first connects.

### Mapping back to the existing task list

| PR | Tasks consumed |
|---|---|
| PR 1 | #2 (`Snip35Info` dataclass) + part of #6 (tests for it) |
| PR 2 | #16 (CENDE writer) + part of #6 (writer tests) |
| PR 3 | #15 (toggle) + half of #4 (the `remove_snip35_info_from_block_json` helper) + part of #6 (toggle tests) |
| PR 4 | other half of #4 (the field on `StarknetBlock`) + #5 (callsite updates) + part of #6 (response tests, backward-compat verification) |

After all four PRs land, **Task #7** (deploy to staging) is the next non-coding step.

---

## Testing strategy

| Layer | Requires Python deploy? | Catches |
|---|---|---|
| 1. JSON deserialization unit test (Task #10) | No | Schema parsing, optional defaulting. |
| 2. Conversion test (Task #10) | No | Field plumbed through `to_starknet_api_block_and_events()`. |
| 3. Backwards-compat test (Task #10) | No | Old fixtures (no `fee_proposal_fri`) still parse with `None`. |
| 4. Round-trip test (Task #10) | No | `Block → JSON → Block` preserves the field. |
| 5. Cende blob round-trip test (Task #14) | No | Apollo's `Snip35Info` JSON-serializes to a shape the Python recorder accepts. |
| 6. Live integration test (Task #11) | **Yes** | End-to-end against a real feeder gateway. Gated `#[ignore]` until staging deploy. |
| 7. Production rollout (Task #13) | Yes | Mainnet/testnet behavior. |
| 8. Block hash invariance check (Task #13) | Yes | `BlockHash` unchanged before vs. after the new field is emitted. Sanity-check that `BlockHashHeader.new()` doesn't accidentally include `fee_proposal_fri`. |
| 9. Backward-compat shape check (Task #13) | Yes | A request with only `withFeeMarketInfo=true` (no `withSnip35Info`) returns byte-identical JSON before vs. after the deploy. |
