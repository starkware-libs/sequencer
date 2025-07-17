# Broadcast Network Stress Test Node

A comprehensive network stress testing tool for the Apollo network that tests P2P communication, measures performance metrics, and validates network behavior under various load patterns and conditions.

## Overview

The broadcast network stress test node is designed to stress test the P2P communication layer of the Apollo network. It creates a network of nodes with configurable broadcasting patterns, measuring latency, throughput, message ordering, and overall network performance. The tool supports both local testing and distributed deployment via Kubernetes with optional network throttling.

## Features

- **Multiple Broadcasting Modes**: Supports different message broadcasting patterns (all nodes, single broadcaster, round-robin)
- **Advanced Performance Metrics**: Measures message latency, throughput, delivery rates, ordering, and duplicate detection
- **Message Ordering Analysis**: Tracks out-of-order messages, missing messages, and duplicates
- **Prometheus Integration**: Exports detailed metrics with proper labels for monitoring and analysis
- **Network Throttling**: Supports bandwidth and latency gating for realistic network conditions
- **Configurable Parameters**: Customizable message sizes, send intervals, buffer sizes, and test duration
- **Multi-Node Support**: Can run multiple coordinated nodes with different broadcasting patterns
- **Kubernetes Deployment**: Includes YAML templates for cluster deployment with traffic shaping
- **Deterministic Peer IDs**: Generates consistent peer identities for reproducible tests

## Building

Build the stress test node binary:

```bash
cargo build --release --bin broadcast_network_stress_test_node
```

## Command Line Arguments

| Argument | Description | Default | Environment Variable |
|----------|-------------|---------|---------------------|
| `--id` | Node ID for identification and metrics | Required | `ID` |
| `--num-nodes` | Total number of nodes in the network | 3 | `NUM_NODES` |
| `--metric-port` | Prometheus metrics server port | 2000 | `METRIC_PORT` |
| `--p2p-port` | P2P network port | 10000 | `P2P_PORT` |
| `--bootstrap` | Bootstrap peer addresses (comma-separated) | None | `BOOTSTRAP` |
| `--verbosity` | Log verbosity (0-5: None, ERROR, WARN, INFO, DEBUG, TRACE) | 0 | `VERBOSITY` |
| `--buffer-size` | Broadcast topic buffer size | 10000 | `BUFFER_SIZE` |
| `--message-size-bytes` | Message payload size in bytes | 1024 | `MESSAGE_SIZE_BYTES` |
| `--heartbeat-millis` | Interval between messages (milliseconds) | 1 | `HEARTBEAT_MILLIS` |
| `--mode` | Broadcasting mode: `all`, `one`, or `rr` | `all` | `MODE` |
| `--broadcaster` | Node ID for broadcasting (OneBroadcast mode) | 1 | `BROADCASTER` |
| `--round-duration-seconds` | Duration per node in RoundRobin mode | 3 | `ROUND_DURATION_SECONDS` |

## Broadcasting Modes

### All Broadcast (`all`)
All nodes continuously broadcast messages simultaneously. Best for testing network capacity and concurrent message handling.

### Single Broadcaster (`one`)
Only the node specified by `--broadcaster` sends messages, while others act as receivers. Ideal for testing message propagation and network topology.

### Round Robin (`rr`)
Nodes take turns broadcasting in sequential order based on their ID. Each node broadcasts for `--round-duration-seconds` before passing to the next. Useful for testing network behavior under changing load patterns.

## Running Locally

### Single Node

```bash
./target/release/broadcast_network_stress_test_node \
    --id 0 \
    --metric-port 2000 \
    --p2p-port 10000 \
    --verbosity 3 \
    --mode all
```

### Multi-Node Network

Use the provided Python script for local multi-node testing:

```bash
cd crates/apollo_network/src/bin/broadcast_network_stress_test_node/run
python local.py --num-nodes 3 --verbosity 3 --mode rr
```

