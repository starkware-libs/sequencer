# Apollo Node Running & Monitoring Tools

## Usage

```bash
# Start the local node (test environment)
./scripts/run_local_node.sh

# Start the Sepolia testnet node (real Sepolia network, working configuration)
RUST_LOG=debug ./scripts/run_sepolia_node.sh

# Start the mainnet node (external services, real data) - may have configuration issues
RUST_LOG=debug ./scripts/run_mainnet_node.sh

# Real-time flush monitoring
./scripts/monitor_flush_realtime.sh

# One-time status report
./scripts/investigate_sync.sh

# Continuous monitoring (updates every 10 seconds)
watch -n 10 './scripts/investigate_sync.sh'
```
