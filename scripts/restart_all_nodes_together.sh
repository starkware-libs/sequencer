#!/bin/bash
set -e

usage() {
    echo "Usage: $0 -f <FEEDER_URL> -n <NAMESPACE_PREFIX> -N <NUM_NODES> [-s <START_INDEX>] [-c <CLUSTER_PREFIX>] [-h]"
    echo ""
    echo "Options:"
    echo "  -f, --feeder-url      The feeder gateway URL (e.g., feeder.integration-sepolia.starknet.io)"
    echo "  -n, --namespace       The Kubernetes namespace prefix (e.g., apollo-sepolia-integration)"
    echo "  -N, --num-nodes       The number of nodes to restart (required)"
    echo "  -s, --start-index     The starting index for the node loop (default: 0)"
    echo "  -c, --cluster         Optional cluster prefix for kubectl context"
    echo "  -h, --help            Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 -f feeder.integration-sepolia.starknet.io -n apollo-sepolia-integration -N 3"
    echo "  $0 -f feeder.integration-sepolia.starknet.io -n apollo-sepolia-integration -N 3 -s 2"
    echo "  $0 --feeder-url feeder.integration-sepolia.starknet.io --namespace apollo-sepolia-integration --num-nodes 3 --start-index 1 --cluster my-cluster"
    exit 1
}

# Initialize variables
FEEDER_URL=""
NAMESPACE_PREFIX=""
NUM_NODES=""
START_INDEX="0"
CLUSTER_PREFIX=""

# Parse command line options using getopt
TEMP_ARGS=$(getopt -o f:n:N:s:c:h --long feeder-url:,namespace:,num-nodes:,start-index:,cluster:,help -n "$0" -- "$@")

if [ $? != 0 ]; then
    echo "Error: Failed to parse arguments" >&2
    usage
fi

# Normalize the arguments so that we can iterate over them.
eval set -- "$TEMP_ARGS"

while true; do
    case "$1" in
        -f|--feeder-url)
            FEEDER_URL="$2"
            shift 2
            ;;
        -n|--namespace)
            NAMESPACE_PREFIX="$2"
            shift 2
            ;;
        -N|--num-nodes)
            NUM_NODES="$2"
            shift 2
            ;;
        -s|--start-index)
            START_INDEX="$2"
            shift 2
            ;;
        -c|--cluster)
            CLUSTER_PREFIX="$2"
            shift 2
            ;;
        -h|--help)
            usage
            ;;
        --)
            shift
            break
            ;;
        *)
            echo "Internal error!"
            exit 1
            ;;
    esac
done

# Validate required arguments
if [ -z "$FEEDER_URL" ]; then
    echo "Error: FEEDER_URL is required. Use -f or --feeder-url flag."
    usage
fi

if [ -z "$NAMESPACE_PREFIX" ]; then
    echo "Error: NAMESPACE_PREFIX is required. Use -n or --namespace flag."
    usage
fi

if [ -z "$NUM_NODES" ]; then
    echo "Error: NUM_NODES is required. Use -N or --num-nodes flag."
    usage
fi

# Validate NUM_NODES is a positive integer
if ! [[ "$NUM_NODES" =~ ^[1-9][0-9]*$ ]]; then
    echo "Error: NUM_NODES must be a positive integer."
    usage
fi

# Validate START_INDEX is a non-negative integer
if ! [[ "$START_INDEX" =~ ^[0-9]+$ ]]; then
    echo "Error: START_INDEX must be a non-negative integer."
    usage
fi

# Inform about cluster context
if [ -z "$CLUSTER_PREFIX" ]; then
    echo -e "\033[1;31mCLUSTER_PREFIX not provided. Assuming all nodes are on the current cluster\033[0m"
fi

CURRENT_BLOCK_NUMBER=$(curl https://${FEEDER_URL}/feeder_gateway/get_block | jq .block_number)
NEXT_BLOCK_NUMBER=$((CURRENT_BLOCK_NUMBER + 1))
for i in $(seq $START_INDEX $((START_INDEX + NUM_NODES - 1))); do
    filename=config${i}.yaml
    command="kubectl get cm sequencer-core-config -n ${NAMESPACE_PREFIX}-${i} -o yaml"
    if [ -n "${CLUSTER_PREFIX}" ]; then
        command="${command} --context=${CLUSTER_PREFIX}-${i}"
    fi
    $(${command} > ${filename})
    cp ${filename} ${filename}_old
    for key in consensus_manager_config.immediate_active_height consensus_manager_config.cende_config.skip_write_height; do
        sed "s/\"${key}\": [0-9]\+/\"${key}\": ${NEXT_BLOCK_NUMBER}/" -i ${filename}
    done
    sed "s/\"validator_id\": \"0x[0-9]\+\"/\"validator_id\": \"0x$((${i} + 64))\"/" -i ${filename}
    echo -e "\033[1;33m--------------------- Config changes to node no. ${i}'s core service --------------------\033[0m"
    # If diffs are found diff wil return a non zero code which will cause the script to fail.
    # To prevent this we use || true.
    diff ${filename}_old ${filename} || true
done
read -p "$(echo -e "\033[1;34mDo you approve these changes? (y/n)\033[0m")" yes_or_no
if [[ "${yes_or_no}" != "y" ]]; then
    exit 1
fi
for i in $(seq $START_INDEX $((START_INDEX + NUM_NODES - 1))); do
    command="kubectl apply -f config${i}.yaml -n ${NAMESPACE_PREFIX}-${i}"
    if [ -n "${CLUSTER_PREFIX}" ]; then
        command="${command} --context=${CLUSTER_PREFIX}-${i}"
    fi
    bash -c "${command}" || { echo "Failed applying config for node ${i}"; exit 1; }
done
for i in $(seq $START_INDEX $((START_INDEX + NUM_NODES - 1))); do
    command="kubectl delete pod sequencer-core-statefulset-0 -n ${NAMESPACE_PREFIX}-${i}"
    if [ -n "${CLUSTER_PREFIX}" ]; then
        command="${command} --context=${CLUSTER_PREFIX}-${i}"
    fi
    bash -c "${command}" || { echo "Failed restarting core pod for node ${i}"; exit 1; }
done