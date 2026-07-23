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
   - Gossipsub, single broadcaster → `--mode one` (you MUST also ask for `--broadcaster <id>`, id < num_nodes, suggest `num_nodes - 1`; the orchestrator rejects the command at parse without it)
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
| `--broadcaster` | required for `one` | Broadcasting node id; required at CLI parse when `--mode one`, must be < num_nodes |
| `--round-duration-seconds` | 3 | Turn length in `rr` mode |
| `--message-size-bytes` | 1024 | Message size (min 36 — metadata overhead) |
| `--heartbeat-millis` | 1000 | Sleep between broadcasts (must be > 0) |
| `--timeout-seconds` | 4000 | Run duration; nodes exit after this |
| `--buffer-size` | 100000 | Broadcast topic channel capacity |
| `--quic` | false | Format bootstrap multiaddrs as UDP/QUIC — does NOT switch the node's transport (see gotchas) |
| `--latency` | off | Added ingress latency, ms (tc netem) |
| `--throughput` | off | Ingress throughput cap, KB/s (tc htb) |
| `--image` | rebuild | Reuse a prebuilt image — local: bare docker tag; cluster: full registry ref (see Step 3) |
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
- Verify a **live** gcloud token, not just a listed account — `gcloud auth print-access-token` must succeed. An account shown as active by `gcloud auth list` can still have an expired token that fails reauth headlessly, so the listing check alone is not enough.
- Confirm `kubectl config current-context` points at the `sequencer-dev` cluster and a trivial authed call works (`kubectl get ns`).
- If the token is expired / reauth is required, or the context is wrong, ask the user to run `! gcloud auth login` and set the context themselves — never re-auth or switch contexts for them. Do this **before** `cluster start`: otherwise a 5–15 min image build runs before the dead token surfaces at the `docker push` / deploy step.

Parameter guards (validate before starting):
- `one` mode: `--broadcaster` is required (the orchestrator rejects the command at parse without it) and its id must be < num_nodes.
- **Clock skew (ALL modes on cluster)**: every received message computes `receive_time - send_time`, and the node **panics** if that's negative — i.e. if the sender's clock is even microseconds ahead (`handlers.rs`). This is mode-independent: `all`/`one`/`rr` all hit it. Single-machine local runs are safe (one clock); on a cluster, nodes crash unless tightly NTP-synced — a real run was killed by 17µs of skew. Warn the user, and treat crashlooping nodes with `clock skew detected` in their logs as this issue, not a benchmark misconfig.
- `rr` mode additionally derives turn ownership from each node's wall clock, so skew also corrupts the round schedule (on top of the panic above).
- `--message-size-bytes` ≥ 36.

## Step 3: Start the benchmark

Run in the background (the first run builds a Docker image, which takes minutes):

```bash
cargo run --release --bin apollo_network_benchmark_run -- local start \
  --num-nodes <N> --mode <mode> --network-protocol <proto> \
  --message-size-bytes <B> --heartbeat-millis <ms> --timeout-seconds <s>
```

For `--mode one`, append `--broadcaster <id>` (id < num_nodes) — the orchestrator fails at CLI parse without it.

Cluster is identical with `cluster start` plus any cluster-only flags. Watch the output for the built image tag and successful container/pod startup; on error, surface it to the user and stop — do not retry blindly.

For back-to-back runs, pass `--image <ref>` to skip the rebuild, using the exact string printed as `Image tag:` on the previous run (also recorded in the deployment metadata under `~/apollo_network_benchmark_local/` or `~/apollo_network_benchmark_cluster/`). The reference differs by mode: **local** takes the bare local docker tag (`broadcast-network-stress-test-node:<timestamp>`), while **cluster** requires the full registry reference (`us-central1-docker.pkg.dev/starkware-dev/sequencer/broadcast-network-stress-test-node:<timestamp>`) — a bare tag fails the cluster image verification and push.

## Step 4: Monitor

- Cluster only: start port-forwarding in the background first:
  ```bash
  cargo run --release --bin apollo_network_benchmark_run -- cluster port-forward
  ```
  If it exits immediately, the Prometheus/Grafana pods aren't `Running` yet (no service endpoints) — wait for them and retry: `kubectl rollout status statefulset/prometheus -n <ns>`.
- **Sleepod auto-sleep**: on sequencer-dev a `sleepod` controller sleeps StatefulSets nightly (20:00–05:00 UTC) by default. `cluster start` opts the run's namespace out with a `SleepPolicy`, so the dashboards stay up at any run time. If they get napped anyway (scaled to 0, `sleepod.io/original-replicas` annotation present), the opt-out didn't apply — re-apply the policy, or as a stopgap `kubectl scale statefulset/prometheus statefulset/grafana --replicas=1 -n <ns>` (note this can be re-slept during the night window).
- Tell the user the dashboards are live: Grafana `http://localhost:3000`, Prometheus `http://localhost:9090`.
- Poll the Prometheus HTTP API every few minutes during the run:
  ```bash
  curl -s 'http://localhost:9090/api/v1/query' --data-urlencode 'query=network_connected_peers'
  curl -s 'http://localhost:9090/api/v1/query' --data-urlencode 'query=sum(receive_message_count)'
  ```
