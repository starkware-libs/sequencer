# Prompt: Add SNIP-35 `fee_proposal_fri` to the centralized feeder gateway

You are working in `/home/andrew/workspace/starkware/` — the centralized Python monorepo that runs the Starknet feeder gateway and CENDE pipeline. Your task is to add a new field, `fee_proposal_fri`, to the data this side records and serves. The design has been worked out in detail; this document is the plan.

The repo is a Graphite-tracked git repo. Trunk branch is `dev`. Use `gt c -am "..."` to create branches and `gt s` to submit (after the design is approved by the manager — see the prerequisite section).

## TL;DR

Apollo (the Rust consensus orchestrator in a sibling repo) will start emitting `fee_proposal_fri` (a SNIP-35 proposer-stated fee value) in the cende blob. Your job:

1. Capture it in the CENDE recorder and persist it to its own storage namespace.
2. Add it to the `StarknetBlock` JSON response.
3. Gate it behind a new opt-in query toggle `withSnip35Info=true` so existing callers see no change.

You write **no SNIP-35 logic** in Python. The proposer (Apollo) decides the value; you record and serve it. This is plumbing.

## Context — what `fee_proposal_fri` is and where it comes from

`fee_proposal_fri` is a per-block value that Apollo proposers declare during consensus. It's part of SNIP-35 (dynamic L2 gas pricing): the proposer announces "the fee I think this block should target," derived from the STRK/USD oracle, clamped into a band around recent fee history.

The value flows like this:

```
Apollo (Rust): proposer states fee_proposal_fri in ProposalInit
            └─ also writes Snip35Info { fee_proposal_fri } into the cende blob
                ↓
[centralized storage]
                ↓
CENDE recorder (Python — YOU): reads blob, persists Snip35Info
                ↓
Feeder gateway (Python — YOU): on withSnip35Info=true, includes fee_proposal_fri in JSON
                ↓
Other clients (Rust, Pathfinder, Juno): read it from JSON
```

Apollo is always the proposer (round-robin among consensus nodes). The centralized Python is **never** the proposer. So you have no decision to make about the value's content; you're just plumbing.

## Architectural decisions already settled

These are not up for re-litigation. If something seems wrong, ask the manager — don't redesign.

### 1. `fee_proposal_fri` does NOT enter `BlockHash` or `PartialBlockHash`

The block hash is L1's reference for chain identity. Adding fields to it requires synchronized updates across every Starknet implementation (Cairo OS, virtual Cairo OS, alt-implementations). It's deliberately not changing.

`fee_proposal_fri` only enters `ProposalCommitment` — the consensus-internal hash. That binding is done on the Rust side. On the Python side: **do not touch `BlockHashHeader.new()` at `block_hash_calculator/shared_objects.py:332-364`.** That struct feeds the block hash; including `fee_proposal_fri` there would break the chain.

### 2. `Snip35Info` is a new, independent dataclass — NOT a field on `FeeMarketInfo`

`FeeMarketInfo` (at `shared_objects.py:757`) currently holds `l2_gas_consumed` and `next_l2_gas_price` (EIP-1559 measurements). It's stored as `IndexedDBObject`s in production. Production blobs already exist — they have two fields.

Adding a third field to `FeeMarketInfo` would mean either (a) writing a one-shot script to migrate every existing blob, or (b) accepting a permanent ambiguity where `fee_proposal_fri == None` could mean "old block" or "proposer chose None." Both are bad.

Instead, define a new `Snip35Info` dataclass alongside `FeeMarketInfo`. New `IndexedDBObject`. New storage namespace. Zero production data touched. Existence of a `Snip35Info` blob unambiguously means "block was committed in the SNIP-35 era."

### 3. New opt-in query toggle `withSnip35Info=true` (not piggybacked on `withFeeMarketInfo`)

Existing requests with `withFeeMarketInfo=true` must continue to receive byte-identical JSON. The `apollo_starknet_client` Rust client uses `#[serde(deny_unknown_fields)]` — a new field appearing unconditionally would hard-fail every old version of that client. Other implementations (Pathfinder, Juno) likely have similar strict parsing.

