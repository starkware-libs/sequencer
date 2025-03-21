#!/bin/bash

DOCKER_BUILDKIT=1
COMPOSE_DOCKER_CLI_BUILD=1

export DOCKER_BUILDKIT
export COMPOSE_DOCKER_CLI_BUILD
export MONITORING_ENABLED=${MONITORING_ENABLED:-true}
export FOLLOW_LOGS=${FOLLOW_LOGS:-true}

monitoring_dir=$(dirname -- "$(readlink -f -- "${BASH_SOURCE[0]}")")
export monitoring_dir

if command -v docker compose &> /dev/null; then
  docker_compose="docker compose"
elif command -v docker-compose &> /dev/null; then
  docker_compose="docker-compose"
else
  echo "Error: docker compose is missing, please install and try again. Official docs: https://docs.docker.com/compose/install/linux/"
  exit 1
fi

if [ "$MONITORING_ENABLED" != true ]; then
  services="sequencer_node_setup dummy_recorder config_injector sequencer_node sequencer_simulator"
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
  python "${monitoring_dir}"/src/dashboard_builder.py builder -j "${monitoring_dir}"/../../Monitoring/sequencer/dev_grafana.json -o /tmp/dashboard_builder -d -u
  python "${monitoring_dir}"/src/alert_builder.py -j "${monitoring_dir}"/examples/dev_grafana_alerts.json -o /tmp/alert_builder
fi

if [ "$FOLLOW_LOGS" == true ]; then
  ${docker_compose} -f "${monitoring_dir}"/local/docker-compose.yml logs -f
fi
