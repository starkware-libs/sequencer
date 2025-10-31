#!/bin/bash

# Script to run apollo_node locally syncing with MAINNET.
set -e

# Base directory for config files.
CONFIG_BASE_DIR="crates/apollo_deployments/resources"

# Config files for mainnet sync.
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
    "deployments/mainnet/deployment_config_override.json"
    "deployments/mainnet/hybrid_0.json"
    "services/consolidated/node.json"
    "mainnet_secrets.json"
)

# Build the cargo command with all config files.
CARGO_CMD="cargo run --bin apollo_node --"

echo "Configuring for MAINNET sync..."
for config_file in "${CONFIG_FILES[@]}"; do
    full_path="${CONFIG_BASE_DIR}/${config_file}"
    if [ -f "$full_path" ]; then
        echo "Adding config file: $full_path"
        CARGO_CMD+=" --config_file ${full_path}"
    else
        echo "Config file not found: $full_path"
    fi
done

#check the saturation with batching
# check the difference in batch sizes
# check the cache faults in the disk maybe?
# check how far we are from the theoretic boundw
# if the txs go to filew we need to see what part is how ,uch time it takes to write to files and how ,uch it takes to write to the mdbx itself- because we need to be close to the optimize throughput (the max throughput)

# lior: focus the txs and investigate there- write them just to files without mmap- can be random data not txs- and then see the difference with mmap.
# the management of the writing to the files themselves should also be investigated- if the file is full we need to see what part is how ,uch time it takes to write to files and how ,uch it takes to write to the mdbx itself- because we need to be close to the optimize throughput (the max throughput)
# that its one big chunks and no tons of inner batches- because we need to be close to the optimize throughput (the max throughput)

# Set debug logging and start.
export RUST_LOG=debug
eval "${CARGO_CMD}"
