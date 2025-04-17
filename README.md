# Sequencer

## About
Apollo is a Starknet sequencer implemented in Rust. It tracks Starknet’s state as it evolves over time and enables you to query this state and execute transactions via Starknet’s JSON-RPC.

## Disclaimer
Apollo is currently in development. Use it at your own risk.

## Getting Started
Run [the dependencies script](scripts/dependencies.sh) to set up your environment.

## Procedure

### 1. Fork and Clone
- Fork and clone the GitHub repository.

### 2. Build and Run the Apollo Sequencer
Create a data directory and launch the sequencer with a configuration file:
```bash
mkdir data
cargo run --bin apollo_sequencer_node -- --config_file <config_file>
```

## Local Node Configuration
The configuration is stored in one or more `.json` files. The default Apollo configuration file, [default_config.json](https://github.com/starkware-libs/sequencer/blob/main/config/sequencer/default_config.json), includes descriptions of all available parameters.  
*Note: It also provides pointers to common values and `#is_none` flags for optional parameters.*

## Customizing Your Configuration

You can customize the configuration in the following ways:

### A. Configuration Files (Local Node Only)
- **Multiple Files:** There is no limit on the number of custom configuration files.
- **Precedence:** If the same parameter appears in multiple files, the value from the last file takes precedence.

To create a custom configuration file, use the same `.json` format as the [node_config.json](https://github.com/starkware-libs/sequencer/blob/main/config/sequencer/presets/system_test_presets/single_node/node_0/executable_0/node_config.json) configuration file. Specify your custom files using the `--config_file` option:
```bash
cargo run --bin apollo_sequencer_node -- --config_file <path_to_custom_configuration_file_1> <path_to_custom_configuration_file_n>
```

**Note:** Apollo uses the `data` directory for node storage:
```
./data/
```
You can configure storage directories by editing and including the `storage_paths.json` preset:
```bash
cargo run --bin apollo_sequencer_node -- --config_file <config_file> storage_paths.json
```

### B. Configuration via the Command-Line
You can also pass configuration parameters as command-line options.
For example, to use the Sepolia testnet:
```bash
cargo run --bin apollo_sequencer_node -- --config_file <config_file> \
  --chain_id SN_SEPOLIA \
  --state_sync_config.central_sync_client_config.central_source_config.starknet_url https://alpha-sepolia.starknet.io/ \
  --base_layer_config.starknet_contract_address 0xe2bb56ee936fd6433dc0f6e7e3b8365c906aa057
```

### C. Editing Configuration Files via the Repository
After modifying a default value in a configuration struct, update the configuration by running:
```bash
cargo run --bin system_test_dump_single_node_config
```

## Prerequisites
- [Rust](https://www.rust-lang.org/tools/install) must be installed (minimum version `1.76`).
- Access to an Ethereum node is required (for example, via a provider such as Infura).
