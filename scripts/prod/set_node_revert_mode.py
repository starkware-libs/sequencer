#!/usr/bin/env python3

import sys

from update_config_and_restart_nodes_lib import (
    ApolloArgsParserBuilder,
    print_colored,
    print_error,
    update_config_and_restart_nodes,
)


def main():
    usage_example = """
Examples:
  # Set revert mode up to a specific block
  %(prog)s --namespace apollo-sepolia-integration --num-nodes 3 revert --revert_up_to_block 12345
  %(prog)s -n apollo-sepolia-integration -N 3 revert -b 12345
  
  # Disable revert mode
  %(prog)s --namespace apollo-sepolia-integration --num-nodes 3 disable-revert
  %(prog)s -n apollo-sepolia-integration -N 3 disable-revert
  
  # Set revert mode with cluster prefix
  %(prog)s -n apollo-sepolia-integration -N 3 -c my-cluster revert -b 12345
  
  # Disable revert mode without restarting nodes
  %(prog)s -n apollo-sepolia-integration -N 3 disable-revert --no-restart
  
  # Set revert mode with explicit restart
  %(prog)s -n apollo-sepolia-integration -N 3 revert -b 12345 -r
  
  # Set revert mode starting from specific node index
  %(prog)s -n apollo-sepolia-integration -N 3 -i 5 revert -b 12345
        """

    args_builder = ApolloArgsParserBuilder(
        "Sets or unsets the revert mode for the sequencer nodes", usage_example
    )

    # Create subparsers for revert operations
    subparsers = args_builder.parser.add_subparsers(
        dest="command", help="Available commands", required=True
    )

    # Revert subcommand
    revert_parser = subparsers.add_parser("revert", help="Enable revert mode")
    revert_parser.add_argument(
        "-b",
        "--revert_up_to_block",
        type=int,
        required=True,
        help="Block number up to which to revert. Must be a positive integer.",
    )

    # No-revert subcommand
    subparsers.add_parser("disable-revert", help="Disable revert mode")

    args = args_builder.build()
    # Validate block number for revert command
    if args.command == "revert":
        if args.revert_up_to_block <= 0:
            print_error("Error: --revert_up_to_block (-b) must be a positive integer")
            sys.exit(1)

    # Add revert-specific configuration based on subcommand
    if args.command == "revert":
        should_revert = True
        revert_up_to_block = args.revert_up_to_block
        print_colored(
            f"\nEnabling revert mode up to (and including) block {args.revert_up_to_block}"
        )
    elif args.command == "disable-revert":
        should_revert = False
        revert_up_to_block = 18446744073709551615  # Max unit64.
        print_colored(f"\nDisabling revert mode")

    config_overrides = {
        "revert_config.should_revert": should_revert,
        "revert_config.revert_up_to_and_including": revert_up_to_block,
    }

    update_config_and_restart_nodes(
        config_overrides,
        args.namespace,
        args.num_nodes,
        args.start_index,
        args.cluster,
        not args.no_restart,
    )


if __name__ == "__main__":
    main()