This will:
- Compile the binary if needed
- Start 3 nodes with sequential ports (10000, 10001, 10002)
- Launch Prometheus for metrics collection
- Provide a web interface at http://localhost:9090

### Advanced Local Testing

```bash
# Test round-robin mode with custom timing
python local.py --num-nodes 5 --mode rr --round-duration-seconds 10 --heartbeat-millis 100

# Test single broadcaster mode
python local.py --num-nodes 3 --mode one --broadcaster 0 --message-size-bytes 4096

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

### Access Prometheus

```bash
python cluster_port_forward_prometheus.py
```

Then visit http://localhost:9090 for metrics visualization.

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
- `messages_sent_total`: Total messages sent by this node
- `messages_received_total`: Total messages received (with `sender_id` label)
- `bytes_received_total`: Total bytes received across all messages

### Performance Metrics
- `message_delay_seconds`: End-to-end message latency histogram (with `sender_id` label)

### Message Ordering Metrics
- `messages_out_of_order_total`: Messages received out of sequence (with `sender_id` label)
- `messages_missing_total`: Messages that appear to be missing (with `sender_id` label)
- `messages_duplicate_total`: Duplicate messages detected (with `sender_id` label)
- `messages_missing_retrieved_total`: Previously missing messages that arrived late (with `sender_id` label)

All metrics include appropriate labels for per-sender analysis, enabling detailed network behavior study.

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
# Test with 64KB messages in single broadcaster mode
python local.py --num-nodes 3 --message-size-bytes 65536 --heartbeat-millis 100 --mode one
```

### Network Resilience Testing
```bash
# Test round-robin with constrained network
python cluster_start.py --num-nodes 4 --latency 200 --throughput 100 --mode rr --round-duration-seconds 30
```

## Development

### File Structure

- `main.rs`: Core stress test logic, broadcasting modes, and coordination
- `converters.rs`: Message serialization/deserialization with ordering support
- `converters_test.rs`: Unit tests for message conversion
- `utils.rs`: Configuration utilities and helper functions
- `run/`: Deployment scripts and configurations
  - `local.py`: Local multi-node testing with Prometheus
  - `cluster_start.py`: Kubernetes deployment with throttling
  - `cluster_stop.py`: Cleanup deployed resources
  - `cluster_port_forward_prometheus.py`: Prometheus access helper
  - `yaml_maker.py`: Kubernetes YAML generation
  - `args.py`: Shared argument parsing for Python scripts
  - `utils.py`: Common utility functions
  - `Dockerfile`: Container image with traffic shaping capabilities
  - `entrypoint.sh`: Container startup script with network throttling
  - Various Kubernetes YAML templates

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

**Prometheus not scraping**: Confirm metric ports are accessible and Prometheus configuration includes all node endpoints. Check firewall rules and network policies.

**Docker permission errors for throttling**: Ensure privileged mode is enabled for network traffic shaping. The container needs CAP_NET_ADMIN capability.

**Message size errors**: Ensure `--message-size-bytes` is at least 40 bytes (metadata size). Check the calculation in `converters.rs` if issues persist.

### Debugging

Enable verbose logging for detailed P2P communication:
```bash
--verbosity 5
```

Check individual node logs in Kubernetes:
```bash
kubectl logs -n network-stress-test-{timestamp} network-stress-test-0 -f
```

Monitor live metrics during testing:
```bash
# View all metrics from a node
curl http://localhost:2000/metrics

# Monitor specific metrics
curl -s http://localhost:2000/metrics | grep messages_received_total
```

Use Prometheus queries for analysis:
```promql
# Average message latency by sender
rate(message_delay_seconds_sum[5m]) / rate(message_delay_seconds_count[5m])

# Message loss rate
rate(messages_missing_total[5m]) / rate(messages_sent_total[5m])

# Network throughput
rate(bytes_received_total[5m])
```
