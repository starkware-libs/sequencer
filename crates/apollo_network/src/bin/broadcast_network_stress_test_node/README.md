# Broadcast Network Stress Test Node

A comprehensive network stress testing tool for the Apollo network that tests P2P communication, measures performance metrics, and validates network behavior under various conditions.

## Overview

The broadcast network stress test node is designed to stress test the P2P communication layer of the Apollo network. It creates a network of nodes that continuously send and receive messages, measuring latency, throughput, and overall network performance. The tool supports both local testing and distributed deployment via Kubernetes.

## Features

- **Bidirectional Message Testing**: Sends and receives broadcast messages across the P2P network
- **Performance Metrics**: Measures message latency, throughput, and delivery rates
- **Prometheus Integration**: Exports metrics for monitoring and analysis
- **Network Throttling**: Supports bandwidth and latency gating for realistic network conditions
- **Configurable Parameters**: Customizable message sizes, send intervals, and buffer sizes
- **Multi-Node Support**: Can run multiple coordinated nodes for comprehensive testing
- **Kubernetes Deployment**: Includes YAML templates for cluster deployment
- **Deterministic Peer IDs**: Generates consistent peer identities for reproducible tests

## Building

Build the stress test node binary:

```bash
cargo build --release --bin broadcast_network_stress_test_node
```

## Command Line Arguments

| Argument | Description | Default | Environment Variable |
|----------|-------------|---------|---------------------|
| `--id` | Node ID for Prometheus logging | Required | `ID` |
| `--metric-port` | Prometheus metrics server port | 2000 | `METRIC_PORT` |
| `--p2p-port` | P2P network port | 10000 | `P2P_PORT` |
| `--bootstrap` | Bootstrap peer address | None | `BOOTSTRAP` |
| `--verbosity` | Log verbosity (0-5) | 0 | `VERBOSITY` |
| `--buffer-size` | Broadcast topic buffer size | 10000 | `BUFFER_SIZE` |
| `--message-size-bytes` | Message payload size | 1024 | `MESSAGE_SIZE_BYTES` |
| `--heartbeat-millis` | Interval between messages (ms) | 1 | `HEARTBEAT_MILLIS` |
| `--timeout` | Maximum runtime (seconds) | 3600 | `TIMEOUT` |

## Running Locally

### Single Node

```bash
./target/release/broadcast_network_stress_test_node \
    --id 0 \
    --metric-port 2000 \
    --p2p-port 10000 \
    --verbosity 3
```

### Multi-Node Network

Use the provided Python script for local multi-node testing:

```bash
cd crates/apollo_network/src/bin/broadcast_network_stress_test_node/run
python local.py --num-nodes 3 --verbosity 3
```

This will:
- Compile the binary
- Start 3 nodes with sequential ports (10000, 10001, 10002)
- Launch Prometheus for metrics collection
- Provide a web interface at http://localhost:9090

## Kubernetes Deployment

### Prerequisites

- Kubernetes cluster access
- Docker registry access
- kubectl configured

### Deploy to Cluster

```bash
cd crates/apollo_network/src/bin/broadcast_network_stress_test_node/run
python cluster_start.py --num-nodes 5 --latency 50 --throughput 1000
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

- **Latency Gating**: Add artificial delay to packets
- **Throughput Limiting**: Cap bandwidth to test under constrained conditions

Set via environment variables in the container:
- `LATENCY`: Delay in milliseconds
- `THROUGHPUT`: Bandwidth limit in KB/s

## Metrics

The tool exports the following Prometheus metrics:

### Message Metrics
- `sent_messages`: Total messages sent by this node
- `message_received`: Total messages received
- `message_received_from_{id}`: Messages received from specific peers

### Performance Metrics
- `message_received_delay_seconds`: End-to-end message latency
- `message_received_delay_seconds_from_{id}`: Latency from specific peers
- `message_received_delay_micros_sum`: Cumulative delay in microseconds

## Configuration

### Message Structure

Each stress test message contains:
- **ID**: Node identifier (4 bytes)
- **Timestamp**: Send time (12 bytes)
- **Peer ID**: Sender's P2P peer ID (38 bytes)
- **Payload**: Configurable data (remaining bytes)

### Network Topology

- All nodes join the same gossipsub topic: `stress_test_topic`
- Node 0 acts as the bootstrap peer for network discovery
- Deterministic peer IDs ensure consistent network formation

## Example Use Cases

### Latency Testing
```bash
# Test with 100ms network latency
python cluster_start.py --num-nodes 3 --latency 100 --message-size-bytes 512
```

### Throughput Testing
```bash
# Test with 500 KB/s bandwidth limit
python cluster_start.py --num-nodes 5 --throughput 500 --heartbeat-millis 10
```

### Large Message Testing
```bash
# Test with 64KB messages
python local.py --num-nodes 3 --message-size-bytes 65536 --heartbeat-millis 100
```

## Development

### File Structure

- `main.rs`: Core stress test logic and coordination
- `converters.rs`: Message serialization/deserialization
- `utils.rs`: Configuration and helper utilities
- `run/`: Deployment scripts and configurations
  - `local.py`: Local multi-node testing
  - `cluster_start.py`: Kubernetes deployment
  - `yaml_maker.py`: Kubernetes YAML generation
  - `Dockerfile`: Container image definition
  - `entrypoint.sh`: Container startup script with traffic shaping

### Adding New Metrics

1. Import metrics crate: `use metrics::{counter, gauge, histogram};`
2. Add metric recording in message handlers
3. Update Prometheus configuration in deployment scripts

### Network Configuration

Modify `NetworkConfig` parameters in `main.rs` for different P2P behaviors:
- Connection limits
- Heartbeat intervals
- Gossipsub parameters
- Discovery mechanisms

## Troubleshooting

### Common Issues

**Nodes not connecting**: Check bootstrap peer address and ensure firewall allows UDP traffic on P2P ports.

**High latency readings**: Verify system clocks are synchronized across test nodes.

**Prometheus not scraping**: Confirm metric ports are accessible and Prometheus configuration includes all node endpoints.

**Docker permission errors**: Ensure privileged mode is enabled for network traffic shaping.

### Debugging

Enable verbose logging for detailed P2P communication:
```bash
--verbosity 5
```

Check individual node logs in Kubernetes:
```bash
kubectl logs -n network-stress-test-{timestamp} network-stress-test-0
```
