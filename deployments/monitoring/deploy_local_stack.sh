#!/bin/bash

export monitoring_dir=$(dirname -- "$(readlink -f -- "$BASH_SOURCE")")

# Deploy the monitoring stack locally
docker compose -f ${monitoring_dir}/local/docker-compose.yml "$@"; ret=$?
if [ $ret -ne 0 ]; then
    echo "Failed to deploy the monitoring stack locally"
    exit 1
fi

if [ "$1" == "down" ]; then
    exit $ret
fi

pip install -r ${monitoring_dir}/src/requirements.txt
python ${monitoring_dir}/src/dashboard_builder.py builder -j ${monitoring_dir}/../../Monitoring/sequencer/dev_grafana.json -o /tmp/dashboard_builder -d -u
