# apollo_starknet_os_program

This crate contains the source code and compiled programs for the **Starknet Operating System (OS)**, including a specialized **Virtual OS** variant.

## Starknet OS Overview

The Starknet OS is a Cairo program responsible for:
- Executing Starknet transactions (invoke, deploy, declare)
- Managing state transitions (contract storage, class declarations)
- Processing L1↔L2 messages (contracts can send messages to L1, and the OS consumes messages sent from L1)
- Producing provable execution traces

The OS processes blocks of transactions and produces an output that includes:
- State commitment updates (initial and final state roots)
- Block metadata (block numbers, block hashes)
- L1↔L2 messages sent during execution
- State diff data for data availability (contract storage changes, class declarations)

## Virtual OS Program

The **Virtual OS** is a variant of the Starknet OS designed to be safe for **client-side proving**. It allows users to prove their transaction execution locally and submit the proof to Starknet, where its validity is verified by the full OS.

### Why a Separate Variant?

The full Starknet OS is designed to run in a trusted sequencer environment (or subject to consensus). The Virtual OS removes or restricts functionality that would be unsafe or impractical for client-side execution:

- **Single transaction scope**: Processes one `INVOKE_FUNCTION` transaction (not entire blocks)
- **No reverted transactions**: Reverted transactions are currently trusted
- **No deprecated syscalls**: Cairo 0 and legacy syscalls are not supported
- **Previous block context**: Block info refers to the previous block, as the current block isn't finalized yet

These restrictions ensure the program is safe to run on untrusted clients while still producing valid proofs.

### Smaller Footprint

By removing unused functionality, the Virtual OS has a significantly smaller bytecode (~30% smaller), which reduces proving time and costs for clients.

## Compilation

The crate uses a build script (`build/main.rs`) that compiles three programs in parallel:

1. **Starknet OS** (`starknet_os_bytes`) - The full OS program
2. **Aggregator** (`starknet_aggregator_bytes`) - For combining multiple block proofs
3. **Virtual OS** (`virtual_os_bytes`) - The single-transaction variant

### The `__virtual` File Convention

The Virtual OS is compiled from the same source tree as the full OS, but with certain files **swapped** at build time. Files ending in `__virtual.cairo` replace their corresponding non-virtual versions, for example:

| Virtual File | Replaces |
|--------------|----------|
| `execute_transactions_inner__virtual.cairo` | `execute_transactions_inner.cairo` |
| `execution_constraints__virtual.cairo`      | `execution_constraints.cairo` |
| `execute_syscalls__virtual.cairo`           | `execute_syscalls.cairo` |
| `entry_point_utils__virtual.cairo`          | `entry_point_utils.cairo` |
| `os_utils__virtual.cairo`                   | `os_utils.cairo` |

The build script:
1. Creates a temporary directory
2. Copies all Cairo files, replacing `X.cairo` with `X__virtual.cairo` where available
3. Compiles the Virtual OS from this modified source tree

This approach allows both programs to share most of the codebase while having different implementations for specific modules.

## Crate Contents

- `OS_PROGRAM` / `OS_PROGRAM_BYTES` - The compiled full Starknet OS
- `VIRTUAL_OS_PROGRAM` / `VIRTUAL_OS_PROGRAM_BYTES` - The compiled Virtual OS
- `AGGREGATOR_PROGRAM` / `AGGREGATOR_PROGRAM_BYTES` - The compiled aggregator
- `PROGRAM_HASHES` - Hashes of the compiled programs

