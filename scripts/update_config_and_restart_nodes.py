#!/usr/bin/env python3

import json
import sys

from typing import Any
from update_config_and_restart_nodes_lib import (
    ArgsParserBuilder,
    Colors,
    get_context_list_from_args,
    get_namespace_list_from_args,
    print_colored,
    print_error,
    update_config_and_restart_nodes,
)


def parse_config_overrides(config_overrides: list[str]) -> dict[str, Any]:
    """Parse config override strings in key=value format.

    Args:
        config_overrides: List of strings in "key=value" format

    Returns:
        dict: Dictionary mapping config keys to their values
    """
    if not config_overrides:
        return {}

    overrides = {}
    for override in config_overrides:
        if "=" not in override:
            print_colored(
                f"Error: Invalid config override format '{override}'. Expected 'key=value'",
                Colors.RED,
                file=sys.stderr,
            )
            sys.exit(1)

        # Split only on first '=' in case value contains '='
        key, value = override.split("=", 1)
        key = key.strip()
        value = value.strip()

        if not key:
            print_error(f"Error: Empty key in config override '{override}'")
            sys.exit(1)

        # Try to convert value to appropriate type
        try:
            overrides[key] = json.loads(value)
        except (json.JSONDecodeError, TypeError) as e:
            print_error(
                f"Error: Invalid value '{value}' for key '{key}': {e}\n"
                'Did you remember to wrap string values in \\" ?'
            )
            sys.exit(1)

    if not overrides:
        print_error("Error: No valid config overrides found")
        sys.exit(1)

    return overrides


def main():
    usage_example = """
Examples:
  # Basic usage with namespace prefix and node count
  %(prog)s -n apollo-sepolia-integration -N 3 --config-overrides consensus_manager_config.timeout=5000 --config-overrides validator_id=0x42
  
  # Using namespace list mode (no num-nodes or start-index allowed)
  %(prog)s --namespace-list apollo-sepolia-test-0 apollo-sepolia-test-1 apollo-sepolia-test-2 --config-overrides consensus_manager_config.timeout=5000
  
  # Using cluster prefix with namespace prefix
  %(prog)s -n apollo-sepolia-integration -N 3 -c my-cluster --config-overrides validator_id=0x42
  
  # Using cluster list with namespace list (must have same number of items)
  %(prog)s --namespace-list apollo-sepolia-test-0 apollo-sepolia-test-2 --cluster-list cluster0 cluster2 --config-overrides validator_id=0x42
  
  # Update different service types
  %(prog)s -n apollo-sepolia-integration -N 3 -j Gateway --config-overrides gateway_config.port=8080
  %(prog)s -n apollo-sepolia-integration -N 3 -j Mempool --config-overrides mempool_config.max_size=1000
  %(prog)s -n apollo-sepolia-integration -N 3 -j L1 --config-overrides l1_config.endpoint=\"https://eth-mainnet.alchemyapi.io/v2/your-key\"
  %(prog)s -n apollo-sepolia-integration -N 3 -j HttpServer --config-overrides http_server_config.port=8081
  %(prog)s -n apollo-sepolia-integration -N 3 -j SierraCompiler --config-overrides sierra_compiler_config.timeout=30000
  
  # Update starting from specific node index
  %(prog)s -n apollo-sepolia-integration -N 3 -s 5 --config-overrides validator_id=0x42
  
  # Update without restart
  %(prog)s -n apollo-sepolia-integration -N 3 --config-overrides validator_id=0x42 --no-restart
  
  # Update with explicit restart (default behavior)
  %(prog)s -n apollo-sepolia-integration -N 3 --config-overrides validator_id=0x42 -r
  
  # Complex example with multiple config overrides
  %(prog)s -n apollo-sepolia-integration -N 3 -c my-cluster -j Core --config-overrides consensus_manager_config.timeout=5000 --config-overrides validator_id=0x42 --config-overrides components.gateway.url=\"localhost\"
  
        """

    args_builder = ArgsParserBuilder(
        "Update configuration for Apollo sequencer nodes and (optionally) restart them",
        usage_example,
    )

    args_builder.add_argument(
        "-o",
        "--config-overrides",
        action="append",
        help="Configuration overrides in key=value format. Can be specified multiple times. "
        "Example: --config-overrides consensus_manager_config.timeout=5000 "
        '--config-overrides components.gateway.url=\\"localhost\\" (note the escaping of the ")',
    )

    args = args_builder.build()
    config_overrides = parse_config_overrides(args.config_overrides)

    if config_overrides:
        print_colored(f"\nConfig overrides to apply:")
        for key, value in config_overrides.items():
            print_colored(f"  {key} = {value}")
    else:
        print_error("No config overrides provided")
        sys.exit(1)

    update_config_and_restart_nodes(
        config_overrides,
        get_namespace_list_from_args(args),
        args.service,
        get_context_list_from_args(args),
        not args.no_restart,
    )


if __name__ == "__main__":
    main()