Solution: a new toggle `withSnip35Info=true`, independent of `withFeeMarketInfo`. The two strip operations are independent (no chained `elif` logic). Default: strip the new field. Only opt-in callers see it.

### 4. Centralized Python is record-only — no SNIP-35 computation

There is no SNIP-35 logic in this codebase today (verified: zero matches for `fee_proposal`, `snip35`, etc.). And there shouldn't be after this work. The proposer (Apollo) computes; you record and serve.

If a manager or reviewer suggests "compute fee_proposal_fri here" — push back. The centralized Python is never the proposer, so it has no basis to decide a value.

## Prerequisite — DO NOT START coding without this

**Manager approval.** Before opening any PR, run the design by the manager of the centralized side. Specifically confirm:

- The architecture: separate `Snip35Info` dataclass, opt-in toggle, no computation.
- The deployment strategy: 4 small PRs in a stack, manager will coordinate the staging deploy.
- Any team-specific concerns (rollout cadence, breaking changes window, etc.).

You can do the *exploration* step below before manager approval — that just learns conventions, doesn't change code.

## Pre-flight: explore the repo's conventions

Before opening PR 1, spend ~15 minutes learning how this repo wants you to work. Run these commands and read the output:

```bash
# Trunk branch and recent activity
git log --oneline dev -20

# Test command — look for Bazel targets, pytest invocations, any "make test" alias
find . -maxdepth 3 -name "BUILD" -o -name "Makefile" -o -name "pyproject.toml" | head
cat src/starkware/starknet/services/feeder_gateway/BUILD 2>&1 | head -40

# Formatter / linter — black? isort? mypy?
find . -maxdepth 2 -name "pyproject.toml" -o -name ".pre-commit-config.yaml" -o -name "setup.cfg"

# Commit message conventions — read several recent commits
git log --format="%s" -30 dev

# Code review culture
ls .github/ 2>&1; cat .github/CODEOWNERS 2>&1 | head -20

# Existing test for the FeeMarketInfo that we're mirroring
find . -path "*cende_recorder_test.py" -o -path "*response_objects_test.py" -o -path "*feeder_gateway_impl_test.py" | head -5
```

Read at least one nearby test file (`cende_recorder_test.py` or `response_objects_test.py`) to see what test style this team uses.

## Verification table (file paths confirmed against the synced repo)

These are the locations you'll be editing. All line numbers verified.

