#!/bin/bash
#
# Builds the starknet_os_runner Docker image.
#
# Usage:
#   ./scripts/build_starknet_os_runner.sh [OPTIONS]
#
# Options:
#   --image-tag TAG               Docker image tag (default: os_runner:latest)
#   --build-mode MODE             Build mode: release or debug (default: release)
#   --docker-build-args ARGS      Additional arguments to pass to docker build
#   -h, --help                    Show this help message
#
# Environment Variables:
#   DOCKER_BUILDKIT               Set to 1 to enable Docker BuildKit (recommended)
#
# Examples:
#   # Build with default settings
#   ./scripts/build_starknet_os_runner.sh
#
#   # Build debug mode
#   ./scripts/build_starknet_os_runner.sh --build-mode debug

# If any command fails, exit immediately.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Colors for output.
BLUE='\033[0;34m'
GREEN='\033[0;32m'
RED='\033[0;31m'
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

# Default values.
DOCKERFILE_PATH="${REPO_ROOT}/crates/starknet_os_runner/Dockerfile"
IMAGE_TAG="us-central1-docker.pkg.dev/starkware-dev/sequencer/os-runner:latest"
BUILD_MODE="release"
DOCKER_BUILD_ARGS=""

# Parse command-line arguments.
while [[ $# -gt 0 ]]; do
    case $1 in
        --image-tag)
            if [[ -z "${2:-}" ]]; then
                error "Error: --image-tag requires a tag argument"
                exit 1
            fi
            IMAGE_TAG="$2"
            shift 2
            ;;
        --build-mode)
            if [[ -z "${2:-}" ]]; then
                error "Error: --build-mode requires a mode argument (release or debug)"
                exit 1
            fi
            if [[ "$2" != "release" && "$2" != "debug" ]]; then
                error "Error: --build-mode must be either 'release' or 'debug' (got '$2')"
                exit 1
            fi
            BUILD_MODE="$2"
            shift 2
            ;;
        --docker-build-args)
            if [[ -z "${2:-}" ]]; then
                error "Error: --docker-build-args requires arguments"
                exit 1
            fi
            DOCKER_BUILD_ARGS="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Builds the starknet_os_runner Docker image."
            echo ""
            echo "Options:"
            echo "  --image-tag TAG               Docker image tag (default: os_runner:latest)"
            echo "  --build-mode MODE             Build mode: release or debug (default: release)"
            echo "  --docker-build-args ARGS      Additional arguments to pass to docker build"
            echo "  -h, --help                    Show this help message"
            echo ""
            echo "Environment Variables:"
            echo "  DOCKER_BUILDKIT               Set to 1 to enable Docker BuildKit (recommended)"
            echo ""
            echo "Examples:"
            echo "  # Build with default settings"
            echo "  $0"
            echo ""
            echo "  # Build debug mode"
            echo "  $0 --build-mode debug"
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            error "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Build the Docker image.
build_docker_image() {
    info "Building Docker image: ${IMAGE_TAG}"
    info "Build mode: ${BUILD_MODE}"

    # Check if Dockerfile exists.
    if [[ ! -f "${DOCKERFILE_PATH}" ]]; then
        error "Dockerfile not found at ${DOCKERFILE_PATH}"
        exit 1
    fi

    # Build docker command.
    local docker_cmd=(
        docker build
        -f "${DOCKERFILE_PATH}"
        --build-arg "BUILD_MODE=${BUILD_MODE}"
        -t "${IMAGE_TAG}"
    )

    # Add additional docker build args if provided.
    if [[ -n "${DOCKER_BUILD_ARGS}" ]]; then
        # Split the args string into an array.
        read -ra extra_args <<< "${DOCKER_BUILD_ARGS}"
        docker_cmd+=("${extra_args[@]}")
    fi

    # Add build context (repository root).
    docker_cmd+=("${REPO_ROOT}")

    info "Running: ${docker_cmd[*]}"
    echo ""

    # Run docker build.
    if "${docker_cmd[@]}"; then
        echo ""
        success "Docker image built successfully: ${IMAGE_TAG}"
        echo ""
        echo "You can now run the container:"
        echo "  docker run --rm -p 3000:3000 ${IMAGE_TAG}"
        echo ""
    else
        error "Docker build failed"
        exit 1
    fi
}

main() {
    echo ""
    info "Building starknet_os_runner Docker image"
    echo ""

    build_docker_image
}

main "$@"
