#!/usr/bin/env python3

import json
import sys

import urllib.error
import urllib.request
from update_config_and_restart_nodes_lib import (
    ArgsParserBuilder,
    Service,
    print_colored,
    print_error,
    update_config_and_restart_nodes,
)


def main():
    usage_example = """
Examples:
  # Restart all nodes to at the next block after current feeder block
  %(prog)s --namespace apollo-sepolia-integration --num-nodes 3 --feeder.integration-sepolia.starknet.io
  %(prog)s -n apollo-sepolia-integration -N 3 -f feeder.integration-sepolia.starknet.io
  
  # Restart nodes with cluster prefix
  %(prog)s -n apollo-sepolia-integration -N 3 -c my-cluster -f feeder.integration-sepolia.starknet.io
  
  # Update configuration without restarting nodes
  %(prog)s -n apollo-sepolia-integration -N 3 -f feeder.integration-sepolia.starknet.io --no-restart
  
  # Restart nodes starting from specific node index
  %(prog)s -n apollo-sepolia-integration -N 3 -s 5 -f feeder.integration-sepolia.starknet.io
  
  # Use different feeder URL
  %(prog)s -n apollo-sepolia-integration -N 3 -f feeder.integration-sepolia.starknet.io
        """

    args_builder = ArgsParserBuilder(
        "Restart all nodes using the value from the feeder URL", usage_example
    )

    args_builder.add_argument(
        "-f",
        "--feeder_url",
        required=True,
        type=str,
        help="The feeder URL to get the current block from",
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
        args.namespace,
        args.num_nodes,
        args.start_index,
        Service.Core,
        args.cluster,
        not args.no_restart,
    )


if __name__ == "__main__":
    main()
