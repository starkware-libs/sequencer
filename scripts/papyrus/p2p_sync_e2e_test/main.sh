#!/bin/bash

MONITORING_METRICS_URL="http://localhost:8082/monitoring/metrics"
SLEEP_DURATION_SECONDS=30

RED='\033[1;31m'
GREEN='\033[0;32m'
GRAY='\033[0;37m'
NO_COLOR='\033[0m'

run_nodes_and_process_output() {
    local client_command=$1
    local server_command=$2

    eval "$client_command" &
    client_pid=$!

    eval "$server_command" &
    server_pid=$!

    echo "Client PID: $client_pid"
    echo "Server PID: $server_pid"

    sleep $SLEEP_DURATION_SECONDS
    validate_marker papyrus_state_marker
    cleanup
    echo -e "${GREEN}Test passed successfully.$NO_COLOR"
}

# Extract the value of the given marker from the monitoring gateway and check it's big enough
validate_marker() {
    local marker_name=$1

    # Run curl and check the state marker
    curl_output=$(curl -s -X GET "$MONITORING_METRICS_URL")

    # Extract the numeric value after marker_name
    marker_value=$(echo "$curl_output" | grep -oP "$marker_name"' \K\d+')

    if [[ -z "$marker_value" ]]; then
        cleanup
        echo -e "${RED}Failed to extract $marker_name value from monitoring output. Test failed.$NO_COLOR"
        exit 1
    fi

    if (( marker_value < 10 )); then
        cleanup
        echo -e "${RED}$marker_name is $marker_value which is less than 10. Test failed.$NO_COLOR"
        exit 1
    fi
    echo -e "${GREEN}$marker_name is $marker_value which is valid.$NO_COLOR"
}



cleanup() {
    echo -e "${GRAY}######## Cleaning up... You'll might see an ERROR on the 2nd process that is killed because the connection was closed and when killing it that there's no such process. This is ok. ########$NO_COLOR"
    pgrep -P $$
    pkill -P $client_pid
    pkill -P $server_pid
    kill -KILL "$client_pid"
    kill -KILL "$server_pid"
}

main() {
    if [[ $# -ne 1 ]]; then
        echo "Usage: $0 <BASE_LAYER_NODE_URL>"
        exit 1
    fi

    base_layer_node_url=$1

    rm -rf scripts/papyrus/p2p_sync_e2e_test/data_client scripts/papyrus/p2p_sync_e2e_test/data_server
    mkdir scripts/papyrus/p2p_sync_e2e_test/data_client scripts/papyrus/p2p_sync_e2e_test/data_server

    client_node_command="target/release/papyrus_node --base_layer_url $base_layer_node_url --config_file scripts/papyrus/p2p_sync_e2e_test/client_node_config.json"
    server_node_command="target/release/papyrus_node --base_layer_url $base_layer_node_url --config_file scripts/papyrus/p2p_sync_e2e_test/server_node_config.json"

    run_nodes_and_process_output "$client_node_command" "$server_node_command"
}

main "$@"
