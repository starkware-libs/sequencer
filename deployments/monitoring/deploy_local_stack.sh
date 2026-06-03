#!/bin/bash

DOCKER_BUILDKIT=1
COMPOSE_DOCKER_CLI_BUILD=1

export DOCKER_BUILDKIT
export COMPOSE_DOCKER_CLI_BUILD
export MONITORING_ENABLED=${MONITORING_ENABLED:-true}
export FOLLOW_LOGS=${FOLLOW_LOGS:-false}
export SEQUENCER_HTTP_PORT=${SEQUENCER_HTTP_PORT:-8081}
export SEQUENCER_MONITORING_PORT=${SEQUENCER_MONITORING_PORT:-8082}
export SEQUENCER_CONFIG_PATH=${SEQUENCER_CONFIG_PATH:-/config/node_0/config.json}
export SEQUENCER_ROOT_DIR=${SEQUENCER_ROOT_DIR:-$(git -C "$(dirname -- "$(readlink -f -- "${BASH_SOURCE[0]}")")" rev-parse --show-toplevel)}

# Exchange-rate oracle source. When PRAGMA_API_KEY is set, point the node at the real Pragma
# oracle so eth_to_strk (L1 gas price conversion) and strk_to_usd (SNIP-35 fee_target) track live
# rates; otherwise fall back to the dummy constant-rate oracle (see docker-compose.yml
# config_injector). This affects the local stack only; the integration-test harness stays on the
# dummy so CI remains hermetic.
export PRAGMA_ETH_STRK_URL=${PRAGMA_ETH_STRK_URL:-https://api.production.pragma.build/node/v1/data/eth/strk}
export PRAGMA_STRK_USD_URL=${PRAGMA_STRK_USD_URL:-https://api.production.pragma.build/node/v1/data/strk/usd}
if [ -n "${PRAGMA_API_KEY:-}" ]; then
  export ETH_STRK_ORACLE_URL_HEADERS="${PRAGMA_ETH_STRK_URL},x-api-key^${PRAGMA_API_KEY}"
  export STRK_USD_ORACLE_URL_HEADERS="${PRAGMA_STRK_USD_URL},x-api-key^${PRAGMA_API_KEY}"
  echo "Exchange-rate oracle: Pragma (eth/strk + strk/usd)"
else
  echo "Exchange-rate oracle: dummy (set PRAGMA_API_KEY to use the real Pragma oracle)"
fi

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
  services="sequencer_node_setup dummy_recorder dummy_exchange_rate_oracle config_injector sequencer_node sequencer_simulator"
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
    --namespace local \
    --cluster local \
    --alert-rules-overrides-config-file "${monitoring_dir}"/examples/config/alert_overrides_testnet.yaml
fi

if [ "$FOLLOW_LOGS" == true ]; then
  ${docker_compose} -f "${monitoring_dir}"/local/docker-compose.yml logs -f
fi
