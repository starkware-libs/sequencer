# Proof Flow Fixture Files

`proof.bin` and `proof_facts.json` are committed ZK proof fixtures used by
`integration_test_proof_flow`. They must be regenerated whenever the chain
configuration changes (chain ID, STRK fee token address, or OS version).

## How to regenerate

Use the convenience script that coordinates both steps:

```bash
./scripts/generate_proof_flow_fixtures.sh
```

Or run each step manually:

### Step 1 — Generate a CairoPie (stable toolchain)

Runs a `balanceOf` invoke transaction through the virtual OS and writes a CairoPie to disk:

```bash
CAIRO_PIE_PATH=/tmp/proof_flow_cairo_pie.zip \
cargo test -p starknet_os_flow_tests generate_cairo_pie -- --ignored --nocapture
```

### Step 2 — Prove the CairoPie (nightly toolchain, ~5–10 minutes)

Reads the CairoPie, proves it with stwo, and writes `proof.bin` and `proof_facts.json`
to this directory:

```bash
CAIRO_PIE_PATH=/tmp/proof_flow_cairo_pie.zip \
cargo +nightly-2025-07-14 run \
    --features stwo_proving \
    --bin generate_proof_flow_fixtures \
    -p starknet_transaction_prover
```

### Step 3 — Commit the updated fixtures

```bash
git add crates/apollo_integration_tests/resources/proof_flow/proof.bin \
        crates/apollo_integration_tests/resources/proof_flow/proof_facts.json
git commit -m "apollo_integration_tests: regenerate proof flow fixtures"
```

## Requirements

- nightly-2025-07-14 Rust toolchain (for the proving step only):
  ```bash
  rustup toolchain install nightly-2025-07-14
  ```

## Environment variables

| Variable          | Default                          | Description                        |
|-------------------|----------------------------------|------------------------------------|
| `CAIRO_PIE_PATH`  | `/tmp/proof_flow_cairo_pie.zip`  | Path for the CairoPie zip file     |

## When to regenerate

- Chain ID changes
- STRK fee token address changes
- OS `config_hash` version changes
- OS program or VM version upgrade that breaks proof verification
