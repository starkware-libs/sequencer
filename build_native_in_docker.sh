#!/bin/env bash
set -e

# Enables build-x for older versions of docker.
export DOCKER_BUILDKIT=1

docker_image_name=sequencer-ci
dockerfile_path="docker-ci/images/${docker_image_name}.Dockerfile"

docker build . --build-arg USER_UID=$UID -t ${docker_image_name} --file ${dockerfile_path}

# Wiz CLI image scan (if credentials are provided via GitHub secrets)
# Note: Failures in Wiz scanning will not stop the script execution
if [ -n "$WIZ_CLIENT_ID" ] && [ -n "$WIZ_CLIENT_SECRET" ]; then
    set +e  # Temporarily disable exit on error for Wiz scanning
    # Download Wiz CLI
    if [ ! -f wizcli ]; then
        curl -s -o wizcli https://downloads.wiz.io/wizcli/latest/wizcli-linux-amd64 > /dev/null 2>&1 || true
        chmod +x wizcli || true
    fi
    
    # Authenticate to Wiz
    ./wizcli auth --id "$WIZ_CLIENT_ID" --secret "$WIZ_CLIENT_SECRET" > /dev/null 2>&1 || true
    
    # Run wiz-cli docker image scan
    ./wizcli docker scan --image ${docker_image_name} --policy "Default vulnerabilities policy" > /dev/null 2>&1 || true
    
    # Fetch digest of Docker image for Graph enrichment
    ./wizcli docker tag --image ${docker_image_name} > /dev/null 2>&1 || true
    set -e  # Re-enable exit on error
fi

docker run \
    --rm \
    --net host \
    -e CARGO_HOME="${HOME}/.cargo" \
    -u $UID \
    -v /tmp:/tmp \
    -v "${HOME}:${HOME}" \
    --workdir "${PWD}" \
    ${docker_image_name} \
    "$@"


