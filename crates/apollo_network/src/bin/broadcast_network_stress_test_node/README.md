# Broadcast Network Stress Test Node

A comprehensive network stress testing tool for the Apollo network that tests P2P communication, measures performance metrics, and validates network behavior under various load patterns and conditions.

## Overview

The broadcast network stress test node is designed to stress test the P2P communication layer of the Apollo network. It creates a network of nodes with configurable broadcasting patterns, measuring latency, throughput, message ordering, and overall network performance. The tool supports both local testing (using the provided Python scripts) and distributed deployment via Kubernetes with optional network throttling.

## Features

- **Multiple Broadcasting Modes**: Supports different message broadcasting patterns (all nodes, single broadcaster, round-robin)
- **Advanced Performance Metrics**: Measures message latency, throughput, delivery rates, ordering, and duplicate detection
- **Comprehensive System Monitoring**: Tracks CPU usage, memory consumption, network I/O, and gossipsub protocol metrics
- **Message Ordering Analysis**: Tracks out-of-order messages, missing messages, and duplicates
- **Prometheus Integration**: Exports detailed metrics with proper labels for monitoring and analysis
- **Network Throttling**: Supports bandwidth and latency gating for realistic network conditions
- **Configurable Parameters**: Customizable message sizes, send intervals, buffer sizes, test duration, and system metrics intervals
- **Multi-Node Support**: Can run multiple coordinated nodes with different broadcasting patterns
- **Kubernetes Deployment**: Includes YAML templates for cluster deployment with traffic shaping and auto-cleanup
- **Deterministic Peer IDs**: Generates consistent peer identities for reproducible tests
- **Performance Profiling**: Integrated support for CPU, memory, and heap profiling with perf and valgrind
- **Automatic Timeout**: Configurable test duration with automatic termination
- **Resource Management**: Kubernetes deployments with configurable CPU/memory limits and dedicated node pools
- **Resilient Infrastructure**: Auto-retry mechanisms for Prometheus port forwarding and automatic namespace cleanup

## Building

Build the stress test node binary:

```bash
# For basic functionality
cargo build --release --bin broadcast_network_stress_test_node

# For additional tokio metrics (used by local script)
RUSTFLAGS="--cfg tokio_unstable" cargo build --release --bin broadcast_network_stress_test_node
```

## Command Line Arguments

**Note**: Default values shown are for direct binary usage. Python scripts (`local.py`, `cluster_start.py`) may override some defaults.

| Argument | Description | Default | Environment Variable |
|----------|-------------|---------|---------------------|
| `--id` | Node ID for identification and metrics | Required | `ID` |
| `--num-nodes` | Total number of nodes in the network | 3 | `NUM_NODES` |
| `--metric-port` | Prometheus metrics server port | 2000 | `METRIC_PORT` |
| `--p2p-port` | P2P network port | 10000 | `P2P_PORT` |
| `--bootstrap` | Bootstrap peer addresses (comma-separated) | None | `BOOTSTRAP` |
| `--verbosity` | Log verbosity (0-5: None, ERROR, WARN, INFO, DEBUG, TRACE) | 0 (2 in Python scripts) | `VERBOSITY` |
| `--buffer-size` | Broadcast topic buffer size | 10000 | `BUFFER_SIZE` |
| `--message-size-bytes` | Message payload size in bytes | 1024 | `MESSAGE_SIZE_BYTES` |
| `--heartbeat-millis` | Interval between messages (milliseconds) | 1 | `HEARTBEAT_MILLIS` |
| `--mode` | Broadcasting mode: `all`, `one`, `rr`, or `explore` | `all` | `MODE` |
| `--broadcaster` | In mode `one` or `explore`, which node ID should do the broadcasting | 1 (last node in Python scripts) | `BROADCASTER` |
| `--round-duration-seconds` | Duration per node in RoundRobin mode | 3 | `ROUND_DURATION_SECONDS` |
| `--explore-cool-down-duration-seconds` | Cool down duration between configuration changes in Explore mode | Required for explore mode | `EXPLORE_COOL_DOWN_DURATION_SECONDS` |
| `--explore-run-duration-seconds` | Duration to run each configuration in Explore mode | Required for explore mode | `EXPLORE_RUN_DURATION_SECONDS` |
| `--explore-min-throughput-byte-per-seconds` | Minimum throughput in bytes per second for Explore mode | Required for explore mode | `EXPLORE_MIN_THROUGHPUT_BYTE_PER_SECONDS` |
| `--system-metrics-interval-seconds` | Interval for collecting process metrics (CPU, memory) | 1 | `SYSTEM_METRICS_INTERVAL_SECONDS` |
| `--timeout` | Timeout in seconds for the node (when exceeded, node is killed) | None (7200 in Python scripts) | `TIMEOUT` |
| `--enable-libp2p-metrics` | Enable libp2p built-in bandwidth and transport metrics | false | `ENABLE_LIBP2P_METRICS` |
| `--tcp` | Sets the multi-addresses to use TCP instead of UDP/QUIC | true | `TCP` |

