#!/usr/bin/env python3

import argparse
import json
import sys
from enum import Enum
from typing import Optional

import urllib.error
import urllib.parse
import urllib.request
from update_config_and_restart_nodes_lib import (
    ApolloArgsParserBuilder,
    RestartStrategy,
    Service,
    get_configmap,
    get_context_list_from_args,
    get_current_block_number,
    get_logs_explorer_url,
    get_namespace_list_from_args,
    parse_config_from_yaml,
    print_colored,
    print_error,
    update_config_and_restart_nodes,
)


# TODO(guy.f): Remove this once we have metrics we use to decide based on.
def get_logs_explorer_url_for_proposal(
    namespace: str,
    validator_id: str,
    min_block_number: int,
    project_name: str,
) -> str:
    # Remove the 0x prefix from the validator id to get the number.
    validator_id = validator_id[2:]

    query = (
        f'resource.labels.namespace_name:"{urllib.parse.quote(namespace)}"\n'
        f'resource.labels.container_name="sequencer-core"\n'
        f'textPayload =~ "DECISION_REACHED:.*proposer 0x0*{validator_id}"\n'
        f'CAST(REGEXP_EXTRACT(textPayload, "height: (\\\\d+)"), "INT64") > {min_block_number}'
    )
    return get_logs_explorer_url(query, project_name)


def get_validator_id(namespace: str, context: Optional[str]) -> str:
    # Get current config and normalize it (e.g. " vs ') to ensure not showing bogus diffs.
    original_config = get_configmap(namespace, context, Service.Core)
    _, config_data = parse_config_from_yaml(original_config)

    return config_data["validator_id"]


def main():
    usage_example = """
Examples:
  # Restart all nodes to at the next block after current feeder block (default: One_By_One strategy)
  %(prog)s --namespace-prefix apollo-sepolia-integration --num-nodes 3 --feeder-url feeder.integration-sepolia.starknet.io
  %(prog)s -n apollo-sepolia-integration -m 3 -f feeder.integration-sepolia.starknet.io
  
  # Restart nodes one by one with project name for showing logs link
  %(prog)s -n apollo-sepolia-integration -m 3 -f feeder.integration-sepolia.starknet.io -t One_By_One --project-name my-gcp-project
  
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
        "--feeder-url",
        required=True,
        type=str,
        help="The feeder URL to get the current block from",
    )

    # TODO(guy.f): Remove this when we rely on metrics for restarting.
    args_builder.add_argument(
        "--project-name",
        help="The name of the project to get logs from. If One_By_One strategy is used, this is required.",
    )

    args = args_builder.build()

    if args.restart_strategy == RestartStrategy.ONE_BY_ONE and args.project_name is None:
        print_error("Error: --project-name is required when using One_By_One strategy")
        sys.exit(1)

    # Get current block number from feeder URL
    current_block_number = get_current_block_number(args.feeder_url)
    next_block_number = current_block_number + 1

    print_colored(f"Current block number: {current_block_number}")
    print_colored(f"Next block number: {next_block_number}")

    config_overrides = {
        "consensus_manager_config.cende_config.skip_write_height": next_block_number,
    }

    if args.restart_strategy != RestartStrategy.ONE_BY_ONE:
        config_overrides["consensus_manager_config.immediate_active_height"] = next_block_number

    namespace_list = get_namespace_list_from_args(args)
    context_list = get_context_list_from_args(args)
    if context_list is not None:
        assert len(namespace_list) == len(
            context_list
        ), "namespace_list and context_list must have the same length"

    # Generate logs explorer URLs if needed
    post_restart_instructions = []
    if args.restart_strategy == RestartStrategy.ONE_BY_ONE:
        for namespace, context in zip(namespace_list, context_list or [None] * len(namespace_list)):
            url = get_logs_explorer_url_for_proposal(
                namespace,
                get_validator_id(namespace, context),
                # Feeder could be behind by up to 10 blocks, so we add 10 to the current block number.
                current_block_number + 10,
                args.project_name,
            )
            post_restart_instructions.append(
                f"Please check logs and verify that the node has proposed a block that was accepted. Logs URL: {url}"
            )
    else:
        post_restart_instructions.extend([""] * len(namespace_list))

    update_config_and_restart_nodes(
        config_overrides,
        namespace_list,
        Service.Core,
        context_list,
        args.restart_strategy,
        post_restart_instructions,
    )


if __name__ == "__main__":
    main()
