# Apollo Network Benchmark

A comprehensive network stress testing and benchmarking tool for evaluating the performance, throughput, and reliability of Apollo's peer-to-peer networking layer.

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Binary: apollo_network_benchmark_node](#binary-apollo_network_benchmark_node)
- [Operating Modes](#operating-modes)
- [Network Protocols](#network-protocols)
- [Metrics and Monitoring](#metrics-and-monitoring)
- [Deployment Options](#deployment-options)
- [Usage Examples](#usage-examples)
- [Configuration Reference](#configuration-reference)

## Overview

The `apollo_network_benchmark` crate provides tools to stress test and benchmark Apollo's network layer under various conditions. It simulates realistic network workloads by running multiple nodes that communicate using different protocols and broadcast patterns.

### Key Features

- **Multiple Broadcasting Modes**: Test different message propagation patterns (all-to-all, single broadcaster, round-robin)
- **Protocol Flexibility**: Support for Gossipsub, SQMR (Sequential Query Message Response), and Reversed SQMR
- **Comprehensive Metrics**: Real-time monitoring of throughput, latency, CPU, memory, and network statistics
- **Integrated Observability**: Built-in Prometheus and Grafana support for visualization
- **Flexible Deployment**: Run locally or deploy to Kubernetes with traffic shaping capabilities

## Architecture

### Components

```
┌─────────────────────────────────────────────────────────────┐
│                  BroadcastNetworkStressTestNode             │
├─────────────────────────────────────────────────────────────┤
│  ┌──────────────────┐    ┌──────────────────┐               │
│  │  Network Manager │    │  Message Sender  │               │
│  │  - P2P Network   │◄───┤  - Gossipsub     │               │
│  │  - Protocol Mux  │    │  - SQMR          │               │
│  └──────────────────┘    │  - Reversed SQMR │               │
│          │               └──────────────────┘               │
│          │                                                  │
│          ▼                                                  │
│  ┌──────────────────┐    ┌──────────────────┐               │
│  │ Message Receiver │    │ Metrics Monitor  │               │
│  │ - Track indices  │    │ - Prometheus     │               │
│  │ - Detect gaps    │    │ - Process stats  │               │
│  └──────────────────┘    └──────────────────┘               │
└─────────────────────────────────────────────────────────────┘
```

### Core Modules (apollo_network_benchmark_node)

- **`stress_test_node.rs`**: Main orchestrator that manages network lifecycle and task coordination
- **`handlers.rs`**: Message sending and receiving logic with metrics collection
- **`protocol.rs`**: Protocol abstraction layer (Gossipsub, SQMR, Reversed SQMR)
- **`message.rs`**: Message serialization/deserialization with metadata
- **`metrics.rs`**: Metrics definitions and collection
- **`message_index_detector.rs`**: Tracks message indices and detects gaps

## Binary: apollo_network_benchmark_node

The main binary creates a network node that can broadcast and receive stress test messages using various protocols and patterns.

### Building

```bash
# Build with tokio metrics support
RUSTFLAGS="--cfg tokio_unstable" cargo build --release --bin apollo_network_benchmark_node

# The binary will be located at:
# target/release/apollo_network_benchmark_node
```

### Basic Usage

```bash
./apollo_network_benchmark_node \
  --id 0 \
  --num-nodes 3 \
  --metric-port 2000 \
  --p2p-port 10000 \
  --bootstrap "/ip4/127.0.0.1/tcp/10001/p2p/..." \
  --mode all \
  --network-protocol gossipsub \
  --message-size-bytes 1024 \
  --heartbeat-millis 1000 \
  --buffer-size 100000 \
  --timeout 3600 \
  --verbosity 3
```

## Operating Modes

The stress test supports three operating modes:

### 1. All Broadcast Mode (`--mode all`)

All nodes continuously broadcast messages to the network.

**Use Case**: Maximum stress testing, evaluating network capacity under full load

```bash
--mode all \
--message-size-bytes 4096 \
--heartbeat-millis 100
```

### 2. One Broadcast Mode (`--mode one`)

Only a single designated node broadcasts messages; all others receive.

**Use Case**: Testing single-source propagation, measuring message delivery consistency

```bash
--mode one \
--broadcaster 0 \
--message-size-bytes 8192 \
--heartbeat-millis 50
```

### 3. Round Robin Mode (`--mode rr`)

Nodes take turns broadcasting in a round-robin fashion.

**Use Case**: Simulating alternating message sources, testing fairness

```bash
--mode rr \
--round-duration-seconds 10 \
--message-size-bytes 2048 \
--heartbeat-millis 200
```

## Network Protocols

### Gossipsub (`--network-protocol gossipsub`)

Pub/sub protocol using libp2p's Gossipsub for message propagation.

**Characteristics**:
- Broadcast to all subscribed peers
- Epidemic message propagation
- Built-in message deduplication
- Best for: Many-to-many communication

### SQMR (`--network-protocol sqmr`)

Sequential Query Message Response - request/response pattern.

**Characteristics**:
- Point-to-point communication
- Query-response model
- Session-based
- Best for: One-to-many with responses

### Reversed SQMR (`--network-protocol reversed-sqmr`)

Inverted SQMR where receivers initiate connections to broadcasters.

**Characteristics**:
- Receivers pull messages from senders
- Persistent streaming connections
- Sender maintains active query state
- Best for: Testing pull-based architectures

## Metrics and Monitoring

### Exposed Metrics

The node exposes Prometheus metrics on the configured metric port (default: 2000):

#### Broadcast Metrics
- `broadcast_message_count`: Total messages sent
- `broadcast_message_bytes`: Size of current message
- `broadcast_message_bytes_sum`: Total bytes sent
- `broadcast_message_throughput`: Theoretical throughput (bytes/sec)
- `broadcast_message_send_delay_seconds`: Time to send message

#### Receive Metrics
- `receive_message_count`: Total messages received
- `receive_message_bytes`: Size of current message
- `receive_message_bytes_sum`: Total bytes received
- `receive_message_delay_seconds`: End-to-end latency
- `receive_message_pending_count`: Messages expected but not yet received

#### Network Metrics
- `network_connected_peers`: Number of connected peers
- `network_blacklisted_peers`: Number of blacklisted peers
- `network_active_inbound_sessions`: Active SQMR inbound sessions
- `network_active_outbound_sessions`: Active SQMR outbound sessions
- `network_dropped_broadcast_messages`: Dropped messages by reason
- `ping_latency_seconds`: Peer-to-peer ping latency

#### System Metrics (via node_exporter)
- `node_memory_MemTotal_bytes` / `node_memory_MemAvailable_bytes`: Memory usage
- `node_cpu_seconds_total`: CPU usage
- `node_network_transmit_bytes_total` / `node_network_receive_bytes_total`: Network I/O
- `node_netstat_Tcp_RetransSegs` / `node_netstat_Tcp_OutSegs`: TCP retransmission

### Grafana Dashboard

The deployment includes a pre-configured Grafana dashboard accessible at `http://localhost:3000` (local) or via port-forward (Kubernetes) that visualizes:

- Real-time throughput graphs
- Message latency distributions
- Network health indicators
- System resource utilization
- Per-node metrics comparison

## Deployment Options

### Local Deployment

Run multiple nodes locally with Docker Compose, integrated Prometheus and Grafana:

```bash
# Run 5 nodes locally
cargo run --release --bin apollo_network_benchmark_run -- local start \
  --num-nodes 5 \
  --mode all \
  --network-protocol gossipsub \
  --message-size-bytes 4096 \
  --heartbeat-millis 100 \
  --verbosity 3

# Run with network throttling
cargo run --release --bin apollo_network_benchmark_run -- local start \
  --num-nodes 3 \
  --latency 50 \
  --throughput 10000 \
  --mode all

# Stop local deployment
cargo run --release --bin apollo_network_benchmark_run -- local stop
```

**Local Deployment Features**:
- Automatic compilation and Docker image building
- Docker Compose orchestration with Prometheus/Grafana
- Optional network throttling via Linux TC (traffic control)
- Pre-configured Grafana dashboards

### Kubernetes Deployment

Deploy to GKE with full observability stack:

```bash
# Deploy to cluster
cargo run --release --bin apollo_network_benchmark_run -- cluster start \
  --num-nodes 10 \
  --mode one \
  --broadcaster 0 \
  --network-protocol gossipsub \
  --cpu-requests 7500m \
  --memory-requests 10Gi \
  --timeout 14400

# Use existing Docker image
cargo run --release --bin apollo_network_benchmark_run -- cluster start \
  --image us-central1-docker.pkg.dev/starkware-dev/sequencer/broadcast-network-stress-test-node:2024-01-15-10-30-00 \
  --num-nodes 10 \
  --mode one \
  --broadcaster 0

# Port forward to access Grafana and Prometheus
cargo run --release --bin apollo_network_benchmark_run -- cluster port-forward

# View logs from all pods
cargo run --release --bin apollo_network_benchmark_run -- cluster logs

# Stop and clean up
cargo run --release --bin apollo_network_benchmark_run -- cluster stop
```

**Kubernetes Deployment Features**:
- Indexed Job for deterministic pod naming and DNS
- StatefulSet for Prometheus with persistent storage (16Gi)
- StatefulSet for Grafana with persistent storage (8Gi)
- Headless services for pod-to-pod communication
- ConfigMaps for Prometheus scrape configuration
- Node affinity for dedicated node pools
- Resource limits and requests
- Automatic Docker image building and pushing to GCR

### Docker Images

Two Dockerfile options are provided:

1. **`Dockerfile.fast`**: Copies pre-built binary (faster iteration)
2. **`Dockerfile.slow`**: Builds from source inside Docker (reproducible)

Both include:
- Ubuntu 24.04 base
- iproute2 for traffic control
- Network throttling entrypoint script
- Prometheus metrics exposure

## Usage Examples

### Example 1: Basic Throughput Test

Test maximum throughput with large messages:

```bash
cargo run --release --bin apollo_network_benchmark_run -- local start \
  --num-nodes 5 \
  --mode all \
  --network-protocol gossipsub \
  --message-size-bytes 65536 \
  --heartbeat-millis 10 \
  --verbosity 2
```

### Example 2: Latency Test

Test message latency with small, frequent messages:

```bash
cargo run --release --bin apollo_network_benchmark_run -- local start \
  --num-nodes 3 \
  --mode one \
  --broadcaster 0 \
  --network-protocol gossipsub \
  --message-size-bytes 1024 \
  --heartbeat-millis 1 \
  --verbosity 3
```

### Example 3: Network Adversity Testing

Test behavior under constrained network conditions:

```bash
cargo run --release --bin apollo_network_benchmark_run -- local start \
  --num-nodes 5 \
  --latency 100 \
  --throughput 5000 \
  --mode one \
  --broadcaster 0 \
  --message-size-bytes 8192 \
  --heartbeat-millis 100
```

## Configuration Reference

### Node Binary Arguments (`apollo_network_benchmark_node`)

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `--id` | u64 | (required) | Node identifier for metrics and peer ID generation |
| `--metric-port` | u16 | (required) | Port for Prometheus metrics endpoint |
| `--p2p-port` | u16 | (required) | Port for P2P network communication |
| `--bootstrap` | String[] | (required) | Bootstrap peer multiaddresses (comma-separated) |
| `--mode` | Mode | `all` | Operating mode: `all`, `one`, `rr` |
| `--network-protocol` | Protocol | `gossipsub` | Network protocol: `gossipsub`, `sqmr`, `reversed-sqmr` |
| `--broadcaster` | u64 | - | Node ID to broadcast (required for `one` mode) |
| `--message-size-bytes` | usize | `1024` | Message size in bytes |
| `--heartbeat-millis` | u64 | `1000` | Milliseconds between broadcasts |
| `--round-duration-seconds` | u64 | `3` | Seconds each node broadcasts in `rr` mode |
| `--buffer-size` | usize | `100000` | Channel buffer size for message queues |
| `--timeout` | u64 | `4000` | Node timeout in seconds |
| `--verbosity` | u8 | `2` | Log level: 0=None, 1=Error, 2=Warn, 3=Info, 4=Debug, 5=Trace |

### Orchestrator Arguments (`apollo_network_benchmark_run`)

#### `local start`

All node arguments above are passed through, plus:

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `--image` | String | None | Use existing Docker image instead of building |
| `--latency` | u32 | None | Minimum network latency in ms |
| `--throughput` | u32 | None | Maximum network throughput in KB/s |
| `--memory-limit` | String | `3g` | Memory limit per container |

#### `cluster start`

All node arguments above are passed through, plus:

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `--image` | String | None | Use existing Docker image instead of building |
| `--latency` | u32 | None | Network latency in ms (pod-level throttling) |
| `--throughput` | u32 | None | Network throughput in KB/s (pod-level throttling) |
| `--node-pool-role` | String | `apollo-network-benchmark` | Node pool selector |
| `--node-toleration` | String | None | Pod toleration in `key=value` format |
| `--cpu-requests` | String | `7500m` | CPU requests per pod |
| `--memory-requests` | String | `10Gi` | Memory requests per pod |
| `--cpu-limits` | String | `7500m` | CPU limits per pod |
| `--memory-limits` | String | `10Gi` | Memory limits per pod |
| `--production-docker` | flag | false | Use full Docker build instead of fast build |

## Files and Structure

```
crates/apollo_network_benchmark/
├── Cargo.toml
├── README.md
├── config/                                 # Static config files for Prometheus/Grafana/K8s
│   ├── datasource.yml
│   ├── dashboard_config.yml
│   ├── grafana.ini
│   ├── preferences.json
│   ├── k8s_grafana_deployment.json
│   ├── k8s_grafana_service.json
│   ├── k8s_grafana_headless_service.json
│   ├── k8s_prometheus_deployment.json
│   ├── k8s_prometheus_service.json
│   ├── k8s_prometheus_headless_service.json
│   └── k8s_stress_test_headless_service.json
├── run/
│   ├── Dockerfile.fast                     # Fast Docker build (copies pre-built binary)
│   ├── Dockerfile.slow                     # Full Docker build (builds from source)
│   └── entrypoint.sh                       # Container entrypoint with traffic control
└── src/
    ├── lib.rs
    ├── metrics.rs                          # Shared metric definitions
    ├── node_args.rs                        # Shared CLI argument types
    ├── peer_key.rs                         # Deterministic peer key derivation
    └── bin/
        ├── apollo_network_benchmark_node/  # Stress test node binary
        │   ├── main.rs
        │   ├── stress_test_node.rs         # Main orchestrator
        │   ├── handlers.rs                 # Message send/receive logic
        │   ├── protocol.rs                 # Protocol abstraction (Gossipsub/SQMR)
        │   ├── message.rs                  # Message serialization
        │   ├── metrics.rs                  # Node-specific metrics
        │   └── message_index_detector.rs   # Gap detection
        └── apollo_network_benchmark_run/   # Deployment orchestrator binary
            ├── main.rs
            ├── args.rs                     # Shared orchestrator args
            ├── mod_utils.rs                # Shell/Docker/K8s helpers
            ├── grafana_config.rs           # Grafana dashboard generation
            ├── yaml_maker.rs               # K8s manifest generation
            ├── local_start.rs              # Local Docker Compose deployment
            ├── local_stop.rs               # Stop local deployment
            ├── cluster_start.rs            # K8s cluster deployment
            ├── cluster_stop.rs             # Stop cluster deployment
            ├── cluster_logs.rs             # Fetch cluster pod logs
            └── cluster_port_forward.rs     # Port-forward Grafana/Prometheus
```

## Advanced Topics

### Message Index Tracking

The stress test tracks message indices to detect:
- **Message gaps**: Missing messages in sequence
- **Pending messages**: Expected messages not yet received
- **Reordering**: Out-of-order message delivery

This is exposed via the `receive_message_pending_count` metric.

### Traffic Control (TC) in Docker

When using `--latency` or `--throughput`, the container entrypoint script configures Linux TC:
- Creates IFB (Intermediate Functional Block) device for ingress shaping
- Redirects ingress traffic to IFB
- Applies `htb` for throughput limiting
- Applies `netem` delay for latency simulation

### Peer ID Generation

Peer IDs are deterministically generated from node IDs using `apollo_network_benchmark::peer_key::private_key_from_node_id`, which writes the node index as little-endian bytes into a 32-byte ed25519 seed. This ensures consistent peer IDs across restarts for the same node ID.

## Troubleshooting

### Issue: "Connection refused" on metrics port

**Solution**: Ensure no other process is using the metric port. Check with `lsof -i :2000`

### Issue: Messages not being received

**Solution**: 
- Verify bootstrap addresses are correct
- Check firewall rules
- Increase `--verbosity` to see connection logs
- Ensure all nodes have correct peer IDs in bootstrap addresses

### Issue: High CPU usage

**Solution**:
- Increase `--heartbeat-millis` to reduce message frequency
- Decrease `--message-size-bytes`
- Reduce `--num-nodes`
- Check for message processing bottlenecks in metrics

### Issue: Kubernetes pods failing to schedule

**Solution**:
- Verify node pool has sufficient resources
- Check `--cpu-requests` and `--memory-requests` are reasonable
- Ensure node pool selector matches available nodes

### Issue: Docker build fails

**Solution**:
- The default build mode is fast (builds locally, copies binary). Use `--production-docker` for full in-container builds.
- Ensure `target/release/apollo_network_benchmark_node` exists when using fast build
- Check Docker daemon is running

## Contributing

When adding new features:

1. Add metrics to `src/metrics.rs` using the `define_metrics!` macro
2. Update modes in `src/node_args.rs` and handle in `stress_test_node.rs`
3. Add protocol support in `protocol.rs`
4. Update this README with usage examples

## License

See workspace LICENSE file.

