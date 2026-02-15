# In-Memory CairoPie Proving Function

## Overview

This document describes the implementation of an alternative proving function that passes
CairoPie in-memory instead of writing to temporary files.

## Current State

The existing `prove` function in `crates/starknet_os_runner/src/proving/prover.rs`:
1. Writes CairoPie to a temp zip file (can be several GB)
2. Creates file-path-based `SimpleBootloaderInput`
3. Spawns external `stwo_run_and_prove` binary
4. Reads proof from temp file

## New Approach

Use the `proving-utils` library directly with in-memory CairoPie passing:
- `Task::Pie(cairo_pie)` wraps the CairoPie object
- `TaskSpec { task: Rc::new(task), program_hash_function: HashFunc::Blake }`
- `SimpleBootloaderInput { tasks: vec![task_spec], ... }` (from `cairo-program-runner-lib`)
- `ProgramInput::from_value(simple_bootloader_input)` passes data in-memory
- Direct library call to `stwo_run_and_prove` function

## Implementation (Completed)

### 1. Added workspace dependencies to proving repos in `Cargo.toml`

```toml
cairo-air = { path = "../stwo-cairo/stwo_cairo_prover/crates/cairo-air" }
cairo-program-runner-lib = { path = "../proving-utils/crates/cairo-program-runner-lib" }
stwo-cairo-adapter = { path = "../stwo-cairo/stwo_cairo_prover/crates/adapter" }
stwo_cairo_prover = { path = "../stwo-cairo/stwo_cairo_prover/crates/prover" }
stwo_run_and_prove_lib = { path = "../proving-utils/crates/stwo_run_and_prove", package = "stwo-run-and-prove" }
```

Version compatibility note:
- This setup assumes `proving-utils` is synced to main tip `316344a31891`.
- `proving-utils`, `stwo`, and `stwo-cairo` currently pin some `cairo-lang-*` and `cairo-vm`
  dependencies below the versions used by the sequencer workspace.
- The workspace keeps its own versions, and this in-memory proving flow links against the proving
  repos directly as a separate dependency set.

### 2. Updated `crates/proving_utils/Cargo.toml`

Added feature flag `in_memory_proving` with dependencies:
- `cairo-air`
- `cairo-program-runner-lib`
- `cairo-vm`
- `stwo_run_and_prove_lib`
- `stwo-cairo-adapter`

### 3. Created new module `crates/proving_utils/src/in_memory_proving.rs`

This module:
- Re-exports relevant types from `cairo-program-runner-lib` (Task, TaskSpec, HashFunc, SimpleBootloaderInput, ProgramInput)
- Re-exports `stwo_run_and_prove` function and error types from the library
- Provides `create_bootloader_input_from_pie()` helper function
- Provides `prove_pie_in_memory()` wrapper function

### 4. Added `prove_in_memory` function in `crates/starknet_os_runner/src/proving/prover.rs`

The function is gated behind the `in_memory_proving` feature flag.

### 5. Updated error types

Added `InMemoryProverExecution` error variant to `ProvingError` (feature-gated).

### 6. Added crate-local `rust-toolchain.toml`

The proving-utils library requires Rust nightly. Added:
```toml
[toolchain]
channel = "nightly-2025-07-14"
```

## Usage

Enable the feature in your dependency:

```toml
starknet_os_runner = { path = "...", features = ["in_memory_proving"] }
```

Then use the new function:

```rust
#[cfg(feature = "in_memory_proving")]
let output = prove_in_memory(cairo_pie)?;
```

## Key Benefits

- Eliminates CairoPie file I/O (can be GBs of data)
- Eliminates SimpleBootloaderInput JSON serialization
- Direct library call instead of subprocess spawn
- Faster execution, lower memory overhead from file operations

## Tasks (Completed)

- [x] Add proving-repo dependencies to workspace Cargo.toml
- [x] Update crates/proving_utils to expose library types and function
- [x] Implement prove_in_memory function in prover.rs
- [x] Update ProvingError enum with new error variants
- [x] Add crate-local rust-toolchain.toml for nightly-2025-07-14
