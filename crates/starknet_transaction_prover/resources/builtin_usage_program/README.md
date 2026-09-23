# Builtin usage program

A Cairo 0 program used by `src/proving/prover_test.rs` to test which builtins the upstream privacy
provers support. The tests execute the compiled program with `cairo-vm` to produce a Cairo PIE, so
every builtin instance counter comes from a real execution.

The program declares every builtin of the `all_cairo` layout. Every execution uses the builtins
that the small privacy prover (`privacy_recursive_prove`) supports: output, pedersen, range_check,
bitwise, ec_op, keccak and poseidon. The `selected_builtin` program input selects one builtin that
the small prover does not support, which the execution also uses, once:

| `selected_builtin` | Unsupported builtin used once |
| -----------------: | ----------------------------- |
|                `0` | None                          |
|                `1` | `ecdsa`                       |
|                `2` | `range_check96`               |
|                `3` | `add_mod`                     |
|                `4` | `mul_mod`                     |

The small prover verifies the Cairo proof in a circuit built for a fixed set of prover components,
and it fails on any execution that enables a different set. The execution without a selected
builtin enables exactly that set, so it proves successfully. The small prover runs the privacy
bootloader in a layout without the ecdsa, ec_op and keccak builtins. The bootloader verifies the
PIE's ec_op and keccak instances with Cairo code, so their usage adds no prover component, and it
fails to load a PIE that uses ecdsa. Each other selected builtin adds its own prover component to
that set.

The program writes the ec_op, keccak, range_check96 and mod builtin instances directly, so that
each execution uses exactly the intended builtins. Library code does not guarantee that: for
example, the `core::circuit` `add` gates also use `mul_mod` to reduce their inputs.

The tests pass the input through a hint that they register with `cairo-vm`
(`SELECTED_BUILTIN_HINT`), since `cairo-vm` does not read `program_input`.

## Regenerating

Compile with the `cairo-lang` version pinned in `scripts/requirements.txt`, with the Python
environment from `scripts/requirements.txt` active:

```bash
cd crates/starknet_transaction_prover/resources/builtin_usage_program
cairo-compile builtin_usage.cairo --output builtin_usage_compiled.json --no_debug_info
```
