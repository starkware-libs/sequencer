#!/bin/bash

DOCKER_BUILDKIT=1
COMPOSE_DOCKER_CLI_BUILD=1

export DOCKER_BUILDKIT
export COMPOSE_DOCKER_CLI_BUILD
export MONITORING_ENABLED=${MONITORING_ENABLED:-true}
export FOLLOW_LOGS=${FOLLOW_LOGS:-false}

monitoring_dir=$(dirname -- "$(readlink -f -- "${BASH_SOURCE[0]}")")
export monitoring_dir

# Set SEQUENCER_ROOT_DIR to the project root (two levels up from monitoring dir)
export SEQUENCER_ROOT_DIR=$(dirname $(dirname "$monitoring_dir"))

if command -v docker compose &> /dev/null; then
  docker_compose="docker compose"
elif command -v docker-compose &> /dev/null; then
  docker_compose="docker-compose"
else
  echo "Error: docker compose is missing, please install and try again. Official docs: https://docs.docker.com/compose/install/linux/"
  exit 1
fi

if [ "$MONITORING_ENABLED" != true ]; then
  services="sequencer_node_setup dummy_recorder dummy_eth_to_strk_oracle config_injector sequencer_node sequencer_simulator"
fi

echo "Running: ${docker_compose} -f ${monitoring_dir}/local/docker-compose.yml $*"
${docker_compose} -f "${monitoring_dir}"/local/docker-compose.yml "$@" ${services}
ret=$?

if [ "$ret" -ne 0 ]; then
    echo "Failed to deploy the monitoring stack locally"
    exit 1
fi

if [ "$1" == "down" ]; then
    exit "$ret"
fi

if [ "$MONITORING_ENABLED" == true ]; then
  pip install -r "${monitoring_dir}"/src/requirements.txt
  python "${monitoring_dir}"/src/main.py \
    --dev-dashboards-file "${monitoring_dir}"/../../crates/apollo_dashboard/resources/dev_grafana.json \
    --dev-alerts-file "${monitoring_dir}"/../../crates/apollo_dashboard/resources/dev_grafana_alerts.json \
    --out-dir /tmp/grafana_builder \
    --env dev
fi

if [ "$FOLLOW_LOGS" == true ]; then
  ${docker_compose} -f "${monitoring_dir}"/local/docker-compose.yml logs -f
fi
