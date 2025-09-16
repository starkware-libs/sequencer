#!/usr/bin/env python3

import sys

from update_config_and_restart_nodes_lib import (
    ArgsParserBuilder,
    update_config_and_restart_nodes,
    print_colored,
    print_error,
    Job,
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

    args_builder = ArgsParserBuilder(
        "Sets or unsets the revert mode for the sequencer nodes", usage_example
    )

    args_builder.add_argument(
        "-f",
        "--feeder_url",
        required=True,
        type=str,
        help="The feeder URL to get the current block from",
    )

    args = args_builder.build()

    config_overrides = {
        "revert_config.should_revert": should_revert,
        "revert_config.revert_up_to_and_including": revert_up_to_block,
    }

    update_config_and_restart_nodes(
        config_overrides,
        args.namespace,
        args.num_nodes,
        args.start_index,
        Job.SequencerCore,
        args.cluster,
        not args.no_restart,
    )


if __name__ == "__main__":
    main()
