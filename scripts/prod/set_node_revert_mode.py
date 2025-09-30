#!/usr/bin/env python3

import sys
from typing import Optional

from update_config_and_restart_nodes_lib import (
    ApolloArgsParserBuilder,
    Service,
    get_context_list_from_args,
    get_current_block_number,
    get_namespace_list_from_args,
    print_colored,
    print_error,
    update_config_and_restart_nodes,
)


def set_revert_mode(
    namespace_list: list[str],
    context_list: Optional[list[str]],
    should_revert: bool,
    revert_up_to_block: int,
):
    config_overrides = {
        "revert_config.should_revert": should_revert,
        "revert_config.revert_up_to_and_including": revert_up_to_block,
    }

    update_config_and_restart_nodes(
        config_overrides,
        namespace_list,
        Service.Core,
        context_list,
        True,
    )


def main():
    usage_example = """
Examples:
  # Set revert mode up to a specific block
  %(prog)s --namespace apollo-sepolia-integration --num-nodes 3 --revert-only --revert_up_to_block 12345
  %(prog)s -n apollo-sepolia-integration -N 3 --revert-only -b 12345
  
  # Disable revert mode
  %(prog)s --namespace apollo-sepolia-integration --num-nodes 3 --disable-revert
  %(prog)s -n apollo-sepolia-integration -N 3 --disable-revert
  
  # Set revert mode with cluster prefix
  %(prog)s -n apollo-sepolia-integration -N 3 -c my-cluster --revert-only -b 12345
  
  # Disable revert mode without restarting nodes
  %(prog)s -n apollo-sepolia-integration -N 3 --disable-revert --no-restart
  
  # Set revert mode with explicit restart
  %(prog)s -n apollo-sepolia-integration -N 3 --revert-only -b 12345 -r
  
  # Set revert mode starting from specific node index
  %(prog)s -n apollo-sepolia-integration -N 3 -i 5 --revert-only -b 12345
        """

    args_builder = ApolloArgsParserBuilder(
        "Sets or unsets the revert mode for the sequencer nodes", usage_example
    )

    revert_group = args_builder.parser.add_mutually_exclusive_group()
    revert_group.add_argument("--revert-only", action="store_true", help="Enable revert mode")
    revert_group.add_argument("--disable-revert", action="store_true", help="Disable revert mode")

    args_builder.add_argument(
        "-b",
        "--revert-up-to-block",
        type=int,
        help="Block number up to which to revert. Must be a positive integer.",
    )

    args_builder.add_argument(
        "-f",
        "--feeder-url",
        type=str,
        help="The feeder URL to get the current block fro. We will revert all blocks above it.",
    )

    args = args_builder.build()

    # if (args.feeder_url is None) != (args.revert_up_to_block is None):
    should_revert = not args.disable_revert
    if should_revert:
        if args.feeder_url is None and args.revert_up_to_block is None:
            print_error(
                "Error: Either --feeder-url or --revert_up_to_block (-b) are required when reverting is requested."
            )
            sys.exit(1)
        if args.feeder_url is not None and args.revert_up_to_block is not None:
            print_error("Error: Cannot specify both --feeder-url and --revert_up_to_block (-b).")
            sys.exit(1)

    if args.disable_revert:
        if args.feeder_url is not None:
            print_error("Error: --feeder-url cannot be set when using --disable-revert")
            sys.exit(1)
        if args.revert_up_to_block is not None:
            print_error("Error: --revert_up_to_block (-b) cannot be set when disabling revert.")
            sys.exit(1)

    namespace_list = get_namespace_list_from_args(args)
    context_list = get_context_list_from_args(args)

    should_disable_revert = not args.revert_only
    if should_revert:
        revert_up_to_block = (
            args.revert_up_to_block
            if args.revert_up_to_block is not None
            else get_current_block_number(args.feeder_url)
        )
        f"\nEnabling revert mode up to (and including) block {revert_up_to_block}"
        set_revert_mode(namespace_list, context_list, True, revert_up_to_block)
    if should_disable_revert:
        print_colored(f"\nDisabling revert mode")
        # Setting to max block to max u64.
        set_revert_mode(namespace_list, context_list, False, 18446744073709551615)


if __name__ == "__main__":
    main()