| Path | What's there |
|---|---|
| `src/starkware/starknet/definitions/fields.py:593` | `gas_price_metadata_default_1 = GasPriceField.metadata(required=False, load_default=1)` — DO NOT use this for `fee_proposal_fri` (defaults to 1, not None). Define a new helper here. |
| `src/starkware/starknet/services/batcher/shared_objects.py:60` | `FEE_MARKET_INFO_KEY = "fee_market_info"` — add `SNIP35_INFO_KEY = "snip35_info"` nearby. |
| `src/starkware/starknet/services/batcher/shared_objects.py:453` | `BatchCreated.get_fee_market_info(self, storage)` — mirror this for `get_snip35_info`. |
| `src/starkware/starknet/services/batcher/shared_objects.py:757` | `class FeeMarketInfo(ValidatedMarshmallowDataclass, IndexedDBObject)` — new `Snip35Info` lives nearby. |
| `src/starkware/starknet/services/api/feeder_gateway/response_objects.py:919-1101` | `class StarknetBlock` and friends. `next_l2_gas_price` field is at line 987; `StarknetBlock.create()` at 996-1049; `dump_without_fee_market_info` at 1092; `remove_fee_market_info_from_block_json` helper at 1097. |
| `src/starkware/starknet/services/feeder_gateway/feeder_gateway.py:109` | `WITH_FEE_MARKET_INFO = "withFeeMarketInfo"` — add `WITH_SNIP35_INFO` nearby. |
| `src/starkware/starknet/services/feeder_gateway/feeder_gateway.py:392` | `validate_request_field_names` — extend allowlist. |
| `src/starkware/starknet/services/feeder_gateway/feeder_gateway.py:398` | Where `with_fee_market_info` is parsed — parse `with_snip35_info` here. |
| `src/starkware/starknet/services/feeder_gateway/feeder_gateway.py:415` | Where `remove_fee_market_info_from_block_json` is conditionally called — add a parallel call for the new helper. |
| `src/starkware/starknet/services/feeder_gateway/feeder_gateway_impl.py:1315` | `StarknetBlock.create(...)` callsite (pending block path). |
| `src/starkware/starknet/services/utils/sequencer_internal_api_utils.py:521` | `StarknetBlock.create(...)` callsite (non-pending path). |
| `src/starkware/starknet/services/cende/cende_recorder.py:108` | `class Blob` dataclass. |
| `src/starkware/starknet/services/cende/cende_recorder.py:144` | `Blob.from_dict` — parses JSON blob into the dataclass. |
| `src/starkware/starknet/services/cende/cende_recorder.py:446` | `asyncio.gather` block in the writer driver — calls `_write_fee_market_info` and others. |
| `src/starkware/starknet/services/cende/cende_recorder.py:703` | `async def _write_fee_market_info` — mirror this for `_write_snip35_info`. |
| `src/starkware/starknet/services/cende/cende_recorder.py:1004` | `additional_object_keys = { ... }` dict construction. SNIP35_INFO_KEY needs to be added when the write succeeded. |
| `src/starkware/starknet/services/applicative_backup/applicative_backup.py:259` | `fee_market_index = additional_object_keys.pop(FEE_MARKET_INFO_KEY, None)` — add a parallel pop for `SNIP35_INFO_KEY`. The assert at line 267 enforces every key is accounted for. |
| `src/starkware/starknet/services/applicative_backup/conftest.py:457`<br>`src/starkware/starknet/services/applicative_backup/test_utils.py:211` | Mirror updates to `additional_object_keys` and pop call. |
| `src/starkware/starknet/services/block_hash_calculator/shared_objects.py:332-364` | `BlockHashHeader.new()` — **DO NOT TOUCH**. Hash invariant: `fee_proposal_fri` must not enter the block hash. |

## The 4 PRs

Each PR is independently deployable and preserves the **backward-compat invariant**: existing requests with `withFeeMarketInfo=true` (and no `withSnip35Info`) must return byte-identical JSON before vs. after every PR.

```
PR 1 (foundation: Snip35Info dataclass + accessor)
  ↓
PR 2 (CENDE storage writer + applicative_backup updates)
  ↓
PR 3 (toggle scaffolding: withSnip35Info + strip helper)
  ↓
PR 4 (response field + populate from storage)
```

### PR 1 — Foundation: define `Snip35Info` dataclass and storage accessor

**Goal:** introduce the new dataclass without changing any runtime behavior. Pure type addition.

**Changes:**

1. **`src/starkware/starknet/definitions/fields.py`** (near line 593, alongside `gas_price_metadata` / `gas_price_metadata_default_1`):

   Define a new metadata helper that defaults to `None`:

   ```python
   optional_fee_proposal_fri_metadata = GasPriceField.metadata(required=False, load_default=None)
   ```

   We need this because `gas_price_metadata_default_1` defaults to `1`, not `None`. For `fee_proposal_fri`, missing-from-blob must mean "no value stated," which must be `None`.

2. **`src/starkware/starknet/services/batcher/shared_objects.py`** (near line 60, alongside `FEE_MARKET_INFO_KEY`):

   ```python
   SNIP35_INFO_KEY = "snip35_info"
   ```

3. **`src/starkware/starknet/services/batcher/shared_objects.py`** (near line 757, alongside `FeeMarketInfo`):

   ```python
   @marshmallow_dataclass.dataclass(frozen=True)
   class Snip35Info(ValidatedMarshmallowDataclass, IndexedDBObject):
       """SNIP-35: proposer-stated fee values for a block."""

       fee_proposal_fri: Optional[int] = field(metadata=fields.optional_fee_proposal_fri_metadata)

       @classmethod
       def default(cls) -> "Snip35Info":
           return cls(fee_proposal_fri=None)

       @classmethod
       async def set_raw_obj(cls, storage: Storage, index: int, raw_obj: bytes):
           await storage.set_value(key=cls.db_key(str(index).encode("ascii")), value=raw_obj)
   ```

