#!/usr/bin/env python3

import argparse
import json
import sys

from update_config_and_restart_nodes_lib import (
    ApolloArgsParserBuilder,
    Colors,
    Service,
    print_colored,
    print_error,
    update_config_and_restart_nodes,
)


def parse_config_overrides(config_overrides: list[str]) -> dict[str, any]:
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


def service_type_converter(service_name: str) -> Service:
    """Convert string to Service enum with informative error message"""
    if service_name.startswith("Service."):
        service_name = service_name[8:]

    try:
        return Service[service_name]
    except KeyError:
        valid_services = ", ".join([service.name for service in Service])
        raise argparse.ArgumentTypeError(
            f"Invalid service type '{service_name}'. Valid options are: {valid_services}"
        )


def main():
    usage_example = """
Examples:
  # Update sequencer core configuration.
  %(prog)s --namespace apollo-sepolia-integration --num-nodes 3 --cluster my-cluster --config-overrides consensus_manager_config.timeout=5000 --config-overrides validator_id=0x42
  %(prog)s -n apollo-sepolia-integration -N 3 --config-overrides consensus_manager_config.timeout=5000 --config-overrides validator_id=0x42
  
  # Update gateway configuration
  %(prog)s -n apollo-sepolia-integration -N 3 -j Gateway --config-overrides gateway_config.port=8080
  
  # Update mempool configuration
  %(prog)s -n apollo-sepolia-integration -N 3 -j Mempool --config-overrides mempool_config.max_size=1000
  
  # Update L1 provider configuration
  %(prog)s -n apollo-sepolia-integration -N 3 -j L1 --config-overrides l1_config.endpoint=\"https://eth-mainnet.alchemyapi.io/v2/your-key\"
  
  # Update HTTP server configuration
  %(prog)s -n apollo-sepolia-integration -N 3 -j HttpServer --config-overrides http_server_config.port=8081
  
  # Update without restart
  %(prog)s -n apollo-sepolia-integration -N 3 --config-overrides validator_id=0x42 --no-restart
  
  # Update with explicit restart
  %(prog)s -n apollo-sepolia-integration -N 3 --config-overrides validator_id=0x42 -r
  
  # Update starting from specific node index
  %(prog)s -n apollo-sepolia-integration -N 3 -i 5 --config-overrides validator_id=0x42
  
        """

    args_builder = ApolloArgsParserBuilder(
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

    args_builder.add_argument(
        "-j",
        "--service",
        type=service_type_converter,
        choices=list(Service),
        default=Service.Core,
        help="Service type to operate on; determines configmap and pod names (default: Core)",
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
        args.namespace,
        args.num_nodes,
        args.start_index,
        args.service,
        args.cluster,
        not args.no_restart,
    )


if __name__ == "__main__":
    main()
