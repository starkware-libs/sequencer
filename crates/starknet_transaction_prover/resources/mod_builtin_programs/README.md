# Mod-builtin test programs

Cairo 0 programs used by `src/proving/prover_test.rs` to test how the upstream privacy provers
handle the `add_mod` and `mul_mod` builtins. The tests execute each compiled program with
`cairo-vm` to produce a Cairo PIE, so every builtin instance counter comes from a real execution.

All three programs declare `output pedersen range_check bitwise poseidon add_mod mul_mod`, use the
non-mod builtins identically, and differ only in which mod builtin they use:

| Program           | `add_mod` instances | `mul_mod` instances |
| ----------------- | ------------------: | ------------------: |
| `no_mod_builtin`  |                   0 |                   0 |
| `add_mod_builtin` |                   1 |                   0 |
| `mul_mod_builtin` |                   0 |                   1 |

The small privacy prover (`privacy_recursive_prove`) verifies the Cairo proof in a circuit built for
a fixed set of prover components, and it fails on any execution that enables a different set. The
non-mod builtin usage makes the `no_mod_builtin` execution enable exactly that set, so it proves
successfully, and each mod-builtin program enables that set plus its mod-builtin component.

The mod-builtin programs write the builtin instance directly instead of going through
`core::circuit`, whose `add` gates also use `mul_mod` to reduce their inputs.

## Regenerating

Compile with the `cairo-lang` version pinned in `scripts/requirements.txt`, with the Python
environment from `scripts/requirements.txt` active:

```bash
cd crates/starknet_transaction_prover/resources/mod_builtin_programs
for program in no_mod_builtin add_mod_builtin mul_mod_builtin; do
    cairo-compile "${program}.cairo" --output "${program}_compiled.json" --no_debug_info
done
```