4. **`src/starkware/starknet/services/batcher/shared_objects.py`** (near line 453, alongside `BatchCreated.get_fee_market_info`):

   ```python
   async def get_snip35_info(self, storage: Storage) -> "Snip35Info":
       """
       Fetches SNIP-35 info from storage or returns default values if missing
       (pre-V0_14_3 blocks).
       """
       key = self.additional_object_keys.get(SNIP35_INFO_KEY)
       if key is None:
           return Snip35Info.default()
       return await Snip35Info.get_obj_or_fail(storage=storage, index=key)
   ```

5. **Tests** in the appropriate test module (mirror the layout of existing `FeeMarketInfo` tests):
   - `Snip35Info.default()` returns `fee_proposal_fri=None`.
   - Marshmallow schema round-trip (dump → load preserves the field).
   - `IndexedDBObject` set/get against a fake storage.
   - `BatchCreated.get_snip35_info` returns default when `SNIP35_INFO_KEY` is missing from `additional_object_keys`.
   - `BatchCreated.get_snip35_info` returns the stored value when the key is present.

**Why this PR is safe alone:**
- Adds a new type and a new accessor. Doesn't touch any existing code paths.
- No existing storage blob format changes.
- No response object changes.
- The accessor is unused in production (no callers yet).

**What's still missing after PR 1:**
- Nothing writes `Snip35Info` to storage (PR 2).
- Nothing reads it into responses (PR 4).
- The `withSnip35Info` toggle doesn't exist (PR 3).

---

### PR 2 — CENDE recorder writes `Snip35Info` to storage

**Goal:** persist `Snip35Info` blobs to storage. Once Apollo (in the sibling repo) starts emitting `snip35_info` in cende blobs, CENDE captures it.

**Changes:**

1. **`cende_recorder.py:108`** (the `Blob` dataclass): add a new field, defaulted to `None` for backward compat with old Apollo blobs:

   ```python
   snip35_info: Optional[str] = None
   ```

2. **`cende_recorder.py:144`** (`Blob.from_dict`): tolerate missing key:

   ```python
   snip35_info=json.dumps(data["snip35_info"]) if "snip35_info" in data else None,
   ```

3. **`cende_recorder.py:446`** (the `asyncio.gather` block in the writer driver): after the existing `self._write_fee_market_info(...)` call, conditionally call the new writer:

   ```python
   *([self._write_snip35_info(
         batch_id=next_batch_info.batch_id, snip35_info=blob.snip35_info,
     )] if blob.snip35_info is not None else []),
   ```

   Or refactor to a clearer conditional. Do not write when `snip35_info is None`.

4. **`cende_recorder.py`** (after line 703, mirroring `_write_fee_market_info`):

   ```python
   async def _write_snip35_info(self, batch_id: int, snip35_info: str):
       await self.storage.set_str(
           Snip35Info.db_key(suffix=str(batch_id).encode("ascii")), value=snip35_info
       )
   ```

5. **`cende_recorder.py:1004`** (the `additional_object_keys` dict construction): conditionally add the new key when the snip35 write succeeded. Mirror the existing `if compressed_state_diff_written` pattern at line 1010:

   ```python
   if blob.snip35_info is not None:
       additional_object_keys[SNIP35_INFO_KEY] = batch_id
   ```

   This is what makes `BatchCreated.get_snip35_info()` find the stored entry. Without it, the storage write happens but the read path can't locate it.

6. **`applicative_backup/applicative_backup.py:259`**: after the existing fee_market_info pop, add a parallel pop for SNIP35_INFO_KEY. The assertion at line 267 (`assert len(additional_object_keys) == 0`) enforces every known key is accounted for, so missing this update will crash applicative_backup the moment any batch carries `SNIP35_INFO_KEY`:

   ```python
   snip35_info_index = additional_object_keys.pop(SNIP35_INFO_KEY, None)
   ```

   Then plumb the index through to wherever the other indices are used downstream (around lines 272–277 / wherever the data is loaded).

7. **`applicative_backup/conftest.py:457`** and **`applicative_backup/test_utils.py:211`**: mirror updates to the test-fixture `additional_object_keys` dict and the corresponding pop calls.

