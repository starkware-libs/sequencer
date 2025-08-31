#!/usr/bin/env python3

import sys
import os

from update_config_and_restart_nodes_lib import (
    build_args_parser,
    validate_arguments,
    parse_config_overrides,
    update_config_and_restart_nodes,
)


def main():
    parser = build_args_parser()
    args = parser.parse_args()
    validate_arguments(args)

    config_overrides = parse_config_overrides(args.config_overrides)
    if config_overrides:
        print(f"\nConfig overrides to apply:")
        for key, value in config_overrides.items():
            print(f"  {key} = {value}")
    else:
        print("No config overrides provided", file=sys.stderr)
        sys.exit(1)

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
