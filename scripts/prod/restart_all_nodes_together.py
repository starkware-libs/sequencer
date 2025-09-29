#!/usr/bin/env python3

import argparse
import json
import sys
from enum import Enum

import urllib.error
import urllib.request
from update_config_and_restart_nodes_lib import (
    ApolloArgsParserBuilder,
    Service,
    get_context_list_from_args,
    get_namespace_list_from_args,
    print_colored,
    print_error,
    update_config_and_restart_nodes,
)


class RestartStrategy(Enum):
    """Strategy for restarting nodes."""

    All_At_Once = 1
    One_By_One = 2


def restart_strategy_converter(strategy_name: str) -> RestartStrategy:
    """Convert string to RestartStrategy enum with informative error message"""
    RESTART_STRATEGY_PREFIX = f"{RestartStrategy.__name__}."
    if strategy_name.startswith(RESTART_STRATEGY_PREFIX):
        strategy_name = strategy_name[len(RESTART_STRATEGY_PREFIX) :]

    try:
        return RestartStrategy[strategy_name]
    except KeyError:
        valid_strategies = ", ".join([strategy.name for strategy in RestartStrategy])
        raise argparse.ArgumentTypeError(
            f"Invalid restart strategy '{strategy_name}'. Valid options are: {valid_strategies}"
        )


def main():
    usage_example = """
Examples:
  # Restart all nodes to at the next block after current feeder block
  %(prog)s --namespace-prefix apollo-sepolia-integration --num-nodes 3 --feeder_url feeder.integration-sepolia.starknet.io
  %(prog)s -n apollo-sepolia-integration -m 3 -f feeder.integration-sepolia.starknet.io
  
  # Restart nodes with cluster prefix
  %(prog)s -n apollo-sepolia-integration -m 3 -c my-cluster -f feeder.integration-sepolia.starknet.io
  
  # Update configuration without restarting nodes
  %(prog)s -n apollo-sepolia-integration -m 3 -f feeder.integration-sepolia.starknet.io --no-restart
  
  # Restart nodes starting from specific node index
  %(prog)s -n apollo-sepolia-integration -m 3 -s 5 -f feeder.integration-sepolia.starknet.io
  
  # Use different feeder URL
  %(prog)s -n apollo-sepolia-integration -m 3 -f feeder.integration-sepolia.starknet.io
  
  # Use namespace list instead of prefix (restart specific namespaces)
  %(prog)s --namespace-list apollo-sepolia-integration-0 apollo-sepolia-integration-2 -f feeder.integration-sepolia.starknet.io
  %(prog)s -N apollo-sepolia-integration-0 apollo-sepolia-integration-2 -f feeder.integration-sepolia.starknet.io
  
  # Use cluster list for multiple clusters (only works with namespace-list, not namespace-prefix)
  %(prog)s -N apollo-sepolia-integration-0 apollo-sepolia-integration-1 -C cluster1 cluster2 -f feeder.integration-sepolia.starknet.io
  %(prog)s --namespace-list apollo-sepolia-integration-0 apollo-sepolia-integration-1 --cluster-list cluster1 cluster2 -f feeder.integration-sepolia.starknet.io
        """

    args_builder = ApolloArgsParserBuilder(
        "Restart all nodes using the value from the feeder URL", usage_example
    )

    args_builder.add_argument(
        "-f",
        "--feeder_url",
        required=True,
        type=str,
        help="The feeder URL to get the current block from",
    )

    args_builder.add_argument(
        "-r",
        "--restart-strategy",
        type=restart_strategy_converter,
        choices=list(RestartStrategy),
        default=RestartStrategy.All_At_Once,
        help="Strategy for restarting nodes (default: All_At_Once)",
    )

    args = args_builder.build()

    # Get current block number from feeder URL
    try:
        url = f"https://{args.feeder_url}/feeder_gateway/get_block"
        with urllib.request.urlopen(url) as response:
            if response.status != 200:
                raise urllib.error.HTTPError(
                    url, response.status, "HTTP Error", response.headers, None
                )
            data = json.loads(response.read().decode("utf-8"))
            current_block_number = data["block_number"]
            next_block_number = current_block_number + 1

            print_colored(f"Current block number: {current_block_number}")
            print_colored(f"Next block number: {next_block_number}")

    except urllib.error.URLError as e:
        print_error(f"Failed to fetch block number from feeder URL: {e}")
        sys.exit(1)
    except KeyError as e:
        print_error(f"Unexpected response format from feeder URL: {e}")
        sys.exit(1)
    except json.JSONDecodeError as e:
        print_error(f"Failed to parse JSON response from feeder URL: {e}")
        sys.exit(1)

    config_overrides = {
        "consensus_manager_config.immediate_active_height": next_block_number,
        "consensus_manager_config.cende_config.skip_write_height": next_block_number,
    }

    update_config_and_restart_nodes(
        config_overrides,
        get_namespace_list_from_args(args),
        Service.Core,
        get_context_list_from_args(args),
        not args.no_restart,
    )


if __name__ == "__main__":
    main()
