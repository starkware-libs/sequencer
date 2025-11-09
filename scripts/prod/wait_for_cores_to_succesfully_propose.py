#!/usr/bin/env python3

import sys

from common_lib import (
    NamespaceAndInstructionArgs,
    RestartStrategy,
    Service,
    print_error,
    wait_until_y_or_n,
)
from metrics_lib import MetricConditionGater
from restarter_lib import WaitOnMetricRestarter
from update_config_and_restart_nodes_lib import (
    ApolloArgsParserBuilder,
    update_config_and_restart_nodes,
)


class WaitForProposalIncrease:
    def __init__(self):
        self.last_proposal_count = None

    def check_if_proposal_count_increased(self, proposal_count: int) -> bool:
        if self.last_proposal_count is None:
            # First time we're checking, so we don't know if the proposal count has increased.
            # Save the current count and on the next increase we'll know we had a successful proposal.
            self.last_proposal_count = proposal_count

        if proposal_count > self.last_proposal_count:
            # Set the last proposal count to None so that next time we check we know we have to
            # again first get the current value.
            self.last_proposal_count = None
            return True

        return False


def main():
    args_builder = ApolloArgsParserBuilder(
        "Wait for each Core to successfully propose a block",
        "python wait_for_cores_to_succesfully_propose.py -n apollo-sepolia-integration -m 3 -t all_at_once",
        include_restart_strategy=False,
    )
    args = args_builder.build()

    namespace_list = NamespaceAndInstructionArgs.get_namespace_list_from_args(args)
    context_list = NamespaceAndInstructionArgs.get_context_list_from_args(args)
    instructions = ["Checking node proposed successfully."] * len(namespace_list)

    namespace_and_instruction_args = NamespaceAndInstructionArgs(
        namespace_list,
        context_list,
        instructions,
    )

    if not wait_until_y_or_n(
        "Please update and or restart the first core as needed and press 'y' when ready to proceed."
    ):
        print_error("Operation cancelled by user")
        sys.exit(1)

    proposal_increase_checker = WaitForProposalIncrease()
    update_config_and_restart_nodes(
        None,
        namespace_and_instruction_args,
        Service.Core,
        WaitOnMetricRestarter(
            namespace_and_instruction_args,
            Service.Core,
            [
                MetricConditionGater.Metric(
                    "consensus_decisions_reached_as_proposer",
                    proposal_increase_checker.check_if_proposal_count_increased,
                )
            ],
            8082,
            RestartStrategy.NO_RESTART,
        ),
    )


if __name__ == "__main__":
    main()
