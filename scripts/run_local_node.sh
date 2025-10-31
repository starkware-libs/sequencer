#!/bin/bash

# Script to run apollo_node locally with all config files from deployment configuration.
set -e

# Base directory for config files.
CONFIG_BASE_DIR="crates/apollo_deployments/resources"

# Config files from deployment_config_consolidated.json.
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
    "deployments/testing/deployment_config_override.json"
    "deployments/testing/consolidated.json"
    "services/consolidated/node.json"
    "testing_secrets.json"
)

# Build the cargo command with all config files.
CARGO_CMD="cargo run --bin apollo_node --"

# Add each config file as a --config_file argument.
for config_file in "${CONFIG_FILES[@]}"; do
    full_path="${CONFIG_BASE_DIR}/${config_file}"
    if [ -f "$full_path" ]; then
        CARGO_CMD="$CARGO_CMD --config_file $full_path"
        echo "Adding config file: $full_path"
    else
        echo "Warning: Config file not found: $full_path"
    fi
done

echo ""
echo "Running command:"
echo "$CARGO_CMD"
echo ""

# Execute the command.
eval $CARGO_CMD
