#!/usr/bin/env python3

import sys
from typing import Optional

import urllib.parse
from update_config_and_restart_nodes_lib import (
    ApolloArgsParserBuilder,
    NamespaceAndInstructionArgs,
    RestartStrategy,
    Service,
    ServiceRestarter,
    get_current_block_number,
    get_logs_explorer_url,
    print_colored,
    print_error,
    update_config_and_restart_nodes,
)


# TODO(guy.f): Remove this once we have metrics we use to decide based on.
def get_logs_explorer_url_for_revert(
    namespace: str,
    block_number: int,
    project_name: str,
) -> str:
    query = (
        f'resource.labels.namespace_name:"{urllib.parse.quote(namespace)}"\n'
        f'resource.labels.container_name="sequencer-core"\n'
        f'textPayload =~ "Done reverting.*storage up to height {block_number}"'
    )
    return get_logs_explorer_url(query, project_name)


def set_revert_mode(
    namespace_list: list[str],
    context_list: Optional[list[str]],
    project_name: str,
    should_revert: bool,
    revert_up_to_block: int,
):
    config_overrides = {
        "revert_config.should_revert": should_revert,
        "revert_config.revert_up_to_and_including": revert_up_to_block,
    }

    post_restart_instructions = []
    for namespace, context in zip(namespace_list, context_list or [None] * len(namespace_list)):
        url = get_logs_explorer_url_for_revert(namespace, revert_up_to_block, project_name)

        post_restart_instructions.append(
            f"Please check logs and verify that revert has completed (both in the batcher and for sync). Logs URL: {url}"
        )

    namespace_and_instruction_args = NamespaceAndInstructionArgs(
        namespace_list,
        context_list,
        post_restart_instructions,
    )
    restarter = ServiceRestarter.from_restart_strategy(
        RestartStrategy.ALL_AT_ONCE,
        namespace_and_instruction_args,
        Service.Core,
    )

    update_config_and_restart_nodes(
        config_overrides,
        namespace_and_instruction_args,
        Service.Core,
        restarter,
    )


def main():
    usage_example = """
Examples:
  # Set revert mode up to a specific block
  %(prog)s --namespace apollo-sepolia-integration --num-nodes 3 --revert-only --revert_up_to_block 12345
  %(prog)s -n apollo-sepolia-integration -N 3 --revert-only -b 12345
  
  # Set revert mode using feeder URL to get current block
  %(prog)s --namespace apollo-sepolia-integration --num-nodes 3 --revert-only --feeder-url feeder.integration-sepolia.starknet.io   
  %(prog)s -n apollo-sepolia-integration -N 3 --revert-only -f feeder.integration-sepolia.starknet.io
  
  # Disable revert mode
  %(prog)s --namespace apollo-sepolia-integration --num-nodes 3 --disable-revert-only
  %(prog)s -n apollo-sepolia-integration -N 3 --disable-revert-only
  
  # Set revert mode with cluster prefix
  %(prog)s -n apollo-sepolia-integration -N 3 -c my-cluster --revert-only -b 12345
  
  # Set revert mode with feeder URL and cluster prefix
  %(prog)s -n apollo-sepolia-integration -N 3 -c my-cluster --revert-only -f feeder.integration-sepolia.starknet.io
  
  # Set revert mode starting from specific node index
  %(prog)s -n apollo-sepolia-integration -N 3 -i 5 --revert-only -b 12345
  
  # Set revert mode with feeder URL starting from specific node index
  %(prog)s -n apollo-sepolia-integration -N 3 -i 5 --revert-only -f feeder.integration-sepolia.starknet.io
        """

    args_builder = ApolloArgsParserBuilder(
        "Sets or unsets the revert mode for the sequencer nodes",
        usage_example,
        include_restart_strategy=False,
    )

    revert_group = args_builder.parser.add_mutually_exclusive_group()
    revert_group.add_argument("--revert-only", action="store_true", help="Enable revert mode")
    revert_group.add_argument(
        "--disable-revert-only", action="store_true", help="Disable revert mode"
    )

    args_builder.add_argument(
        "-b",
        "--revert-up-to-block",
        type=int,
        help="Block number up to which to revert (inclusive). Must be a positive integer.",
    )

    args_builder.add_argument(
        "-f",
        "--feeder-url",
        type=str,
        help="The feeder URL to get the current block from. We will revert all blocks above it.",
    )

    # TODO(guy.f): Remove this when we rely on metrics for restarting.
    args_builder.add_argument(
        "--project-name",
        required=True,
        help="The name of the project to get logs from. If One_By_One strategy is used, this is required.",
    )

    args = args_builder.build()

    should_revert = not args.disable_revert_only
    if should_revert:
        if args.feeder_url is None and args.revert_up_to_block is None:
            print_error(
                "Error: Either --feeder-url or --revert_up_to_block (-b) are required when reverting is requested."
            )
            sys.exit(1)
        if args.feeder_url is not None and args.revert_up_to_block is not None:
            print_error("Error: Cannot specify both --feeder-url and --revert_up_to_block (-b).")
            sys.exit(1)

    if args.disable_revert_only:
        if args.feeder_url is not None:
            print_error("Error: --feeder-url cannot be set when using --disable-revert-only")
            sys.exit(1)
        if args.revert_up_to_block is not None:
            print_error("Error: --revert-up-to-block (-b) cannot be set when disabling revert.")
            sys.exit(1)

    namespace_list = NamespaceAndInstructionArgs.get_namespace_list_from_args(args)
    context_list = NamespaceAndInstructionArgs.get_context_list_from_args(args)

    should_disable_revert = not args.revert_only
    if should_revert:
        revert_up_to_block = (
            args.revert_up_to_block
            if args.revert_up_to_block is not None
            else get_current_block_number(args.feeder_url)
        )
        f"\nEnabling revert mode up to (and including) block {revert_up_to_block}"
        set_revert_mode(
            namespace_list,
            context_list,
            args.project_name,
            True,
            revert_up_to_block,
        )
    if should_disable_revert:
        print_colored(f"\nDisabling revert mode")
        # Setting to max block to max u64.
        set_revert_mode(
            namespace_list,
            context_list,
            args.project_name,
            False,
            18446744073709551615,
        )


if __name__ == "__main__":
    main()