8. **Tests:**
   - The writer correctly persists a blob with `snip35_info`.
   - Backward-compat: a blob *without* `snip35_info` still loads and writes everything else without crashing.
   - `applicative_backup` handles a batch with `SNIP35_INFO_KEY` (no assertion crash).
   - `applicative_backup` still works for old batches without `SNIP35_INFO_KEY`.

**Why this PR is safe alone:**
- Backward-compat with old Apollo blobs: missing `snip35_info` → no write, no error.
- Nothing reads `Snip35Info` from storage yet (PR 4 does), so even when writes succeed, behavior is unobservable.
- No response shape changes.
- No new query parameters.

**What's still missing after PR 2:**
- Apollo isn't yet writing `snip35_info` to its cende blobs (separate Rust work in the sibling repo). Until that lands, this writer is a no-op (every blob has `snip35_info=None`).
- Once Apollo's update ships and a block is committed, `Snip35Info` will start appearing in storage. But it still doesn't show up in responses until PR 4.

---

### PR 3 — Add `withSnip35Info` query toggle and strip helper

**Goal:** introduce the opt-in toggle and the strip helper *before* the field is exposed (PR 4). This way, when PR 4 adds the field to the response, the toggle is already there to gate it. Until PR 4, the toggle is a no-op.

**Changes:**

1. **`response_objects.py`** (after `remove_fee_market_info_from_block_json` at line 1097):

   ```python
   def remove_snip35_info_from_block_json(block_json: dict) -> dict:
       block_json = block_json.copy()
       block_json.pop("fee_proposal_fri", None)
       return block_json
   ```

   Use `pop(..., None)` *with default* — this makes it a safe no-op when the key isn't yet in the response (which it isn't, until PR 4).

2. **`feeder_gateway.py:109`** (alongside `WITH_FEE_MARKET_INFO`):

   ```python
   WITH_SNIP35_INFO = "withSnip35Info"
   ```

3. **`feeder_gateway.py:392`** (`validate_request_field_names`):

   ```python
   field_names=self.BLOCK_IDENTIFIERS | {self.HEADER_ONLY, self.WITH_FEE_MARKET_INFO, self.WITH_SNIP35_INFO},
   ```

   Without this, requests carrying the new param get rejected with "unknown field."

4. **`feeder_gateway.py:398`** (alongside `with_fee_market_info` parsing):

   ```python
   with_snip35_info = self._parse_flag(
       request=request, flag_name=self.WITH_SNIP35_INFO, default=False
   )
   ```

5. **`feeder_gateway.py:415`** (after the existing `if not with_fee_market_info` strip):

   ```python
   if not with_snip35_info:
       block_json = remove_snip35_info_from_block_json(block_json=block_json)
   ```

   Two **independent** strip operations (no chained `elif`) — each toggle controls its own data.

6. **Docstring at `feeder_gateway.py:366-368`**: document the new flag.

