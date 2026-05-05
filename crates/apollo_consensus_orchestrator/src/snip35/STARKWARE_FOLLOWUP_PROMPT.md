# Follow-up: fix `get_state_update` leak in PR 4

You are working in `/home/andrew/workspace/starkware/`. There is an existing 4-PR Graphite stack that adds SNIP-35 `fee_proposal_fri` to the feeder gateway:

```
◉ 05-06-expose_fee_proposal_fri_on_starknetblock_via_withsnip35info_toggle  (PR 4 — top)
◯ 05-06-add_withsnip35info_toggle_and_remove_snip35_info_from_block_json_helper  (PR 3)
◯ 05-06-persist_snip35info_via_cende_recorder_and_applicative_backup  (PR 2)
◯ 05-06-add_snip35info_dataclass_and_batchcreated.get_snip35_info_accessor  (PR 1)
◯ dev
```

PRs are unsubmitted. PR 4 has a backward-compat gap that needs to be fixed by **amending PR 4 in place** — do not insert a new PR.

## The gap

PR 4 added `fee_proposal_fri` to `StarknetBlock`. That's correct. But the `/feeder_gateway/get_state_update?includeBlock=true` endpoint embeds a `StarknetBlock` inside `StarknetBlockAndStateUpdate`, and its existing strip operations only pop `l2_gas_consumed`/`next_l2_gas_price` — they don't pop `fee_proposal_fri`. Result: that endpoint would silently start exposing `fee_proposal_fri` for any block with a `Snip35Info` blob in storage.

The original design intent is that `get_state_update` does NOT carry fee market info (see `feeder_gateway.py:544` — `# Fee market info is not supported in this endpoint.`). The same intent applies to SNIP-35: the only endpoint exposing `fee_proposal_fri` is `get_block` (gated by `withSnip35Info=true`). Every other endpoint must continue to strip it.

## Specific edits required (amend into PR 4)

### Edit 1: `src/starkware/starknet/services/api/feeder_gateway/response_objects.py` — line 1146

`StarknetBlockAndStateUpdate.dump_without_fee_market_info` currently only strips fee market fields:

```python
def dump_without_fee_market_info(self) -> dict:
    data = self.dump()
    data["block"] = remove_fee_market_info_from_block_json(data["block"])
    return data
```

Extend it to also strip `fee_proposal_fri`:

```python
def dump_without_fee_market_info(self) -> dict:
    data = self.dump()
    data["block"] = remove_fee_market_info_from_block_json(data["block"])
    # SNIP-35: get_state_update does not surface SNIP-35 data either, mirroring the
    # fee-market-info policy. Only get_block (gated by withSnip35Info=true) exposes
    # fee_proposal_fri.
    data["block"] = remove_snip35_info_from_block_json(data["block"])
    return data
```

The method name technically becomes a slight misnomer (it now strips both fee market AND snip35 info), but renaming it would force every caller to update, which is bigger churn than this gap deserves. Leave the name; the comment makes the intent clear. If the team objects, the alternative is to rename to `dump_without_fee_market_or_snip35_info` and update all callers — note the existing callers in this commit's scope before deciding.

### Edit 2: `src/starkware/starknet/services/feeder_gateway/feeder_gateway_impl.py` — line 558

The non-pending path of `get_block_and_state_update` currently calls:

```python
block_json = remove_fee_market_info_from_block_json(block_json=block_json)
```

Add a parallel call right after it. Also add the import.

```python
block_json = remove_fee_market_info_from_block_json(block_json=block_json)
# SNIP-35: same policy as fee market info — get_state_update does not surface
# fee_proposal_fri. Only get_block (gated by withSnip35Info=true) exposes it.
block_json = remove_snip35_info_from_block_json(block_json=block_json)
```

At the top of the file (around line 67 where `remove_fee_market_info_from_block_json` is imported from `response_objects`), add `remove_snip35_info_from_block_json` to the import.

### Edit 3: tests

In `src/starkware/starknet/services/feeder_gateway/feeder_gateway_impl_test.py`, find the test(s) that exercise `get_block_and_state_update` (search for `dump_without_fee_market_info` or `get_block_and_state_update`). For any block fixture that gets returned through this path, assert that the resulting JSON does NOT contain `fee_proposal_fri` — even when the block's `Snip35Info` storage is populated with a non-None value.

