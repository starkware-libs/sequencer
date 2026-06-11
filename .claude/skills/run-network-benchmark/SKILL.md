---
name: run-network-benchmark
description: Run the apollo_network_benchmark P2P stress test end-to-end — gather parameters, start a local Docker or GKE cluster deployment, monitor it via Prometheus/Grafana, and summarize latency/throughput/delivery results. Use when the user says 'run the network benchmark', 'network stress test', 'apollo_network_benchmark', 'p2p benchmark', or wants to benchmark gossipsub/sqmr broadcasting.
---

# Run the apollo network benchmark

`apollo_network_benchmark` spins up N libp2p nodes that broadcast stress-test messages to each other and measures delivery latency, throughput, and message loss. Everything is driven by one orchestrator binary:

```bash
cargo run --release --bin apollo_network_benchmark_run -- <local|cluster> <start|stop|logs|port-forward> [flags]
```

- **local**: Docker Compose on this machine — one container per node plus Prometheus (:9090), Grafana (:3000), and cAdvisor (:8080). Best for quick runs.
- **cluster**: GKE on `sequencer-dev` (us-central1) using an Indexed Job on the `apollo-network-benchmark` node pool. Best for scale and isolated resources.

Crate: `crates/apollo_network_benchmark`. The orchestrator builds the Docker image, generates configs, and deploys — you never invoke the node binary directly.

## Step 1: Gather parameters

Ask ONE AskUserQuestion round. Skip any question the user already answered in their prompt — map their words onto flags using the reference table below. Everything not asked uses defaults.

1. **Where to run**: Local Docker (Recommended — quick, no cloud access needed) / GKE cluster (scale beyond local resources; needs gcloud + kubectl).
2. **Scenario** (protocol + broadcast mode):
   - Gossipsub, all nodes broadcast (Recommended — the default) → `--network-protocol gossipsub --mode all`
   - Gossipsub, single broadcaster → `--mode one` (broadcaster defaults to the last node; `--broadcaster <id>` to override)
   - Gossipsub, round-robin → `--mode rr` (nodes take turns every `--round-duration-seconds`, default 3s)
   - SQMR → `--network-protocol sqmr` (or `reversed-sqmr` via Other)
3. **Load** (nodes × message size @ heartbeat):
   - Default: 3 nodes × 1 KB @ 1000 ms (the binary defaults)
   - Medium: 5 nodes × 4 KB @ 200 ms
   - Heavy: 10 nodes × 16 KB @ 50 ms
4. **Duration**: 5 min smoke (`--timeout-seconds 300`) / 15 min (Recommended, `900`) / full default (`4000` ≈ 66 min) / custom. Always pass `--timeout-seconds` explicitly — the 66-minute default is a trap for interactive runs.

Then ask ONE short follow-up (multiSelect) — "Change any advanced defaults?":
- **None (Recommended)** — keep all defaults.
- **Network shaping** — add ingress `--latency` (ms) and/or `--throughput` (KB/s) constraints.
- **Buffers & logging** — `--buffer-size`, `--verbosity`.
- **Cluster tuning** (only offer for cluster runs) — `--node-pool-role`, `--node-toleration`, CPU/memory requests and limits, `--production-docker`.

Only for the selected categories, ask one more round for the concrete values (offer sensible presets, e.g. latency 50/100/200 ms, with Other for custom). Never ask for values in unselected categories — defaults apply, unless the user already gave values in their prompt.

### Flag reference

| Flag | Default | Meaning |
|---|---|---|
| `--num-nodes` | 3 | Number of nodes |
| `--mode` | `all` | `all` / `one` / `rr` |
| `--network-protocol` | `gossipsub` | `gossipsub` / `sqmr` / `reversed-sqmr` |
| `--broadcaster` | last node | Broadcasting node id (`one` mode; must be < num_nodes) |
| `--round-duration-seconds` | 3 | Turn length in `rr` mode |
| `--message-size-bytes` | 1024 | Message size (min 36 — metadata overhead) |
| `--heartbeat-millis` | 1000 | Sleep between broadcasts (must be > 0) |
| `--timeout-seconds` | 4000 | Run duration; nodes exit after this |
| `--buffer-size` | 100000 | Broadcast topic channel capacity |
| `--quic` | false | Format bootstrap multiaddrs as UDP/QUIC — does NOT switch the node's transport (see gotchas) |
| `--latency` | off | Added ingress latency, ms (tc netem) |
| `--throughput` | off | Ingress throughput cap, KB/s (tc htb) |
| `--image` | rebuild | Reuse a previously built image tag |
| `--memory-limit` | 3g | Local only: memory per container |
| `--node-pool-role` | apollo-network-benchmark | Cluster only: node pool selector |
| `--node-toleration` | off | Cluster only: `key=value` NoSchedule toleration |
| `--cpu-requests` / `--cpu-limits` | 7500m | Cluster only: per-pod CPU |
| `--memory-requests` / `--memory-limits` | 10Gi | Cluster only: per-pod memory |
| `--production-docker` | false | Cluster only: slow production image build |

## Step 2: Preflight

Local:
- `docker info` succeeds.
- Ports 3000, 9090, 8080, and the per-node ranges 2000+id / 10000+id are free (`ss -tlnp | grep -E ':(3000|9090|8080)'`).
- Sanity-check memory: num_nodes × 3g (or `--memory-limit`) must fit in RAM.

