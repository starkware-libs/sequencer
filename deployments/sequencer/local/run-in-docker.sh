#!/usr/bin/env bash
set -euo pipefail

# Wrapper script to build and run the CDK8S deployment Docker container
# This script builds the Docker image and runs it with the same user as the host

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Dockerfile is in the same directory as this script
DOCKERFILE_DIR="$SCRIPT_DIR"
IMAGE_NAME="cdk8s-sequencer"
IMAGE_TAG="latest"
FULL_IMAGE_NAME="${IMAGE_NAME}:${IMAGE_TAG}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

# Check if Docker is available
if ! command -v docker &> /dev/null; then
    error "Docker is not installed or not in PATH"
    exit 1
fi

# Get current user's UID and GID
USER_ID=$(id -u)
GROUP_ID=$(id -g)
USER_NAME=$(id -un)
GROUP_NAME=$(id -gn)

info "Building Docker image: ${FULL_IMAGE_NAME}"
# Build from the sequencer directory (parent of local/) so COPY paths work correctly
SEQUENCER_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
if ! docker build -t "${FULL_IMAGE_NAME}" -f "${DOCKERFILE_DIR}/Dockerfile" "${SEQUENCER_DIR}"; then
    error "Failed to build Docker image"
    exit 1
fi
success "Docker image built successfully"

# Get repo root (3 levels up from local/run-docker.sh)
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"

KUBE_CONFIG_DIR="${HOME}/.kube"
info "Starting Docker container..."
info "  User: ${USER_NAME} (UID: ${USER_ID}, GID: ${GROUP_ID})"
info "  Mounting: ${REPO_ROOT} -> /workspace"
info "  Mounting: ${KUBE_CONFIG_DIR} -> /home/${USER_NAME}/.kube-tmp (read-only; entrypoint copies to .kube for write access)"
info "  Working directory: /workspace/deployments/sequencer"

# Run the container
# Pass user info to entrypoint (no HOME mount)
# Mount repo root to /workspace; mount host ~/.kube to .kube-tmp (ro), entrypoint copies to .kube so kubectl/kubectx can write
docker run -it --rm \
    -v "${REPO_ROOT}:/workspace" \
    -v "${KUBE_CONFIG_DIR}:/home/${USER_NAME}/.kube-tmp:ro" \
    -w /workspace/deployments/sequencer \
    -e "USER_ID=${USER_ID}" \
    -e "GROUP_ID=${GROUP_ID}" \
    -e "USER_NAME=${USER_NAME}" \
    "${FULL_IMAGE_NAME}" \
    /bin/bash -l
