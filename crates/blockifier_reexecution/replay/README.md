# RPC Replay

Continuously reexecutes blocks via RPC with Cairo Native enabled on all contracts,
comparing the resulting state diffs against the original chain to verify correctness.

## CLI Usage

```bash
cargo run --release -p blockifier_reexecution -- \
  rpc-replay \
  -n <RPC_URL> \
  -c <CHAIN_ID> \
  --start-block <BLOCK_NUMBER> \
  --end-block <BLOCK_NUMBER> \   # optional, omit to run forever
  --parallelism <N>              # default: 1
```

### Parameters

| Parameter       | Description                                                    |
|-----------------|----------------------------------------------------------------|
| `-n`            | RPC endpoint URL (e.g. `http://juno:6060`)                     |
| `-c`            | Chain ID: `testnet`, `mainnet`, or `integration`               |
| `--start-block` | First block to reexecute                                       |
| `--end-block`   | Last block (inclusive). Omit to run indefinitely                |
| `--parallelism` | Number of parallel worker threads (default: 1)                 |

### Native Compilation

Cairo Native is controlled at build time via the `cairo_native` feature flag.
When enabled, all Sierra contracts are compiled to native before execution.
The `ContractClassManager` is shared across all workers, so native compilations
are cached and reused across blocks.

## Cloud Deployment

### Build and push the Docker image

From the repo root:

```bash
docker build -f crates/blockifier_reexecution/replay/Dockerfile -t blockifier-reexecution .
docker tag blockifier-reexecution us-central1-docker.pkg.dev/starkware-dev/sequencer/blockifier-reexecution:<TAG>
docker push us-central1-docker.pkg.dev/starkware-dev/sequencer/blockifier-reexecution:<TAG>
```

### Deploy to Kubernetes

1. Copy `job.yaml` and update it for your environment:
   - Set `image` to your registry image
   - Set `RPC_URL` to a fullnode RPC in the same cluster (e.g. `http://pathfinder.starknet-testnet-full-nodes:9545/rpc/v0_10`)
   - Set `CHAIN_ID` to `testnet`, `mainnet`, or `integration`
   - Set `START_BLOCK` to the block you want to start from
   - Set `END_BLOCK` or leave empty to run forever
   - Set `PARALLELISM` based on available resources

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

| Env Var       | Description                              | Default              |
|---------------|------------------------------------------|----------------------|
| `RPC_URL`     | Starknet RPC endpoint                    | `http://juno:6060`   |
| `CHAIN_ID`    | Chain identifier                         | `testnet`            |
| `START_BLOCK` | First block to replay                    | `800000`             |
| `END_BLOCK`   | Last block (empty = run forever)         | (empty)              |
| `PARALLELISM` | Number of worker threads                 | `4`                  |

### Resource Sizing

- The shared `ContractClassManager` caches compiled native classes across all
  workers, so memory grows with the number of unique contracts encountered.
- CPU and memory scale with `PARALLELISM` — tune based on observed usage.
- The RPC fullnode should be in the same cluster to avoid network latency.

### Output

- `stdout`: per-block pass/fail messages (e.g. `Block 800000 passed.`)
- `stderr`: detailed state diff mismatches with colored diffs, and error messages

On mismatch, the full expected vs actual state diff is printed to stderr.
