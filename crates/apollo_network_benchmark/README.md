# Apollo Network Benchmark

A comprehensive network stress testing and benchmarking tool for evaluating the performance, throughput, and reliability of Apollo's peer-to-peer networking layer.

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Binary: broadcast_network_stress_test_node](#binary-broadcast_network_stress_test_node)
- [Operating Modes](#operating-modes)
- [Network Protocols](#network-protocols)
- [Metrics and Monitoring](#metrics-and-monitoring)
- [Deployment Options](#deployment-options)
- [Usage Examples](#usage-examples)
- [Configuration Reference](#configuration-reference)

## Overview

The `apollo_network_benchmark` crate provides tools to stress test and benchmark Apollo's network layer under various conditions. It simulates realistic network workloads by running multiple nodes that communicate using different protocols and broadcast patterns.

### Key Features

- **Multiple Broadcasting Modes**: Test different message propagation patterns (all-to-all, single broadcaster, round-robin, explore mode)
- **Protocol Flexibility**: Support for Gossipsub, SQMR (Sequential Query Message Response), and Reversed SQMR
- **Comprehensive Metrics**: Real-time monitoring of throughput, latency, CPU, memory, and network statistics
- **Integrated Observability**: Built-in Prometheus and Grafana support for visualization
- **Flexible Deployment**: Run locally or deploy to Kubernetes with traffic shaping capabilities
- **Explore Mode**: Automatically tests multiple message size and throughput combinations to find optimal configurations

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

### Core Modules

- **`stress_test_node.rs`**: Main orchestrator that manages network lifecycle and task coordination
- **`handlers.rs`**: Message sending and receiving logic with metrics collection
- **`network_channels.rs`**: Protocol abstraction layer for different network protocols
- **`message_handling.rs`**: Unified interface for sending/receiving across protocols
- **`metrics.rs`**: Comprehensive metrics definitions and collection
- **`explore_config.rs`**: Configuration exploration mode for finding optimal parameters
- **`converters.rs`**: Message serialization/deserialization with metadata

## Binary: broadcast_network_stress_test_node

The main binary creates a network node that can broadcast and receive stress test messages using various protocols and patterns.

### Building

```bash
# Build with tokio metrics support
RUSTFLAGS="--cfg tokio_unstable" cargo build --release --bin broadcast_network_stress_test_node

# The binary will be located at:
# target/release/broadcast_network_stress_test_node
```

### Basic Usage

```bash
./broadcast_network_stress_test_node \
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

The stress test supports four distinct operating modes:

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

### 4. Explore Mode (`--mode explore`)

Automatically explores different combinations of message sizes and broadcast intervals, progressively increasing throughput to find optimal configurations.

**Use Case**: Performance tuning, finding maximum sustainable throughput

```bash
--mode explore \
--broadcaster 0 \
--explore-cool-down-duration-seconds 100 \
--explore-run-duration-seconds 100 \
--explore-min-throughput-byte-per-seconds 102400 \
--explore-min-message-size-bytes 1024
```

**Explore Mode Behavior**:
- Tests combinations of message sizes (1KB to 4MB) and heartbeat intervals (1ms to 1000ms)
- Filters by minimum throughput and message size thresholds
- Sorts configurations by throughput (ascending)
- Runs each configuration for `explore-run-duration-seconds`
- Cools down for `explore-cool-down-duration-seconds` between configurations
- Automatically resets network state between trials

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

#### System Metrics
- `system_process_cpu_usage_percent`: CPU usage of the process
- `system_process_memory_usage_bytes`: Memory usage of the process
- `system_network_bytes_sent_total`: Total bytes sent (all interfaces)
- `system_network_bytes_received_total`: Total bytes received
- `system_tcp_retransmit_rate_percent`: TCP retransmission rate (proxy for packet loss)

### Grafana Dashboard

The deployment includes a pre-configured Grafana dashboard accessible at `http://localhost:3000` (local) or via port-forward (Kubernetes) that visualizes:

- Real-time throughput graphs
- Message latency distributions
- Network health indicators
- System resource utilization
- Per-node metrics comparison

## Deployment Options

### Local Deployment

Run multiple nodes locally with integrated Prometheus and Grafana:

```bash
cd crates/apollo_network_benchmark/run

# Run 5 nodes locally
python3 local.py \
  --num-nodes 5 \
  --mode all \
  --network-protocol gossipsub \
  --message-size-bytes 4096 \
  --heartbeat-millis 100 \
  --verbosity 3

# Run with Docker containers
python3 local.py \
  --num-nodes 5 \
  --docker \
  --mode one \
  --broadcaster 0 \
  --message-size-bytes 8192 \
  --heartbeat-millis 50

# Run with network throttling (Docker only)
python3 local.py \
  --num-nodes 3 \
  --docker \
  --latency 50 \
  --throughput 10000 \
  --mode all
```

**Local Deployment Features**:
- Automatic compilation and Docker image building
- Prometheus/Grafana setup with pre-configured dashboards
- Optional network throttling via Linux TC (traffic control)
- Process monitoring and health checks
- Logs stored in `/tmp/broadcast-network-stress-test-<timestamp>/`

### Kubernetes Deployment

Deploy to GKE with full observability stack:

```bash
cd crates/apollo_network_benchmark/run

# Deploy to cluster
python3 cluster_start.py \
  --num-nodes 10 \
  --mode explore \
  --broadcaster 0 \
  --network-protocol gossipsub \
  --explore-cool-down-duration-seconds 120 \
  --explore-run-duration-seconds 120 \
  --explore-min-throughput-byte-per-seconds 1048576 \
  --cpu-requests 7500m \
  --memory-requests 10Gi \
  --timeout 14400

# Use existing Docker image
python3 cluster_start.py \
  --image us-central1-docker.pkg.dev/starkware-dev/sequencer/broadcast-network-stress-test-node:2024-01-15-10-30-00 \
  --num-nodes 10 \
  --mode one \
  --broadcaster 0

# Port forward to access Grafana
python3 cluster_port_forward.py

# View logs from all pods
python3 cluster_logs.py

# Stop and clean up
python3 cluster_stop.py
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
python3 local.py \
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
python3 local.py \
  --num-nodes 3 \
  --mode one \
  --broadcaster 0 \
  --network-protocol gossipsub \
  --message-size-bytes 1024 \
  --heartbeat-millis 1 \
  --verbosity 3
```

### Example 3: Protocol Comparison

Compare Gossipsub vs SQMR performance:

```bash
# Gossipsub
python3 local.py --num-nodes 3 --network-protocol gossipsub --mode all &
sleep 60 && pkill -f broadcast_network

# SQMR
python3 local.py --num-nodes 3 --network-protocol sqmr --mode all &
sleep 60 && pkill -f broadcast_network
```

### Example 4: Explore Mode - Find Optimal Configuration

```bash
python3 cluster_start.py \
  --num-nodes 20 \
  --mode explore \
  --broadcaster 0 \
  --network-protocol gossipsub \
  --explore-cool-down-duration-seconds 180 \
  --explore-run-duration-seconds 300 \
  --explore-min-throughput-byte-per-seconds 10485760 \
  --explore-min-message-size-bytes 4096 \
  --cpu-requests 7500m \
  --memory-requests 10Gi \
  --timeout 28800
```

This will:
- Test configurations with ≥10 MB/s throughput and ≥4KB messages
- Run each configuration for 5 minutes
- Cool down for 3 minutes between configurations
- Automatically progress through increasing throughput levels
- Run for up to 8 hours

### Example 5: Network Adversity Testing

Test behavior under constrained network conditions:

```bash
python3 local.py \
  --num-nodes 5 \
  --docker \
  --latency 100 \
  --throughput 5000 \
  --mode one \
  --broadcaster 0 \
  --message-size-bytes 8192 \
  --heartbeat-millis 100
```

## Configuration Reference

### Command Line Arguments

| Argument | Type | Required | Default | Description |
|----------|------|----------|---------|-------------|
| `--id` | u64 | Yes | - | Node identifier for metrics and peer ID generation |
| `--num-nodes` | u64 | Yes | - | Total number of nodes in the network |
| `--metric-port` | u16 | Yes | - | Port for Prometheus metrics endpoint |
| `--p2p-port` | u16 | Yes | - | Port for P2P network communication |
| `--bootstrap` | String[] | Yes | - | Bootstrap peer multiaddresses (comma-separated) |
| `--mode` | Mode | Yes | - | Operating mode: `all`, `one`, `rr`, `explore` |
| `--network-protocol` | Protocol | Yes | - | Network protocol: `gossipsub`, `sqmr`, `reversed-sqmr` |
| `--broadcaster` | u64 | Conditional | - | Node ID to broadcast (required for `one` and `explore` modes) |
| `--message-size-bytes` | usize | Conditional | - | Message size in bytes (required for `all`, `one`, `rr` modes) |
| `--heartbeat-millis` | u64 | Conditional | - | Milliseconds between broadcasts (required for `all`, `one`, `rr` modes) |
| `--round-duration-seconds` | u64 | Conditional | - | Seconds each node broadcasts (required for `rr` mode) |
| `--explore-cool-down-duration-seconds` | u64 | Conditional | 100 | Cool down seconds (required for `explore` mode) |
| `--explore-run-duration-seconds` | u64 | Conditional | 100 | Run duration seconds (required for `explore` mode) |
| `--explore-min-throughput-byte-per-seconds` | f64 | Conditional | 102400 | Min throughput filter (required for `explore` mode) |
| `--explore-min-message-size-bytes` | usize | Conditional | 1024 | Min message size filter (required for `explore` mode) |
| `--buffer-size` | usize | Yes | - | Channel buffer size for message queues |
| `--timeout` | u64 | Yes | - | Node timeout in seconds |
| `--verbosity` | u8 | Yes | - | Log level: 0=None, 1=Error, 2=Warn, 3=Info, 4=Debug, 5=Trace |

### Python Deployment Script Arguments

#### Local Deployment (`local.py`)

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `--profile` | flag | False | Enable perf profiling (saves to tmp directory) |
| `--profile-mode` | choice | cpu | Profiling mode: `cpu`, `mem`, `dhat` |
| `--docker` | flag | False | Run nodes in Docker containers |
| `--image` | String | None | Use existing Docker image (with `--docker`) |
| `--latency` | int | None | Minimum network latency in ms (with `--docker`) |
| `--throughput` | int | None | Maximum network throughput in KB/s (with `--docker`) |

#### Cluster Deployment (`cluster_start.py`)

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `--image` | String | None | Use existing Docker image instead of building |
| `--latency` | int | None | Network latency in ms (pod-level throttling) |
| `--throughput` | int | None | Network throughput in KB/s (pod-level throttling) |
| `--node-pool-role` | String | apollo-network-benchmark | Node pool selector |
| `--cpu-requests` | String | 7500m | CPU requests per pod |
| `--memory-requests` | String | 10Gi | Memory requests per pod |
| `--cpu-limits` | String | 7500m | CPU limits per pod |
| `--memory-limits` | String | 10Gi | Memory limits per pod |
| `--fast-docker` | flag | False | Use fast Docker build (copies binary) |

## Files and Structure

```
crates/apollo_network_benchmark/
├── Cargo.toml                              # Crate dependencies
├── README.md                               # This file
├── bin/
│   └── broadcast_network_stress_test_node/
│       ├── main.rs                         # Entry point
│       ├── stress_test_node.rs            # Main orchestrator
│       ├── handlers.rs                     # Message handling logic
│       ├── network_channels.rs            # Protocol abstraction
│       ├── message_handling.rs            # Sender/receiver unification
│       ├── metrics.rs                      # Metrics definitions
│       ├── explore_config.rs              # Explore mode configuration
│       ├── converters.rs                   # Message serialization
│       ├── args.rs                         # CLI argument parsing
│       ├── message_index_detector.rs      # Gap detection
│       └── utils.rs                        # Utility functions
└── run/
    ├── local.py                            # Local deployment
    ├── cluster_start.py                   # Kubernetes deployment
    ├── cluster_stop.py                    # Cleanup Kubernetes resources
    ├── cluster_logs.py                    # View cluster logs
    ├── cluster_port_forward.py           # Port forwarding setup
    ├── yaml_maker.py                      # Kubernetes manifest generation
    ├── grafana_config.py                  # Grafana configuration
    ├── args.py                             # Shared argument parsing
    ├── utils.py                            # Shared utilities
    ├── Dockerfile.fast                    # Fast Docker build
    ├── Dockerfile.slow                    # Full Docker build
    └── entrypoint.sh                       # Container entrypoint with traffic control
```

## Advanced Topics

### Message Index Tracking

The stress test tracks message indices to detect:
- **Message gaps**: Missing messages in sequence
- **Pending messages**: Expected messages not yet received
- **Reordering**: Out-of-order message delivery

This is exposed via the `receive_message_pending_count` metric.

### Network Reset in Explore Mode

Explore mode automatically resets the network between configuration trials to ensure clean state:
1. Detects phase transition (Running → CoolDown)
2. Aborts all running tasks
3. Recreates NetworkManager with fresh state
4. Waits for next Running phase
5. Resumes with new configuration

### Traffic Control (TC) in Docker

When using `--docker --latency` or `--docker --throughput`, the entrypoint script configures Linux TC:
- Creates IFB (Intermediate Functional Block) device
- Redirects ingress traffic to IFB
- Applies `netem` delay for latency
- Applies `tbf` (token bucket filter) for throughput limiting
- Affects both ingress and egress

### Peer ID Generation

Peer IDs are deterministically generated from node IDs:
```rust
fn create_peer_private_key(peer_index: u64) -> [u8; 32] {
    let array = peer_index.to_le_bytes();
    let mut private_key = [0u8; 32];
    private_key[0..8].copy_from_slice(&array);
    private_key
}
```

This ensures consistent peer IDs across restarts for the same node ID.

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
- Use `--fast-docker` to skip in-container compilation
- Ensure `target/release/broadcast_network_stress_test_node` exists
- Check Docker daemon is running

## Contributing

When adding new features:

1. Add metrics to `metrics.rs` using the `define_metrics!` macro
2. Update modes in `args.rs` and handle in `stress_test_node.rs`
3. Add protocol support in `network_channels.rs` and `message_handling.rs`
4. Update Python deployment scripts in `run/` as needed
5. Update this README with usage examples

## License

See workspace LICENSE file.

