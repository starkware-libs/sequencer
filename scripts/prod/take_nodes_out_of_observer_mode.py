#!/usr/bin/env python3

import urllib.error
import urllib.parse
import urllib.request
from typing import Optional

from common_lib import NamespaceAndInstructionArgs, RestartStrategy, Service
from restarter_lib import ServiceRestarter
from update_config_and_restart_nodes_lib import (
    ApolloArgsParserBuilder,
    ConfigValuesUpdater,
    get_configmap,
    get_logs_explorer_url,
    parse_config_from_yaml,
    update_config_and_restart_nodes,
)


# TODO(guy.f): Remove this once we have metrics we use to decide based on.
def get_logs_explorer_url_for_proposal(
    namespace: str,
    validator_id: str,
    project_name: str,
) -> str:
    # Remove the 0x prefix from the validator id to get the number.
    validator_id = validator_id[2:]

    query = (
        f'resource.labels.namespace_name:"{urllib.parse.quote(namespace)}"\n'
        f'resource.labels.container_name="sequencer-core"\n'
        f'textPayload =~ "DECISION_REACHED:.*proposer 0x0*{validator_id}"'
    )
    return get_logs_explorer_url(query, project_name)


def get_validator_id(namespace: str, context: Optional[str]) -> str:
    # Get current config and normalize it (e.g. " vs ') to ensure not showing bogus diffs.
    original_config = get_configmap(namespace, context, Service.Core)
    _, config_data = parse_config_from_yaml(original_config)

    return config_data["validator_id"]


class NodeValidatorIdUpdater(ConfigValuesUpdater):
    def __init__(self, validator_id_start_from: int):
        self.validator_id_start_from = validator_id_start_from

    def get_updated_config_for_instance(
        self, config_data: dict[str, any], instance_index: int
    ) -> dict[str, any]:
        updated_config = config_data.copy()
        validator_id_as_hex = hex(self.validator_id_start_from + instance_index)
        updated_config["validator_id"] = validator_id_as_hex
        return updated_config


def main():
    usage_example = """
Examples:
  # Take all nodes out of observer mode at once.
  %(prog)s --namespace-prefix apollo-sepolia-integration --num-nodes 3 --project-name my-gcp-project --validator-id-start-from 64
  %(prog)s -n apollo-sepolia-integration -m 3 --project-name my-gcp-project --validator-id-start-from 64

  # Take nodes out of observer mode with cluster prefix
  %(prog)s -n apollo-sepolia-integration -m 3 -c my-cluster --project-name my-gcp-project --validator-id-start-from 64

  # Take nodes out of observer mode starting from a specific node index
  %(prog)s -n apollo-sepolia-integration -m 3 -s 5 --project-name my-gcp-project --validator-id-start-from 64

  # Use namespace list instead of prefix (operate on specific namespaces)
  %(prog)s --namespace-list apollo-sepolia-integration-0 apollo-sepolia-integration-2 --project-name my-gcp-project --validator-id-start-from 64
  %(prog)s -N apollo-sepolia-integration-0 apollo-sepolia-integration-2 --project-name my-gcp-project --validator-id-start-from 64

  # Use cluster list for multiple clusters (only works with namespace-list, not namespace-prefix)
  %(prog)s -N apollo-sepolia-integration-0 apollo-sepolia-integration-1 -C cluster1 cluster2 --project-name my-gcp-project --validator-id-start-from 64
  %(prog)s --namespace-list apollo-sepolia-integration-0 apollo-sepolia-integration-1 --cluster-list cluster1 cluster2 --project-name my-gcp-project --validator-id-start-from 64
        """

    args_builder = ApolloArgsParserBuilder(
        "Take nodes out of observer mode by assigning validator IDs and restarting them",
        usage_example,
        include_restart_strategy=False,
    )

    # TODO(guy.f): Remove this when we rely on metrics for restarting.
    args_builder.add_argument(
        "--project-name",
        default=None,
        help="The name of the GCP project to get logs from. When provided, log explorer URLs are printed after restart.",
    )

    args_builder.add_argument(
        "--validator-id-start-from",
        required=True,
        type=int,
        help="Update the validator ID config to this value + index of the instance being restarted. Value is in decimal format.",
    )

    args = args_builder.build()

    namespace_list = NamespaceAndInstructionArgs.get_namespace_list_from_args(args)
    context_list = NamespaceAndInstructionArgs.get_context_list_from_args(args)

    post_restart_instructions = []

    for namespace, context in zip(namespace_list, context_list or [None] * len(namespace_list)):
        instruction = (
            "Please check logs and verify that the nodes have proposed a block that was accepted"
        )
        if args.project_name is not None:
            url = get_logs_explorer_url_for_proposal(
                namespace,
                get_validator_id(namespace, context),
                args.project_name,
            )
            instruction = f"{instruction}. Logs URL: {url}"
        post_restart_instructions.append(instruction)

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
        NodeValidatorIdUpdater(args.validator_id_start_from),
        namespace_and_instruction_args,
        Service.Core,
        restarter,
        args.max_parallelism,
    )


if __name__ == "__main__":
    main()