## Broadcasting Modes

### All Broadcast (`all`)
All nodes continuously broadcast messages simultaneously. Best for testing network capacity and concurrent message handling.

### Single Broadcaster (`one`)
Only the node specified by `--broadcaster` sends messages, while others act as receivers. Ideal for testing message propagation and network topology.

### Round Robin (`rr`)
Nodes take turns broadcasting in sequential order based on their ID. Each node broadcasts for `--round-duration-seconds` before passing to the next. Useful for testing network behavior under changing load patterns.

### Explore (`explore`)
A specialized single broadcaster mode that automatically explores different combinations of message sizes and throughput rates over time. Only the node specified by `--broadcaster` sends messages, but the configuration (message size and send interval) changes every `--explore-run-duration-seconds` with a cooldown period of `--explore-cool-down-duration-seconds`. Ideal for automated performance testing across various network conditions and finding optimal throughput configurations.

## Running Locally

### Recommended: Multi-Node Network using Local Script

The best way to run locally is using the local script. First, navigate to the run directory:

```bash
cd crates/apollo_network/src/bin/broadcast_network_stress_test_node/run
python local.py --num-nodes 3 --verbosity 3 --mode rr
```

This will:
- Compile the binary if needed
- Start 3 nodes with sequential ports (10000, 10001, 10002) 
- Automatically configure bootstrap peers for all nodes
- Launch Prometheus in Docker for metrics collection
- Provide a web interface at http://localhost:9090

### Manual Single Node (Advanced)

For direct binary testing (not recommended for most use cases):

```bash
./target/release/broadcast_network_stress_test_node \
    --id 0 \
    --metric-port 2000 \
    --p2p-port 10000 \
    --verbosity 3 \
    --mode all
```

### Advanced Local Testing

All commands should be run from the run directory:

```bash
cd crates/apollo_network/src/bin/broadcast_network_stress_test_node/run

# Test round-robin mode with custom timing
python local.py --num-nodes 5 --mode rr --round-duration-seconds 10 --heartbeat-millis 100

# Test single broadcaster mode
python local.py --num-nodes 3 --mode one --broadcaster 0 --message-size-bytes 4096

# Enable profiling with perf (CPU profiling)
python local.py --num-nodes 3 --profile --profile-mode cpu --mode all

# Enable memory profiling with perf
python local.py --num-nodes 3 --profile --profile-mode mem --mode rr

# Use DHAT memory profiler (requires valgrind)
python local.py --num-nodes 3 --profile --profile-mode dhat --mode all

# Test explore mode with automatic configuration changes
python local.py --num-nodes 3 --mode explore --broadcaster 0 --explore-run-duration-seconds 30 --explore-cool-down-duration-seconds 5 --explore-min-throughput-byte-per-seconds 1000
```

## Kubernetes Deployment

### Prerequisites

- Kubernetes cluster access
- Docker registry access
- kubectl configured

### Deploy to Cluster

```bash
cd crates/apollo_network/src/bin/broadcast_network_stress_test_node/run
python cluster_start.py --num-nodes 5 --latency 50 --throughput 1000 --mode rr
```

This will:
- Build and push a Docker image
- Create Kubernetes StatefulSet with 5 nodes
- Apply network throttling (50ms latency, 1000 KB/s throughput)
- Deploy to a timestamped namespace
- Set up automatic namespace deletion after timeout

### Advanced Cluster Deployment Options

```bash
# Use a pre-built image instead of building a new one
python cluster_start.py --image us-central1-docker.pkg.dev/starkware-dev/sequencer/broadcast-network-stress-test-node:2024-01-15-10-30-00 --num-nodes 3

# Deploy to dedicated node pool with custom resource limits
python cluster_start.py --num-nodes 4 --dedicated-node-pool --node-pool-name production --cpu-requests 2000m --memory-requests 4Gi

# Custom timeout and resource configuration
python cluster_start.py --num-nodes 3 --timeout 3600 --cpu-limits 2000m --memory-limits 2Gi
```

### Access Prometheus

```bash
python cluster_port_forward_prometheus.py
```

Then visit http://localhost:9090 for metrics visualization.

### View Logs

```bash
python cluster_log.py
```

This saves logs from all deployed nodes to `/tmp/broadcast-network-stress-test-*.logs.txt` files for offline analysis.

### Cleanup