Cluster:
- `gcloud auth list` shows an active account and `kubectl config current-context` points at the `sequencer-dev` cluster. If not, ask the user to run `! gcloud auth login` and set the context themselves — never switch contexts for them.

Parameter guards (validate before starting):
- `one` mode: broadcaster id < num_nodes, otherwise no node ever broadcasts and the run times out with empty metrics.
- `rr` mode: relies on synchronized wall clocks across nodes; nodes **panic** on negative receive delay (clock skew). Fine on a single machine; on cluster, warn the user that NTP skew pollutes latency numbers.
- `--message-size-bytes` ≥ 36.

## Step 3: Start the benchmark

Run in the background (the first run builds a Docker image, which takes minutes):

```bash
cargo run --release --bin apollo_network_benchmark_run -- local start \
  --num-nodes <N> --mode <mode> --network-protocol <proto> \
  --message-size-bytes <B> --heartbeat-millis <ms> --timeout-seconds <s>
```

Cluster is identical with `cluster start` plus any cluster-only flags. Watch the output for the built image tag and successful container/pod startup; on error, surface it to the user and stop — do not retry blindly.

For back-to-back runs, pass `--image <tag>` with the tag from the previous run (printed at start, also recorded in the deployment metadata under `~/apollo_network_benchmark_local/` or `~/apollo_network_benchmark_cluster/`) to skip the rebuild.

## Step 4: Monitor

- Cluster only: start port-forwarding in the background first:
  ```bash
  cargo run --release --bin apollo_network_benchmark_run -- cluster port-forward
  ```
- Tell the user the dashboards are live: Grafana `http://localhost:3000`, Prometheus `http://localhost:9090`.
- Poll the Prometheus HTTP API every few minutes during the run:
  ```bash
  curl -s 'http://localhost:9090/api/v1/query' --data-urlencode 'query=network_connected_peers'
  curl -s 'http://localhost:9090/api/v1/query' --data-urlencode 'query=sum(receive_message_count)'
  ```
- Health checks (~2 min in): every node should report `network_connected_peers` = N-1, and `receive_message_count` should be increasing. If peers never connect or counts stay 0, the run is broken — fetch container/pod logs, report, and offer to stop.
- Report interim snapshots to the user; don't go silent for the whole run.

## Step 5: Summarize results

Query Prometheus **near the end of the run, before the timeout** — once nodes exit, scrape targets disappear and only stale data remains. Instant queries:

```bash
# Latency percentiles (seconds)
histogram_quantile(0.50, sum(rate(receive_message_delay_seconds_bucket[5m])) by (le))
histogram_quantile(0.95, sum(rate(receive_message_delay_seconds_bucket[5m])) by (le))
histogram_quantile(0.99, sum(rate(receive_message_delay_seconds_bucket[5m])) by (le))

# Actual vs theoretical throughput (bytes/s)
sum(rate(receive_message_bytes_sum[5m]))
sum(broadcast_message_theoretical_throughput)

# Delivery accounting
sum(broadcast_message_count)
sum(receive_message_count)
sum(receive_message_pending_count)
sum(network_dropped_broadcast_messages) by (drop_reason)
```

Report back a short table plus a plain-language verdict:
- The exact command run (so it's reproducible), node count, scenario, duration.
- Latency p50/p95/p99.
- Achieved vs theoretical throughput.
- Delivery: sent vs received (expected received = sent × (N-1) for broadcast in `all`/`one`/`rr` modes), pending count at end, drops by reason.
- Verdict: **healthy** (no drops, pending ≈ 0, latency stable), **lossy** (drops or growing pending), or **saturated** (achieved throughput plateaus below theoretical, latency climbing).

## Step 6: Cleanup

Ask the user before tearing down — they may want to keep dashboards up.

```bash
# Cluster: fetch logs first if wanted (saved to /tmp/broadcast-network-stress-test-<id>.logs.txt)
cargo run --release --bin apollo_network_benchmark_run -- cluster logs

cargo run --release --bin apollo_network_benchmark_run -- local stop
cargo run --release --bin apollo_network_benchmark_run -- cluster stop
```

Verify local cleanup with `docker ps` (no `broadcast-network-stress-test` containers left). Kill any background port-forward processes you started.

## Gotchas

- **66-minute default timeout**: always set `--timeout-seconds` explicitly for interactive runs.
- **Round-robin clock skew**: `rr` mode derives turn ownership from each node's wall clock, and nodes panic on negative receive delay. Single-machine local runs are safe; cluster runs need NTP-synced nodes.
- **Broadcaster out of range** (`one` mode) silently produces an empty run — the guard in Step 2 prevents this.
- **`--quic` does not enable QUIC**: it only changes the bootstrap multiaddr format to `/udp/<port>/quic-v1`; the actual transport is whatever the node binary's `NetworkManager` builds, which is currently TCP-only (`crates/apollo_network/src/network_manager/mod.rs`, `.with_tcp` + `TODO: .with_quic()`). Until QUIC lands there, passing `--quic` breaks peer bootstrapping — don't offer it.
- **Min message size is 36 bytes** (message metadata).
- **Port collisions**: the local stack claims 3000/9090/8080 plus 2000+id and 10000+id per node — stop other local Grafana/Prometheus stacks first.
- **Cluster scheduling failures**: pods need the `apollo-network-benchmark` node pool; if pods stay Pending, check the pool exists and consider `--node-toleration` or lower `--cpu-requests`/`--memory-requests`.
- **`--production-docker`** is a slow multi-stage build — only for production-fidelity measurements; the default fast dev build is right for iteration.
