#!/bin/bash

export monitoring_dir=$(dirname -- "$(readlink -f -- "$BASH_SOURCE")")

# Deploy the monitoring stack locally
docker-compose -f ${monitoring_dir}/local/docker-compose.yml "$@"; ret=$?
if [ $ret -ne 0 ]; then
    echo "Failed to deploy the monitoring stack locally"
    exit 1
fi

if [ "$1" == "down" ]; then
    exit $ret
fi
${monitoring_dir}/src/generate_dashboard.py --input-file ${monitoring_dir}/src/dummy_json.json --output-dir /tmp/ --upload
