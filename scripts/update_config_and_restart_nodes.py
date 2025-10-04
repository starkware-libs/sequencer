#!/usr/bin/env python3


import sys

from update_config_and_restart_nodes_lib import (
    build_args_parser,
    parse_config_overrides,
    print_colored,
    print_error,
    update_config_and_restart_nodes,
    validate_arguments,
)


def main():
    parser = build_args_parser()
    args = parser.parse_args()
    validate_arguments(args)

    config_overrides = parse_config_overrides(args.config_overrides)
    if config_overrides:
        print_colored(f"\nConfig overrides to apply:")
        for key, value in config_overrides.items():
            print_colored(f"  {key} = {value}")
    else:
        print_error("No config overrides provided")
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