7. **Tests:**
   - `withSnip35Info=true` is allowlisted (request doesn't get rejected as "unknown field").
   - The strip helper is a safe no-op when the response doesn't contain `fee_proposal_fri`.
   - Response shape without the toggle is byte-identical to current behavior.
   - Response shape with `withFeeMarketInfo=true` is byte-identical to current behavior (the `fee_proposal_fri` key is absent).

**Why this PR is safe alone:**
- The strip helper uses `pop("fee_proposal_fri", None)` — won't crash when the key is absent (which it is, until PR 4).
- The toggle exists but has no observable effect: the strip is a no-op, and the field isn't in the response yet.
- Existing requests (no toggle) behave identically to today.
- Existing requests with `withFeeMarketInfo=true` behave identically to today.
- Even requests with `withSnip35Info=true` produce the same response as today.

**What's still missing after PR 3:**
- The response object has no `fee_proposal_fri` field (PR 4 adds it).
- The two `StarknetBlock.create()` callsites don't pass `fee_proposal_fri` (PR 4 updates them).

---

### PR 4 — Expose `fee_proposal_fri` in `StarknetBlock` response

**Goal:** complete the feature. Add the response field, update `StarknetBlock.create()`, update both callsites to read `Snip35Info` from storage and pass the value through.

**Changes:**

1. **`response_objects.py`** (around line 988, after `next_l2_gas_price`):

   ```python
   fee_proposal_fri: Optional[int] = field(metadata=fields.optional_fee_proposal_fri_metadata)
   ```

   The new metadata helper from PR 1, with `load_default=None`. Top-level field on `StarknetBlock`, parallel in shape to `next_l2_gas_price`.

   **Do NOT use `gas_price_metadata_default_1`** — that defaults to `1`, which would surface as `1` for old blocks instead of `None`.

2. **`response_objects.py`** (`StarknetBlock.create()`, lines 996-1049): add `fee_proposal_fri` to the params and pass it through to the constructor. Required (typed `Optional`), matching the `next_l2_gas_price` pattern. All callsites must be updated atomically:

   ```python
   def create(
       cls: Type[TBlockInfo],
       ...
       l2_gas_consumed: Optional[int],
       next_l2_gas_price: Optional[int],
       fee_proposal_fri: Optional[int],
   ) -> TBlockInfo:
       return cls(
           ...
           l2_gas_consumed=l2_gas_consumed,
           next_l2_gas_price=next_l2_gas_price,
           fee_proposal_fri=fee_proposal_fri,
       )
   ```

3. **`feeder_gateway_impl.py:1315`** (pending block path): read `Snip35Info` and pass it to `StarknetBlock.create()`:

   ```python
   fee_market_info = await batch.get_fee_market_info(storage=self.storage)
   snip35_info = await batch.get_snip35_info(storage=self.storage)
   return StarknetBlock.create(
       ...
       l2_gas_consumed=fee_market_info.l2_gas_consumed,
       next_l2_gas_price=fee_market_info.next_l2_gas_price,
       fee_proposal_fri=snip35_info.fee_proposal_fri,
   )
   ```

4. **`sequencer_internal_api_utils.py:521`** (non-pending path): same — read `Snip35Info` and pass `fee_proposal_fri`.

5. **`block_hash_calculator/shared_objects.py:332-364`** (`BlockHashHeader.new()`): **DO NOT MODIFY**. The block hash struct must NOT learn about `fee_proposal_fri`. The fact that `BlockHashHeader.new()` accepts a `FeeMarketInfo` is fine — it just consumes `l2_gas_consumed` and `next_l2_gas_price` from it and ignores anything else. Don't add a `Snip35Info` parameter to that path.

6. **Tests:**
   - `StarknetBlock` schema includes `fee_proposal_fri`.
   - `withSnip35Info=true` request gets the field populated when `Snip35Info` is in storage.
   - `withSnip35Info=true` request gets `fee_proposal_fri=None` (or the key omitted, depending on Marshmallow serialization) when storage is empty (old block).
   - `withSnip35Info=false` (default) request has no `fee_proposal_fri` in the JSON.
   - **Backward-compat verification:** `withFeeMarketInfo=true` (without `withSnip35Info`) returns byte-identical JSON to the pre-PR shape. Test this explicitly.
   - Pending block path returns `fee_proposal_fri=None` (see "Known limitations" below — pending blocks can't get the value via this path).

**Why this PR is safe alone:**
- The toggle from PR 3 is in place: requests without `withSnip35Info=true` have the field stripped. Existing callers see no shape change.
- For old blocks (no `Snip35Info` in storage), `get_snip35_info` returns `Snip35Info.default()` → `fee_proposal_fri=None`. Either serializes as `null` or is omitted depending on Marshmallow metadata. Either is acceptable; the Rust client (`#[serde(default)]`) accepts both.
- For new blocks (with Apollo's update deployed and `Snip35Info` in storage), the actual proposer-stated value flows through.
- `BlockHash` is unchanged because `BlockHashHeader` is untouched.

**What's still missing after PR 4:**
- Nothing on this side. Feature is complete.
- Apollo's outbound write side is in a separate Rust repo and progresses on its own schedule.

## Backward-compat verification per PR

| After PR | Default request | `withFeeMarketInfo=true` | `withSnip35Info=true` | Both `=true` |
|---|---|---|---|---|
| (today) | strips fee market | includes fee market | (rejected — unknown field) | (rejected) |
| PR 1 | unchanged | unchanged | (rejected) | (rejected) |
| PR 2 | unchanged | unchanged | (rejected) | (rejected) |
| PR 3 | unchanged | unchanged | unchanged (field doesn't exist yet) | unchanged |
| PR 4 | strips fee market AND `fee_proposal_fri` | includes fee market only (NO `fee_proposal_fri`) | includes `fee_proposal_fri` only | includes all three |

The `withFeeMarketInfo=true` column is the load-bearing invariant. If any PR breaks this column, something is wrong.

## Known limitations and rollout-time checks

These don't block the PR stack but should be flagged in PR descriptions or follow-up tickets.

### 1. Pending blocks expose `null` for `fee_proposal_fri`

The pending block path (`feeder_gateway_impl.py:1295` → `_get_pending_block`) builds `StarknetBlock` from a `BatchCreated` that hasn't been committed yet, so the cende blob hasn't been written, so `BatchCreated.get_snip35_info()` returns `Snip35Info.default()`. Even SNIP-35 era pending blocks will report `fee_proposal_fri=null` (or omit the key).

This is symmetric with how `next_l2_gas_price` is populated for pending blocks — verify that's also done via `fee_market_info` from storage (it is, per the line 1314 pattern). So the limitation is consistent with existing behavior. A future improvement would plumb the value directly from in-memory batch state, but that's out of scope.

### 2. Cross-language wire-shape match (Apollo serialization vs CENDE Marshmallow)

Apollo's Rust `Snip35Info` uses `#[derive(Serialize)]`; your `Snip35Info` schema uses `GasPriceField` (which expects hex strings). The wire shapes must agree, exactly as they already do for `FeeMarketInfo`.

When validating PR 2, write a test that:
- Constructs an Apollo-shaped JSON (a string like `{"fee_proposal_fri": "0x123"}` or whatever shape the Rust side emits).
- Loads it through `Snip35Info.Schema().load(json.loads(s))`.
- Verifies the result matches expectations.

If the existing `FeeMarketInfo` produces hex strings on the Apollo side, the same will apply for `Snip35Info`. If it produces integers and Python coerces, follow that. Either way: confirm with a test, don't assume.

The cende blob payload is something you can grep for in test fixtures or Apollo logs.

### 3. `null` in JSON for old blocks

With `load_default=None`, Marshmallow dumps a `None` value as JSON `null` (by default). A consumer requesting `withSnip35Info=true` for an old block will receive `"fee_proposal_fri": null` in the response.

The Rust client (the primary consumer in scope) handles this fine. Other consumers (Pathfinder, Juno, RPC frontends) should also handle `null` correctly — it's standard JSON. But verify on rollout: the failure mode would be a downstream parser choking, which surfaces only in CI when a new client first connects.

If a downstream consumer does need the field omitted entirely instead of `null`, you can add `dump_default=marshmallow.missing` to the metadata. Don't preemptively do this; only if there's a reported issue.

## Mapping to the existing task tracking

The user has been tracking this work in their sequencer repo with these tasks:

- Task #2 → PR 1 (`Snip35Info` dataclass)
- Task #16 → PR 2 (CENDE writer)
- Task #15 → PR 3 (toggle)
- Task #4 (response field portion) + Task #5 (callsites) → PR 4
- Task #6 (tests) → distributed across all 4 PRs
- Task #7 (deploy) → after all 4 PRs land

After PR 4 lands, the next step is deploying to staging. That's coordinated with the user's manager and the Rust-side work in the sibling repo.

## Final notes

- The Rust outbound (`Apollo` writes `snip35_info` into cende blobs) is being built in parallel in a sibling repo. Don't wait for it. Your work is forward-compatible: if it lands first, blobs will not yet have `snip35_info`, and your PR 2 tolerates that. If it lands second, your stack already accommodates that case.
- The user's manager on the centralized side is `[FILL IN]`. Run the design by them before opening PR 1.
- Be explicit in PR descriptions about the backward-compat invariant. Reviewers will appreciate the verification matrix above.
- If you hit any divergence between this plan and the actual code (a line moved, a helper renamed, a pattern that doesn't fit), pause and reconcile before changing files. Don't paper over differences silently.
