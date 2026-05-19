# Repro: privacy-prove fails on virtual OS PIE that uses keccak builtin

This branch carries an `#[ignore]`-gated integration test that demonstrates a
deterministic proving failure in `privacy-prove` when the inner task (a Starknet
virtual OS run) contains keccak builtin usage by a user contract.

The keccak **syscall** in the virtual OS is **not** the cause — that path proves
fine. The failure is triggered by the keccak **builtin** appearing in the OS PIE.

## Test

`crates/starknet_os_flow_tests/src/virtual_os_test.rs::prove_and_verify_keccak_tx`

It invokes `test_contract::test_keccak`, which exercises both the keccak builtin
(`keccak::keccak_u256s_le_inputs`) and the keccak syscall (failure path with an
invalid length).

## How to run

Requires the project's Python venv (for the Cairo 0 compiler) and the
nightly Rust toolchain that the prover crate needs.

```bash
# From the repo root:
source sequencer_venv/bin/activate

cargo +nightly-2025-07-14 test \
    -p starknet_os_flow_tests \
    --features starknet_transaction_prover/stwo_proving \
    --release \
    prove_and_verify_keccak_tx \
    -- --ignored --nocapture
```

First run cold-builds the workspace in release mode (~5 min on a fast
machine); subsequent runs are fast. The test itself runs the OS, proves, and
verifies in ~40 s once built.

## Observed failure

```
thread 'virtual_os_test::prove_and_verify_keccak_tx' panicked at
crates/starknet_os_flow_tests/src/virtual_os_test_manager.rs:42:53:
Proving virtual OS should not fail.:
ProvingError(ProverExecution(
  ".../simple_bootloader/execute_task.cairo:209:5: Error at pc=0:615:
  Got an exception while executing a hint: Hint Error:
  Error while relocating Cairo PIE memory: Memory addresses must be relocatable
  Cairo traceback (most recent call last):
  <start>:3:1: (pc=0:2)
  .../simple_bootloader/privacy_simple_bootloader.cairo:109:5: (pc=0:1116)
  .../simple_bootloader/run_simple_bootloader.cairo:114:9: (pc=0:727)
  .../simple_bootloader/run_simple_bootloader.cairo:199:5: (pc=0:782)"
))
```

OS execution succeeds — the failure is strictly in the proving step.

## Suspected root cause

1. `privacy_prove::consts::CAIRO_RUN_CONFIG` (`crates/privacy_prove/src/consts.rs`
   in `proving-utils` rev `580135e`) selects `LayoutName::stwo_no_ecop`.
2. `BuiltinsInstanceDef::stwo_no_ecop()` in `cairo-vm-3.2.0` returns
   `keccak: None, ecdsa: None, ec_op: None`.
3. The privacy bootloader's `simple_bootloader_simulate_keccak` hint installs an
   auto-deduction-rule-based simulated keccak segment, but the relocation logic
   in
   `cairo-program-runner-lib::hints::load_cairo_pie::build_cairo_pie_relocation_table`
   reads `cairo_pie_execution_segment[idx]` for each declared builtin and
   `extract_segment`s a relocatable from it. When the inner OS PIE has actually
   exercised keccak (so the keccak builtin segment in the PIE is non-empty), the
   value found at that cell is a felt (Int), not a relocatable, and
   `extract_segment` returns `MemoryError::AddressNotRelocatable`.

## Isolation experiment (already verified locally)

Comment out the keccak BUILTIN call inside
`crates/blockifier_test_utils/resources/feature_contracts/cairo1/test_contract.cairo::test_keccak`
(the `keccak::keccak_u256s_le_inputs(...)` block, lines ~642–647 on this branch),
leaving only the keccak SYSCALL failure-path call. Re-run the same command — the
test now proves and verifies cleanly. This confirms the failure is exercised by
the keccak BUILTIN, not the SYSCALL.

## Baselines that pass

- `cargo +nightly-2025-07-14 test -p starknet_os_flow_tests --features starknet_transaction_prover/stwo_proving --release generate_proof_fixtures -- --ignored`
  (fund-account tx, no syscalls)
- `prove_and_verify_keccak_tx` modified to call `test_storage_read` instead of
  `test_keccak` (non-keccak syscall)

Both succeed on this branch, confirming the proving path itself works and only
the keccak BUILTIN trip path is affected.
