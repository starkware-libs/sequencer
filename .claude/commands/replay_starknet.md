# Replay

Guide the user through running a blockifier reexecution replay job. Follow the steps below in order.

## Background

The replay tool continuously reexecutes Starknet blocks fetched via RPC and compares the resulting state diffs to verify correctness.

**Modes:**
- **Standard** (default): Reexecutes each block once and compares the resulting state diff against the expected one from the chain. Good for general correctness checking.
- **Compare-native**: Reexecutes each block twice — once with Cairo Native and once with CASM — and compares the two state diffs against each other. Use this to verify that Cairo Native execution matches CASM. Requires roughly 2x the CPU of standard mode.

**Deployment options:**
- **Cloud (Kubernetes)**: Runs as a Kubernetes Job. Recommended for long-running or production runs. For best performance, deploy in the **same cluster as your fullnode** — this avoids cross-cluster network latency on every block fetch.
- **Local**: Runs directly on this machine with cargo. Good for short tests or quick debugging. Requires a locally accessible RPC endpoint and a long build time.

**Output:** All output goes to stdout via structured logging. Per-block pass/fail messages appear at `INFO` level, state diff mismatches at `WARN` level with colored diffs, and errors at `ERROR` level. Override with `RUST_LOG=<level>`.

---

## Step 1: Ask about mode

Present the two modes as described above and ask:

> Which mode do you want to use — **standard** or **compare-native**?

---

## Step 2: Ask for core configuration

After the user answers, ask for the following (present as a numbered list):

1. **Chain**: `testnet`, `mainnet`, or `integration`?
2. **Start block**: Block number to begin from (e.g. `800000`)
3. **End block**: Last block (inclusive). Leave blank to run indefinitely.
4. **Workers**: Number of parallel worker threads. Higher = faster but uses more CPU and memory. In compare-native mode, each block is executed twice so CPU usage is roughly doubled.
   - Suggested default for **local**: `1`
   - Suggested default for **cloud**: `16`

---

## Step 3: Ask about deployment target

Present both options with a clear recommendation:

> **Where do you want to run this?**
>
> - **Cloud (Kubernetes)** *(recommended for long or production runs)* — Deploys as a Kubernetes Job. **For best performance, deploy in the same cluster as your fullnode** to avoid network latency on every block fetch.
> - **Local** — Runs on this machine with `cargo run`. Good for short tests. Note: the first build can take a long time.

---

## Step 4: Ask for deployment-specific details

### If cloud:

Ask:

> A few more details for cloud deployment:
>
> 5. **RPC URL**: Full URL of your Starknet fullnode RPC (e.g. `http://juno:6060`). If deploying to Kubernetes, use the in-cluster service URL for best performance.
> 6. **Docker image**: Pre-built images are published to GHCR on version tags by CI (`ghcr.io/starkware-libs/sequencer/blockifier-reexecution:<TAG>`).
>    - **Use a release tag** (recommended): Provide the tag (e.g. `v1.2.3`).
>    - **Build and push manually**: If you need a custom image (e.g. from a feature branch), Claude will build from the current repo state and push to the registry.
> 7. **Namespace**: Kubernetes namespace to deploy into (default: `rpc-replay`)

### If local:

Ask:

> 5. **RPC URL**: Full URL of your Starknet fullnode RPC endpoint, accessible from this machine (e.g. `http://localhost:6060`)

---

## Step 5: Execute

### Local

Run from the repo root. Only add `--features cairo_native` for compare-native mode. Only add `--end-block` if the user provided one. Only add `--compare-native` for compare-native mode.

```bash
cargo run --release -p blockifier_reexecution [--features cairo_native] -- \
  rpc-replay \
  -n <RPC_URL> \
  -c <CHAIN_ID> \
  --start-block <START_BLOCK> \
  [--end-block <END_BLOCK>] \
  --n-workers <N_WORKERS> \
  [--compare-native]
```

### Cloud

**If the user chose to build and push manually** (run from repo root):

```bash
docker build -f crates/blockifier_reexecution/replay/Dockerfile -t ghcr.io/starkware-libs/sequencer/blockifier-reexecution:<TAG> .
docker push ghcr.io/starkware-libs/sequencer/blockifier-reexecution:<TAG>
```

The Docker image always includes the `cairo_native` feature, so both modes are available in the container.

**Generate and apply the Kubernetes Job:**

Write a filled-in job manifest to `/tmp/replay-job.yaml` based on the template at `crates/blockifier_reexecution/replay/job.yaml`, substituting:
- `IMAGE_PLACEHOLDER` → `ghcr.io/starkware-libs/sequencer/blockifier-reexecution:<TAG>`
- `RPC_URL` value → user's RPC URL
- `CHAIN_ID` value → user's chain
- `START_BLOCK` value → user's start block
- `END_BLOCK` value → user's end block (empty string if not provided)
- `N_WORKERS` value → user's worker count
- `COMPARE_NATIVE` value → `"true"` for compare-native mode, `""` otherwise

Then run:

```bash
# Create namespace (idempotent)
kubectl create namespace <NAMESPACE> --dry-run=client -o yaml | kubectl apply -f -

# Deploy the job
kubectl apply -f /tmp/replay-job.yaml -n <NAMESPACE>
```

After deploying, show the user the command to monitor progress:

```bash
kubectl logs -f job/blockifier-reexecution -n <NAMESPACE>
```