- Health checks (~2 min in): every node should report `network_connected_peers` = N-1, and `receive_message_count` should be increasing. If peers never connect or counts stay 0, the run is broken — fetch container/pod logs, report, and offer to stop.
- Report interim snapshots to the user; don't go silent for the whole run.
- **Fallback if central Prometheus is unavailable** (cluster): the per-node `/metrics` endpoints vanish the instant the Job completes, so anything not scraped before the timeout is lost. If the central Prometheus/Grafana pods are down, port-forward each node pod and scrape its exporter directly during the run, e.g. `kubectl -n <ns> port-forward pod/broadcast-network-stress-test-<id> 2000:2000 && curl -s localhost:2000/metrics`. Always capture a final snapshot before the timeout regardless of which path you use.

## Step 5: Summarize results

Query Prometheus **near the end of the run, before the timeout** — once nodes exit, scrape targets disappear and only stale data remains. Instant queries:

```bash
# Latency percentiles (seconds). The node exporter renders these as Prometheus SUMMARIES
# (quantile labels), not histograms — there are no `_bucket` series, so histogram_quantile()
# returns empty. Query the quantile labels directly, matching the generated Grafana dashboard.
receive_message_delay_seconds{quantile="0.5"}
receive_message_delay_seconds{quantile="0.95"}
receive_message_delay_seconds{quantile="0.99"}
# Aggregate across nodes:
avg(receive_message_delay_seconds) by (quantile)

# Send-side achieved vs theoretical throughput (bytes/s) — like-for-like, this is the saturation signal
sum(rate(broadcast_message_bytes_sum[5m]))
sum(broadcast_message_theoretical_throughput)

# Receive-side aggregate throughput (bytes/s) — counts every delivered copy, expect ≈ send rate × (N-1)
sum(rate(receive_message_bytes_sum[5m]))

# Delivery accounting
sum(broadcast_message_count)
sum(receive_message_count)
sum(receive_message_pending_count)
sum(network_dropped_broadcast_messages) by (drop_reason)
```

Report back a short table plus a plain-language verdict:
- The exact command run (so it's reproducible), node count, scenario, duration.
- Latency p50/p95/p99.
- Throughput: send-side achieved vs theoretical, and receive-side aggregate vs expected (send rate × (N-1)).
- Delivery: sent vs received (expected received = sent × (N-1) for broadcast in `all`/`one`/`rr` modes), pending count at end, drops by reason.
- Verdict: **healthy** (no drops, pending ≈ 0, latency stable), **lossy** (drops or growing pending), or **saturated** (send-side achieved throughput plateaus below theoretical, latency climbing).

## Step 6: Cleanup

Ask the user before tearing down — they may want to keep dashboards up. Run only the commands for the deployment mode chosen in Step 1; the other mode's commands error when no such deployment exists.

Local:
```bash
cargo run --release --bin apollo_network_benchmark_run -- local stop
```
Verify cleanup with `docker ps` (no `broadcast-network-stress-test` containers left).

Cluster:
```bash
# Fetch logs first if wanted (saved to /tmp/broadcast-network-stress-test-<id>.logs.txt)
cargo run --release --bin apollo_network_benchmark_run -- cluster logs

cargo run --release --bin apollo_network_benchmark_run -- cluster stop
```
Kill any background port-forward processes you started.

## Gotchas

- **66-minute default timeout**: always set `--timeout-seconds` explicitly for interactive runs.
- **Clock-skew panic on cluster (any mode)**: the receive path panics on a negative `receive_time - send_time`, so any node whose clock trails a sender's by even microseconds crashes the run — observed killing a `--mode all` cluster run on 17µs of skew. Local single-machine runs are safe; cluster runs need tightly NTP-synced nodes (or a tolerance change in the node code). `rr` mode is extra-sensitive since it also keys turn ownership off the wall clock.
- **`--broadcaster` is mandatory for `--mode one`**: the orchestrator flattens `UserArgs`, whose `broadcaster` has `required_if_eq("mode", "one")`, so the command fails at CLI parse without it (the `num_nodes - 1` fallback in `get_env_var_pairs` is unreachable via the CLI). It also errors if the id is ≥ num_nodes.
- **`--quic` does not enable QUIC**: it only changes the bootstrap multiaddr format to `/udp/<port>/quic-v1`; the actual transport is whatever the node binary's `NetworkManager` builds, which is currently TCP-only (`crates/apollo_network/src/network_manager/mod.rs`, `.with_tcp` + `TODO: .with_quic()`). Until QUIC lands there, passing `--quic` breaks peer bootstrapping — don't offer it.
- **Min message size is 36 bytes** (message metadata).
- **Port collisions**: the local stack claims 3000/9090/8080 plus 2000+id and 10000+id per node — stop other local Grafana/Prometheus stacks first.
- **Cluster scheduling failures**: pods need the `apollo-network-benchmark` node pool; if pods stay Pending, check the pool exists and consider `--node-toleration` or lower `--cpu-requests`/`--memory-requests`.
- **`--production-docker`** is a slow multi-stage build — only for production-fidelity measurements; the default fast dev build is right for iteration.