```bash
python cluster_stop.py
```

## Network Throttling

The Docker deployment supports network traffic shaping to simulate realistic network conditions:

- **Latency Gating**: Add artificial delay to packets (via `LATENCY` environment variable in ms)
- **Throughput Limiting**: Cap bandwidth to test under constrained conditions (via `THROUGHPUT` environment variable in KB/s)

The entrypoint script uses Linux traffic control (`tc`) with HTB (Hierarchical Token Bucket) for bandwidth limiting and NetEm for latency simulation.

## Metrics

The tool exports comprehensive Prometheus metrics with proper labels:

### Message Flow Metrics
- `broadcast_messages_sent_total`: Total messages sent by this node via broadcast topic
- `broadcast_bytes_sent_total`: Total bytes sent via broadcast topic
- `receive_messages_total`: Total messages received
- `receive_bytes_total`: Total bytes received across all messages

### Configuration Metrics (Explore Mode)
- `broadcast_message_size`: Current size of the stress test message in bytes
- `broadcast_message_heartbeat_millis`: Current number of milliseconds between consecutive broadcasts
- `broadcast_message_throughput`: Current throughput in bytes per second of broadcasted messages

### Performance Metrics
- `receive_message_delay_seconds`: End-to-end message latency histogram
- `receive_message_negative_delay_seconds`: Negative message delay histogram (clock synchronization issues)
- `broadcast_message_send_delay_seconds`: Time taken to send messages (local sending latency)

### Message Ordering Metrics
- `receive_messages_out_of_order_total`: Messages received out of sequence
- `receive_messages_missing_total`: Messages that appear to be missing
- `receive_messages_duplicate_total`: Duplicate messages detected
- `receive_messages_missing_retrieved_total`: Previously missing messages that arrived late

### Network Connection Metrics
- `network_connected_peers`: Number of connected peers
- `network_blacklisted_peers`: Number of blacklisted peers
- `network_stress_test_sent_messages`: Messages sent via broadcast topic
- `network_stress_test_received_messages`: Messages received via broadcast topic

### Gossipsub Protocol Metrics
- `gossipsub_mesh_peers`: Number of mesh peers
- `gossipsub_all_peers`: Total number of known peers
- `gossipsub_subscribed_topics`: Number of subscribed topics
- `gossipsub_protocol_peers`: Number of gossipsub protocol peers
- `gossipsub_messages_received`: Number of gossipsub messages received
- `gossipsub_positive_score_peers`: Peers with positive scores
- `gossipsub_negative_score_peers`: Peers with negative scores

### System Resource Metrics
- `process_cpu_usage_percent`: CPU usage percentage of the current process
- `process_memory_usage_bytes`: Memory usage in bytes of the current process
- `process_virtual_memory_usage_bytes`: Virtual memory usage in bytes
- `system_total_memory_bytes`: Total system memory
- `system_available_memory_bytes`: Available system memory
- `system_network_bytes_sent_total`: Total network bytes sent
- `system_network_bytes_received_total`: Total network bytes received

All metrics are properly labeled for detailed analysis, and the tool automatically collects system-level metrics at configurable intervals.

## Configuration

### Message Structure

Each stress test message contains:
- **Sender ID**: Node identifier (8 bytes)
- **Message Index**: Sequential message number from sender (8 bytes)
- **Timestamp**: Send time as nanoseconds since UNIX epoch (16 bytes)
- **Payload Length**: Size of variable payload (8 bytes)
- **Payload**: Configurable data (remaining bytes)

### Network Topology

- All nodes join the same gossipsub topic: `stress_test_topic`
- Node 0 typically acts as the bootstrap peer for network discovery
- Deterministic peer IDs based on node ID ensure consistent network formation
- Secret keys are generated deterministically from node ID for reproducibility

## Example Use Cases

### Latency Testing
```bash
# Test with 100ms network latency
python cluster_start.py --num-nodes 3 --latency 100 --message-size-bytes 512 --mode all
```

### Throughput Testing
```bash
# Test with 500 KB/s bandwidth limit
python cluster_start.py --num-nodes 5 --throughput 500 --heartbeat-millis 10 --mode rr
```

### Large Message Testing
```bash
# Test with 64KB messages in single broadcaster mode (run from the run directory)
cd crates/apollo_network/src/bin/broadcast_network_stress_test_node/run
python local.py --num-nodes 3 --message-size-bytes 65536 --heartbeat-millis 100 --mode one
```

### Network Resilience Testing
```bash
# Test round-robin with constrained network
python cluster_start.py --num-nodes 4 --latency 200 --throughput 100 --mode rr --round-duration-seconds 30
```

