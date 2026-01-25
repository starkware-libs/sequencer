# Proving VirtualOS with Stwo Run and Prove

This document outlines how to prove the `starknet_os_runner` with the extra hint processor using `stwo_run_and_prove` from the `proving-utils` repository.

## Overview

The goal is to:
1. Pass a `Cairo0Program` task (instead of a PIE) containing the VirtualOS program to `stwo_run_and_prove`
2. Pass the `SnosHintProcessor` as the `extra_hint_processor` so the bootloader can execute VirtualOS hints

## Toolchain Constraint

**IMPORTANT**: The `stwo` crate (a dependency of `stwo_run_and_prove`) requires nightly Rust features such as `#![feature(array_chunks)]`. The sequencer uses stable Rust (1.92), while proving-utils uses nightly (nightly-2025-07-14).

To use the proving feature, you must use nightly Rust:
```bash
rustup run nightly-2025-07-14 cargo build -p starknet_os_runner --features stwo_native
```

The `stwo_native` feature is optional and does not affect the default stable build.

## Location

All direct library proving functionality is in the `starknet_os_runner` crate:
- `crates/starknet_os_runner/src/proving/stwo_direct.rs` - Main implementation

The `starknet_os` crate remains unchanged from its original state.

## Changes Made

### 1. Workspace Dependencies (`Cargo.toml`)

Added dependencies for the external proving-utils crates:

```toml
proving_utils_cairo_program_runner_lib = { package = "cairo-program-runner-lib", path = "../proving-utils/crates/cairo-program-runner-lib" }
proving_utils_stwo_run_and_prove = { package = "stwo_run_and_prove", path = "../proving-utils/crates/stwo_run_and_prove" }
```

### 2. Starknet OS Runner Dependencies (`crates/starknet_os_runner/Cargo.toml`)

Added optional dependencies and a `stwo_native` feature:

```toml
[features]
stwo_native = [
    "apollo_starknet_os_program",
    "blockifier/testing",
    "proving_utils_cairo_program_runner_lib",
    "proving_utils_stwo_run_and_prove",
    "starknet_os/testing",
]

[dependencies]
apollo_starknet_os_program = { workspace = true, optional = true }
proving_utils_cairo_program_runner_lib = { workspace = true, optional = true }
proving_utils_stwo_run_and_prove = { workspace = true, optional = true }
```

### 3. Error Type (`crates/starknet_os_runner/src/errors.rs`)

Added a `StwoDirectProvingError` enum for the direct proving functionality.

### 4. proving-utils Re-export (`../proving-utils/crates/stwo_run_and_prove/src/lib.rs`)

Added re-export of `ProofFormat` to avoid version conflicts:

```rust
pub use cairo_air::utils::ProofFormat;
```

## Usage

```rust
use starknet_os_runner::proving::stwo_direct::{run_and_prove_virtual_os, StwoDirectProvingConfig};
use starknet_os::io::os_input::OsHints;
use std::path::PathBuf;

let os_hints: OsHints = /* ... */;

let proving_config = StwoDirectProvingConfig {
    bootloader_program_path: None,  // Uses bundled simple_bootloader
    proof_output_path: PathBuf::from("path/to/output_proof.json"),
    verify: true,
    prover_params_path: None,
    debug_data_dir: None,
    save_debug_data: false,
};

run_and_prove_virtual_os(os_hints, proving_config)?;
```

## How It Works

### Execution Flow

1. `run_and_prove_virtual_os` validates tracked resources (must be `SierraGas`)
2. Creates an `SnosHintProcessor` with the VirtualOS program
3. Creates a `Cairo0Program` task containing the VirtualOS program
4. This task is wrapped in a `SimpleBootloaderInput` and passed to `stwo_run_and_prove`
5. The bootloader executes the VirtualOS program as a subtask
6. The `BootloaderHintProcessor` delegates VirtualOS-specific hints to the `SnosHintProcessor`
7. A Stwo proof is generated for the entire execution

### Hint Processor Chain

The `BootloaderHintProcessor` in proving-utils supports an `extra_hint_processor` field. When executing hints, it tries processors in this order:

1. Cairo1 subtask hint processor (if in a Cairo1 subtask)
2. Bootloader hints (`MinimalBootloaderHintProcessor`)
3. Special hints (`EXECUTE_TASK_CALL_TASK`, `EXECUTE_TASK_EXIT_SCOPE`)
4. Cairo VM builtin hints (`BuiltinHintProcessor`)
5. **Extra hint processor** (`SnosHintProcessor`)
6. Test program hints

## Key Files

### In `sequencer` repository:
- `crates/starknet_os_runner/src/proving/stwo_direct.rs` - Direct proving implementation
- `crates/starknet_os_runner/src/errors.rs` - Error types
- `crates/starknet_os/src/hint_processor/snos_hint_processor.rs` - SNOS hint processor (unchanged)

### In `proving-utils` repository:
- `crates/stwo_run_and_prove/src/lib.rs` - Main entry point
- `crates/cairo-program-runner-lib/src/lib.rs` - `cairo_run_program` function
- `crates/cairo-program-runner-lib/src/hints/hint_processors.rs` - `BootloaderHintProcessor`
- `crates/cairo-program-runner-lib/src/hints/types.rs` - `Task`, `Cairo0Executable`, `TaskSpec`

## Running Tests

```bash
# Run stwo_direct tests (requires nightly)
rustup run nightly-2025-07-14 cargo test -p starknet_os_runner --features stwo_native stwo_direct

# Verify stable compilation still works
cargo check -p starknet_os_runner
cargo check -p starknet_os
```

## Notes

- The `SnosHintProcessor` implements `HintProcessorLogic` and `ResourceTracker`, which automatically provides `HintProcessor` via a blanket impl in `cairo_vm`
- The VirtualOS program is passed as a `Cairo0Program` task, not as a PIE
- The bootloader used is `simple_bootloader_compiled.json` bundled in the resources directory
