# Manual Test Bug Report

Bugs found during manual testing of the `starknet_transaction_prover` proving
service. These are code-level issues to fix in a follow-up patch.

---

## Bug 1: Upstream code 41 (TransactionExecutionError) hidden as -32603

**Symptom**: Section 5.5 of the manual testing guide — sending an invoke tx with
a mutated nonce returns `-32603` ("Internal error") instead of a user-facing
error. The upstream RPC node returns code 41 with execution error details, but
the prover wraps it as an internal error.

**Expected**: Error code 41 or 55 with the execution error detail surfaced to
the caller.

**Root cause**: `virtual_block_executor.rs:524` wraps
`RPCStateReaderError::TransactionExecutionError` as
`VirtualBlockExecutorError::ReexecutionError`, which `server/errors.rs:160` maps
to `internal_server_error()`.

**Fix**: Match `RPCStateReaderError::TransactionExecutionError` at line 524,
extract `execution_error` from `RpcErrorResponse.error.data`, and surface it as
a new `TRANSACTION_EXECUTION_ERROR` (code 41) in the prover API.

**Note**: Upstream code 41 covers both validation AND execution failures (no
structured distinction). Using code 41 (not 55) avoids mislabeling execution
reverts as validation failures.

---

## Bug 2: Upstream code 42 (StorageProofNotSupported) hidden as -32603

**Symptom**: Steps 7.5 & 8 of the manual testing guide — requesting a proof for
a stale block returns `-32603` with data containing
`"RPC provider error: JSON-RPC error: code=42, message=\"The node doesn't support storage proofs for blocks that are too far in the past\""`.

**Expected**: User-facing error code 42 with a clear "block too old" message.

**Root cause**: `server/errors.rs:140` maps all `RunnerError::ProofProvider(_)`
to `internal_server_error()`.

**Fix**: Match
`ProofProviderError::Rpc(ProviderError::StarknetError(StarknetError::StorageProofNotSupported))`
in `runner_error_to_rpc` and return a new `STORAGE_PROOF_NOT_SUPPORTED` (code
42) error.

**Key types**: `ProviderError::StarknetError(StarknetError::StorageProofNotSupported)`
from `starknet-rust-providers-0.16.0` is a structured, matchable variant.

---

## Detailed fix plan

A step-by-step implementation plan for both bugs is saved at:
`.claude/plans/snuggly-soaring-cherny-code-fixes.md`
