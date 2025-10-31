#!/bin/bash

# Script to run apollo_node locally syncing with SEPOLIA testnet
set -e

echo "🌐 Starting Sepolia testnet node..."
echo "This will sync with Starknet Sepolia testnet using real external services."

# Create data directories (skip if already exist to avoid sudo prompt in VTune)
if [ ! -d "/data/batcher" ]; then
    echo "Creating data directories..."
    sudo mkdir -p /data/{batcher,class_manager,state_sync}
    sudo chmod 755 /data/{batcher,class_manager,state_sync}
else
    echo "Data directories already exist, skipping creation..."
fi

# Base directory for config files
CONFIG_BASE_DIR="crates/apollo_deployments/resources"

# Config files for Sepolia testnet sync
CONFIG_FILES=(
    "app_configs/base_layer_config.json"
    "app_configs/batcher_config.json"
    "app_configs/class_manager_config.json"
    "app_configs/consensus_manager_config.json"
    "app_configs/revert_config.json"
    "app_configs/versioned_constants_overrides_config.json"
    "app_configs/validate_resource_bounds_config.json"
    "app_configs/gateway_config.json"
    "app_configs/http_server_config.json"
    "app_configs/l1_endpoint_monitor_config.json"
    "app_configs/l1_gas_price_provider_config.json"
    "app_configs/l1_gas_price_scraper_config.json"
    "app_configs/l1_provider_config.json"
    "app_configs/l1_scraper_config.json"
    "app_configs/mempool_config.json"
    "app_configs/mempool_p2p_config.json"
    "app_configs/monitoring_endpoint_config.json"
    "app_configs/sierra_compiler_config.json"
    "app_configs/state_sync_config.json"
    "deployments/sepolia_testnet/deployment_config_override.json"
    
    "deployments/sepolia_testnet/hybrid_0.json"
    "services/consolidated/node.json"
    "port_override_for_vtune.json"
    "sepolia_secrets.json"
)

# Build the cargo command with all config files (release mode with debug symbols)
CARGO_CMD="cargo run --release --bin apollo_node --"
#CARGO_CMD="cargo build --release --bin apollo_node --"

echo "Configuring for SEPOLIA testnet sync..."
for config_file in "${CONFIG_FILES[@]}"; do
    full_path="${CONFIG_BASE_DIR}/${config_file}"
    if [ -f "$full_path" ]; then
        echo "Adding config file: $full_path"
        CARGO_CMD+=" --config_file ${full_path}"
    else
        echo "Config file not found: $full_path"
    fi
done

echo ""
echo "Starting SEPOLIA testnet sync node..."
echo "This will sync with real Starknet Sepolia testnet data!"
echo "Monitor flush performance with: ./scripts/monitor_flush_realtime.sh"
echo ""
echo "Running command:"
echo "$CARGO_CMD"
echo ""

# Set debug logging and start
export RUST_LOG=debug
eval "${CARGO_CMD}"
