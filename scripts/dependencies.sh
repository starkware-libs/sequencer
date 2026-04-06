#!/bin/bash

set -e

[[ ${UID} == "0" ]] || SUDO="sudo"

# Source common apt utilities
# Handle both cases: script in subdirectory or current directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
if [ -f "${SCRIPT_DIR}/apt_utils.sh" ]; then
    source "${SCRIPT_DIR}/apt_utils.sh"
elif [ -f "./apt_utils.sh" ]; then
    source "./apt_utils.sh"
else
    echo "Error: apt_utils.sh not found in ${SCRIPT_DIR} or current directory" >&2
    exit 1
fi

function install_essential_deps_linux() {
    log_step "dependencies" "Installing essential Linux dependencies..."
    $SUDO bash -c "$(declare -f apt_update_with_retry); $(declare -f apt_install_with_retry)"'
        apt_update_with_retry && apt_install_with_retry -y \
            ca-certificates \
            curl \
            git \
            gnupg \
            jq \
            lsb-release \
            protobuf-compiler \
            ripgrep \
            software-properties-common \
            zstd \
            wget
  '
    log_step "dependencies" "Essential Linux dependencies installed successfully"
}

function main() {
    log_step "dependencies" "Starting dependencies installation..."
    [ "$(uname)" = "Linux" ] && install_essential_deps_linux
    "${SCRIPT_DIR}/install_llvm19.sh"
    log_step "dependencies" "All dependencies installed successfully!"
}

main "$@"
