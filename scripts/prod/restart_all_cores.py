#!/usr/bin/env python3

import urllib.error
import urllib.parse
import urllib.request
from typing import Optional

from common_lib import NamespaceAndInstructionArgs, RestartStrategy, Service, print_colored
from restarter_lib import ServiceRestarter
from update_config_and_restart_nodes_lib import (
    ApolloArgsParserBuilder,
    ConstConfigValuesUpdater,
    get_configmap,
    get_current_block_number,
    get_logs_explorer_url,
    parse_config_from_yaml,
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


class NodeValidatorIdCompositeUpdater(ConstConfigValuesUpdater):
    def __init__(self, config_overrides: dict[str, any], validator_id_start_from: int):
        super().__init__(config_overrides)
        self.validator_id_start_from = validator_id_start_from

    def get_updated_config_for_instance(
        self, config_data: dict[str, any], instance_index: int
    ) -> dict[str, any]:
        updated_config = super().get_updated_config_for_instance(config_data, instance_index)
        validator_id_as_hex = hex(self.validator_id_start_from + instance_index)
        updated_config["validator_id"] = validator_id_as_hex
        return updated_config


def main():
    usage_example = """
Examples:
  # Restart all nodes at once.
  %(prog)s --namespace-prefix apollo-sepolia-integration --num-nodes 3 --feeder-url feeder.integration-sepolia.starknet.io --project-name my-gcp-project
  %(prog)s -n apollo-sepolia-integration -m 3 -f feeder.integration-sepolia.starknet.io --project-name my-gcp-project
  
  # Restart nodes with cluster prefix
  %(prog)s -n apollo-sepolia-integration -m 3 -c my-cluster -f feeder.integration-sepolia.starknet.io --project-name my-gcp-project
  
  # Restart nodes starting from specific node index
  %(prog)s -n apollo-sepolia-integration -m 3 -s 5 -f feeder.integration-sepolia.starknet.io --project-name my-gcp-project
  
  # Use different feeder URL
  %(prog)s -n apollo-sepolia-integration -m 3 -f feeder.integration-sepolia.starknet.io --project-name my-gcp-project
  
  # Use namespace list instead of prefix (restart specific namespaces)
  %(prog)s --namespace-list apollo-sepolia-integration-0 apollo-sepolia-integration-2 -f feeder.integration-sepolia.starknet.io --project-name my-gcp-project
  %(prog)s -N apollo-sepolia-integration-0 apollo-sepolia-integration-2 -f feeder.integration-sepolia.starknet.io --project-name my-gcp-project
  
  # Use cluster list for multiple clusters (only works with namespace-list, not namespace-prefix)
  %(prog)s -N apollo-sepolia-integration-0 apollo-sepolia-integration-1 -C cluster1 cluster2 -f feeder.integration-sepolia.starknet.io --project-name my-gcp-project
  %(prog)s --namespace-list apollo-sepolia-integration-0 apollo-sepolia-integration-1 --cluster-list cluster1 cluster2 -f feeder.integration-sepolia.starknet.io --project-name my-gcp-project
        """

    args_builder = ApolloArgsParserBuilder(
        "Restart all nodes using the value from the feeder URL",
        usage_example,
        include_restart_strategy=False,
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
        required=True,
        help="The name of the project to get logs from.",
    )

    args_builder.add_argument(
        "--validator-id-start-from",
        type=int,
        help="If set, also update the validator ID config to this value + index of the instance being restarted. Value is in decimal format.",
    )

    args = args_builder.build()

    # Get current block number from feeder URL
    current_block_number = get_current_block_number(args.feeder_url)
    next_block_number = current_block_number + 1

    print_colored(f"Current block number: {current_block_number}")
    print_colored(f"Next block number: {next_block_number}")

    config_overrides = {
        "consensus_manager_config.cende_config.skip_write_height": next_block_number,
        "consensus_manager_config.immediate_active_height": next_block_number,
    }

    namespace_list = NamespaceAndInstructionArgs.get_namespace_list_from_args(args)
    context_list = NamespaceAndInstructionArgs.get_context_list_from_args(args)

    # Generate logs explorer URLs if needed
    post_restart_instructions = []

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

    updater = (
        NodeValidatorIdCompositeUpdater(config_overrides, args.validator_id_start_from)
        if args.validator_id_start_from
        else ConstConfigValuesUpdater(config_overrides)
    )

    update_config_and_restart_nodes(
        updater,
        namespace_and_instruction_args,
        Service.Core,
        restarter,
    )


if __name__ == "__main__":
    main()
