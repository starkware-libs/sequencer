# RPC Replay

Continuously reexecutes blocks fetched via RPC and compares the resulting state
diffs to verify correctness. Supports two modes:

- **Standard** (default): reexecutes each block once and compares the actual
  state diff against the expected one from the chain.
- **Compare-native** (`--compare-native`): reexecutes each block twice — once
  with Cairo Native and once with CASM — and compares the two state diffs
  against each other. Requires the `cairo_native` feature.

## Running locally

All commands must be run from the **repository root**.

### Standard mode

```bash
cargo run --release -p blockifier_reexecution -- \
  rpc-replay \
  -n <RPC_URL> \
  -c <CHAIN_ID> \
  --start-block <BLOCK_NUMBER> \
  --end-block <BLOCK_NUMBER> \   # optional, omit to run forever
  --n-workers <N>                # default: 1
```

### Compare-native mode

Requires building with the `cairo_native` feature:

```bash
cargo run --release -p blockifier_reexecution --features cairo_native -- \
  rpc-replay \
  -n <RPC_URL> \
  -c <CHAIN_ID> \
  --start-block <BLOCK_NUMBER> \
  --compare-native \
  --n-workers <N>
```

### Parameters

| Parameter          | Description                                              |
|--------------------|----------------------------------------------------------|
| `-n`               | RPC endpoint URL (e.g. `http://juno:6060`)               |
| `-c`               | Chain ID: `testnet`, `mainnet`, or `integration`         |
| `--start-block`    | First block to reexecute                                 |
| `--end-block`      | Last block (inclusive). Omit to run indefinitely          |
| `--n-workers`      | Number of parallel worker threads (default: 1)           |
| `--compare-native` | Run native-vs-CASM comparison (requires `cairo_native`)  |
| `--prefetch-initial-reads` | Prefetch initial reads before execution with `starknet_simulateTransactions` (default: `true`) |

## Cloud Deployment

The Docker image is built **with `cairo_native` enabled**, so both standard and
compare-native modes are available inside the container.

### Docker image

Pre-built images are published to GHCR on version tags by the
`Blockifier-Reexecution-Docker-Publish` workflow:
`ghcr.io/starkware-libs/sequencer/blockifier-reexecution:<tag>`

### Deploy to Kubernetes

1. Copy `job.yaml` and update it for your environment:
   - Set `image` to the GHCR image tag.
   - Set `RPC_URL` to a fullnode RPC in the same cluster.
   - Set `CHAIN_ID` to `testnet`, `mainnet`, or `integration`.
   - Set `START_BLOCK` to the block you want to start from.
   - Set `END_BLOCK` or leave empty to run forever.
   - Set `N_WORKERS` based on available resources.
   - Set `COMPARE_NATIVE` to `true` to enable native-vs-CASM comparison.

2. Create a namespace and deploy:

```bash
kubectl create namespace rpc-replay
kubectl apply -f job.yaml -n rpc-replay
```

3. Monitor progress:

```bash
kubectl logs -f job/blockifier-reexecution -n rpc-replay
```

### Configuration

All parameters are configured via environment variables in `job.yaml`:

| Env Var          | Description                              | Default            |
|------------------|------------------------------------------|--------------------|
| `RPC_URL`        | Starknet RPC endpoint                    | `http://juno:6060` |
| `CHAIN_ID`       | Chain identifier                         | `testnet`          |
| `START_BLOCK`    | First block to replay                    | `800000`           |
| `END_BLOCK`      | Last block (empty = run forever)         | (empty)            |
| `N_WORKERS`      | Number of worker threads                 | `16`               |
| `COMPARE_NATIVE` | Enable native-vs-CASM comparison         | (empty/disabled)   |
| `PREFETCH_INITIAL_READS` | Prefetch initial reads via simulate (`true`/`false`) | `true` |

### Resource Sizing

- CPU and memory scale with `N_WORKERS` — tune based on observed usage.
- In compare-native mode, each block is executed twice, so expect roughly
  double the CPU usage compared to standard mode.
- The shared `ContractClassManager` caches compiled native classes across all
  workers, so memory grows with the number of unique contracts encountered.
- The RPC fullnode should be in the same cluster to avoid network latency.

### Output

All output goes to stdout via structured logging (tracing). Per-block
pass/fail messages appear at `INFO` level, state diff mismatches at `WARN`
level with colored diffs, and errors at `ERROR` level.

Override the log level with the `RUST_LOG` environment variable (e.g.
`RUST_LOG=debug`). The default level is `info` for `blockifier` and
`blockifier_reexecution`, `warn` for everything else.
