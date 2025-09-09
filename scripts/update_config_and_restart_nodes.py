#!/usr/bin/env python3

import json
import sys

import os

from update_config_and_restart_nodes_lib import (
    ArgsParserBuilder,
    update_config_and_restart_nodes,
    print_colored,
    print_error,
    Job,
    Colors,
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


def main():
    usage_example = """
Examples:
  # Update sequencer core configuration (default job)
  %(prog)s --namespace apollo-sepolia-integration --num-nodes 3 --cluster my-cluster --config-overrides consensus_manager_config.timeout=5000 --config-overrides validator_id=0x42
  %(prog)s -n apollo-sepolia-integration -N 3 --config-overrides consensus_manager_config.timeout=5000 --config-overrides validator_id=0x42
  
  # Update gateway configuration
  %(prog)s -n apollo-sepolia-integration -N 3 -j SequencerGateway --config-overrides gateway_config.port=8080
  
  # Update mempool configuration
  %(prog)s -n apollo-sepolia-integration -N 3 -j SequencerMempool --config-overrides mempool_config.max_size=1000
  
  # Update L1 provider configuration
  %(prog)s -n apollo-sepolia-integration -N 3 -j SequencerL1 --config-overrides l1_config.endpoint=https://eth-mainnet.alchemyapi.io/v2/your-key
  
  # Update without restart
  %(prog)s -n apollo-sepolia-integration -N 3 --config-overrides validator_id=0x42 --no-restart
  
  # Update with explicit restart
  %(prog)s -n apollo-sepolia-integration -N 3 --config-overrides validator_id=0x42 -r
  
  # Update starting from specific node index
  %(prog)s -n apollo-sepolia-integration -N 3 -i 5 --config-overrides validator_id=0x42
        """

    # Note: The config-overrides argument is already added by the builder as a required flag
    # No need to add it again here
    args_builder = ArgsParserBuilder(usage_example)
    args_builder.add_argument(
        "-j",
        "--job",
        type=lambda x: Job[x],  # Convert string to enum instance
        choices=list(Job),
        default=Job.SequencerCore,
        help="Job type to operate on; determines configmap and pod names (default: sequencer-core)",
    )
    args_builder.add_argument(
        "-o",
        "--config-overrides",
        action="append",
        help="Configuration overrides in key=value format. Can be specified multiple times. Example: --config-overrides consensus_manager_config.timeout=5000 --config-overrides validator_id=0x42",
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
        args.job,
        args.cluster,
        not args.no_restart,
    )


if __name__ == "__main__":
    main()
