#!/bin/bash
set -e

usage() {
    echo "Usage: $0 -f <FEEDER_URL> -n <NAMESPACE_PREFIX> [-c <CLUSTER_PREFIX>] [-h]"
    echo ""
    echo "Options:"
    echo "  -f, --feeder-url      The feeder gateway URL (e.g., feeder.integration-sepolia.starknet.io)"
    echo "  -n, --namespace       The Kubernetes namespace prefix (e.g., apollo-sepolia-integration)"
    echo "  -c, --cluster         Optional cluster prefix for kubectl context"
    echo "  -h, --help            Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 -f feeder.integration-sepolia.starknet.io -n apollo-sepolia-integration"
    echo "  $0 --feeder-url feeder.integration-sepolia.starknet.io --namespace apollo-sepolia-integration --cluster my-cluster"
    exit 1
}

# Initialize variables
FEEDER_URL=""
NAMESPACE_PREFIX=""
CLUSTER_PREFIX=""

# Parse command line options using getopt
TEMP_ARGS=$(getopt -o f:n:c:h --long feeder-url:,namespace:,cluster:,help -n "$0" -- "$@")

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

# Inform about cluster context
if [ -z "$CLUSTER_PREFIX" ]; then
    echo -e "\033[1;31mCLUSTER_PREFIX not provided. Assuming all nodes are on the current cluster\033[0m"
fi

CURRENT_BLOCK_NUMBER=$(curl https://${FEEDER_URL}/feeder_gateway/get_block | jq .block_number)
NEXT_BLOCK_NUMBER=$((CURRENT_BLOCK_NUMBER + 1))
for i in {0..2}; do
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
    diff ${filename}_old ${filename}
done
read -p "$(echo -e "\033[1;34mDo you approve these changes? (y/n)\033[0m")" yes_or_no
if [[ "${yes_or_no}" != "y" ]]; then
    exit 1
fi
for i in {0..2}; do
    command="kubectl apply -f config${i}.yaml -n ${NAMESPACE_PREFIX}-${i}"
    if [ -n "${CLUSTER_PREFIX}" ]; then
        command="${command} --context=${CLUSTER_PREFIX}-${i}"
    fi
    bash -c "${command}" || { echo "Failed applying config for node ${i}"; exit 1; }
done
for i in {0..2}; do
    command="kubectl delete pod sequencer-core-statefulset-0 -n ${NAMESPACE_PREFIX}-${i}"
    if [ -n "${CLUSTER_PREFIX}" ]; then
        command="${command} --context=${CLUSTER_PREFIX}-${i}"
    fi
    bash -c "${command}" || { echo "Failed restarting core pod for node ${i}"; exit 1; }
done