### Performance Exploration Testing
```bash
# Automatically explore different message sizes and throughput configurations
python cluster_start.py --num-nodes 3 --mode explore --broadcaster 0 --explore-run-duration-seconds 60 --explore-cool-down-duration-seconds 10 --explore-min-throughput-byte-per-seconds 500

# Local exploration with custom parameters (run from the run directory)
cd crates/apollo_network/src/bin/broadcast_network_stress_test_node/run
python local.py --num-nodes 3 --mode explore --broadcaster 1 --explore-run-duration-seconds 45 --explore-cool-down-duration-seconds 15 --explore-min-throughput-byte-per-seconds 1000
```

## Development

### File Structure

- `main.rs`: Core stress test logic, broadcasting modes, and comprehensive metrics collection
- `converters.rs`: Message serialization/deserialization with ordering support
- `converters_test.rs`: Unit tests for message conversion
- `utils.rs`: Configuration utilities and helper functions
- `run/`: Deployment scripts and configurations
  - `local.py`: Local multi-node testing with Prometheus and profiling support
  - `cluster_start.py`: Kubernetes deployment with throttling and resource management
  - `cluster_stop.py`: Cleanup deployed resources and namespaces
  - `cluster_port_forward_prometheus.py`: Resilient Prometheus access with auto-retry
  - `cluster_log.py`: Log viewing and management for cluster deployments
  - `yaml_maker.py`: Kubernetes YAML generation with RBAC and auto-cleanup
  - `args.py`: Shared argument parsing for Python scripts
  - `utils.py`: Common utility functions and peer ID management
  - `Dockerfile`: Container image with traffic shaping capabilities
  - `entrypoint.sh`: Container startup script with network throttling

### Adding New Metrics

1. Import metrics crate: `use metrics::{counter, histogram, gauge};`
2. Add metric recording in message handlers or broadcasting logic
3. Use appropriate labels for detailed analysis
4. Update Prometheus configuration in deployment scripts if needed

### Adding New Broadcasting Modes

1. Extend the `Mode` enum in `main.rs`
2. Update the mode-specific logic in `send_stress_test_messages()`
3. Add corresponding argument parsing in `args.py`
4. Update documentation and examples

### Network Configuration

Modify `NetworkConfig` parameters in `main.rs` for different P2P behaviors:
- Connection limits and timeouts
- Heartbeat intervals
- Gossipsub parameters (mesh size, fanout, etc.)
- Discovery mechanisms and protocols

## Troubleshooting

### Common Issues

**Nodes not connecting**: Check bootstrap peer address and ensure firewall allows UDP traffic on P2P ports. Verify that node 0 is started first as the bootstrap peer.

**High or inconsistent latency readings**: Verify system clocks are synchronized across test nodes. Consider NTP setup for distributed testing.

**Out-of-order messages**: This is normal in P2P networks. Monitor the `messages_out_of_order_total` metric to understand network behavior patterns.

**Prometheus not scraping**: Confirm metric ports are accessible and Prometheus configuration includes all node endpoints. When using the local script, Prometheus runs in Docker and automatically configures all node endpoints. Check firewall rules and ensure Docker is running properly.

**Docker permission errors for throttling**: Ensure privileged mode is enabled for network traffic shaping. The container needs CAP_NET_ADMIN capability.

**Message size errors**: Ensure `--message-size-bytes` is at least 40 bytes (metadata size). Check the calculation in `converters.rs` if issues persist.

### Debugging

Enable verbose logging for detailed P2P communication:
```bash
# For local script (default verbosity is 2)
python local.py --verbosity 5

# For cluster script (default verbosity is 2)  
python cluster_start.py --verbosity 5 --num-nodes 3

# For direct binary usage (default verbosity is 0)
./target/release/broadcast_network_stress_test_node --verbosity 5

# Set timeout for automatic termination
python local.py --timeout 3600 --verbosity 3
```

Check individual node logs in Kubernetes:
```bash
kubectl logs -n broadcast-network-stress-test-{timestamp} broadcast-network-stress-test-0 -f
```

Monitor live metrics during testing:
```bash
# View all metrics from a node
curl http://localhost:2000/metrics

# Monitor specific metrics
curl -s http://localhost:2000/metrics | grep receive_messages_total
```

Use Prometheus queries for analysis:
```promql
# Average message latency by sender
rate(receive_message_delay_seconds_sum[5m]) / rate(receive_message_delay_seconds_count[5m])

# Message loss rate
rate(receive_messages_missing_total[5m]) / rate(broadcast_messages_sent_total[5m])

# Network throughput
rate(receive_bytes_total[5m])

# Explore mode: current throughput
broadcast_message_throughput

# Monitor specific metrics
curl -s http://localhost:2000/metrics | grep receive_messages_total
```