If there's no existing `get_block_and_state_update` test, add one along these lines (pseudocode — match existing test style):

```python
async def test_get_state_update_with_include_block_strips_fee_proposal_fri(...):
    """
    /get_state_update?includeBlock=true must NOT include fee_proposal_fri in the response,
    mirroring the existing 'fee market info is not supported in this endpoint' policy
    documented at feeder_gateway.py:544.
    """
    # Set up a block with a populated Snip35Info storage entry (fee_proposal_fri = some non-None hex).
    # Hit /get_state_update?includeBlock=true.
    # Assert "fee_proposal_fri" is absent from the response JSON, both for the embedded block
    # and the top-level response.
```

If the existing `feeder_gateway_test.py` (the integration-style test file) has a test for this endpoint, add a similar negative assertion there too. Look for `test_get_state_update_with_flags` or similar.

Also: run the existing tests under `feeder_gateway_test.py` for `withFeeMarketInfo` flow against `get_block` to make sure the fee market path still works — these edits shouldn't touch that, but a regression check is cheap.

## Procedure

1. Confirm you're on the top branch:
   ```bash
   gt log 2>&1 | head -5
   # Should show: 05-06-expose_fee_proposal_fri_on_starknetblock_via_withsnip35info_toggle (current)
   ```

2. Make Edit 1 (`response_objects.py`), Edit 2 (`feeder_gateway_impl.py`), and Edit 3 (tests).

3. Run the test suite for the affected modules. Use whatever test runner the team uses (Bazel, pytest); look at recent commit messages or `BUILD` files for the canonical command. Run at least the tests under:
   - `src/starkware/starknet/services/api/feeder_gateway/response_objects_test.py`
   - `src/starkware/starknet/services/feeder_gateway/feeder_gateway_impl_test.py`
   - `src/starkware/starknet/services/feeder_gateway/feeder_gateway_test.py`

4. Run the formatter the team uses (likely `black` + `isort`; check `pyproject.toml` or `.pre-commit-config.yaml`).

5. Amend PR 4:
   ```bash
   gt m -a
   ```

   Do NOT use `gt m -m`; the existing PR 4 commit message is fine. `gt m -a` stages all changes and amends the existing commit without modifying its message.

6. Verify the restack went cleanly:
   ```bash
   gt log 2>&1 | head -10
   git status
   ```

   There should be no merge conflicts (none of the PR 4 changes overlap with downstack). If `gt m -a` triggered a restack of any branch above PR 4 (there is none in this stack), follow the conflict-resolution prompt; otherwise the amend is done.

## Verification before declaring done

Before reporting back, manually verify the backward-compat invariant:

- `GET /feeder_gateway/get_block?withFeeMarketInfo=true` (without `withSnip35Info`) → response does not contain `fee_proposal_fri`. (Already true after PR 4; just confirming.)
- `GET /feeder_gateway/get_block?withSnip35Info=true` → response contains `fee_proposal_fri`.
- `GET /feeder_gateway/get_state_update?includeBlock=true` → response does NOT contain `fee_proposal_fri`. **This is the new invariant added by this amend.**
- `GET /feeder_gateway/get_state_update?includeBlock=true&includeSignature=true` → same: no `fee_proposal_fri`.

The clean intent: `fee_proposal_fri` only appears in responses from `/get_block` and only when `withSnip35Info=true`. Every other endpoint and every other request shape strips it.

## Why this isn't a new PR

The stack hasn't been submitted to the remote yet (verified via `gt log` — no PR numbers shown). Amending PR 4 is cleaner than inserting a fifth PR for two reasons:

1. PR 4's commit message claims to deliver the SNIP-35 response field plumbing. Without this fix, that delivery is incomplete — the message would be misleading. Amending makes the message honest.
2. Reviewers reading the stack one PR at a time benefit from PR 4 being self-contained: "this PR adds the field to the response, AND ensures it only leaks through the one endpoint that's supposed to surface it."

If the team prefers the historical-record style where every fix is its own PR, you can `gt c --insert -am "..."` instead. Default to amend.
