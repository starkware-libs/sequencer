#!/bin/bash

# revert.sh - Script to revert blocks
# Usage: ./revert.sh <block to revert until (inclusive)> <update_config_and_restart_nodes.py args...]

# Check if at least one argument is provided
if [ $# -eq 0 ]; then
    echo "Error: At least one argument is required"
    echo "Usage: $0 <first_param> [other_args...]"
    exit 1
fi

# Get the first parameter (the revert up to and including value)
FIRST_PARAM="$1"

# Shift the first parameter off, leaving the rest of the arguments
shift

# Call the Python script with all original arguments plus the revert config
python3 "$(dirname "$0")/update_config_and_restart_nodes.py" "$@" \
    -o "revert_config.revert_up_to_and_including=$FIRST_PARAM" \
    -o "revert_config.should_revert=true" 