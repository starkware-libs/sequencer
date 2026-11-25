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

# Exact versions of the tools CI builds with, as shipped by Ubuntu 24.04 (noble).
# Ubuntu keeps only the newest update in its archive, so when it publishes a new one the old
# version can no longer be installed: bump these then (`apt-cache policy <package>` shows it).
PROTOBUF_COMPILER_VERSION="3.21.12-8.2ubuntu0.4"
RIPGREP_VERSION="14.1.0-1"

# Prints protobuf-compiler and ripgrep pinned to the versions above on Ubuntu 24.04. These
# versions exist only there, so other distributions get the version their release ships.
function pinned_tool_packages() {
    local codename
    codename=$(. /etc/os-release 2>/dev/null && echo "${VERSION_CODENAME:-}")
    if [ "$codename" = "noble" ]; then
        echo "protobuf-compiler=${PROTOBUF_COMPILER_VERSION} ripgrep=${RIPGREP_VERSION}"
    else
        echo "Warning: not Ubuntu 24.04; installing unpinned protobuf-compiler and ripgrep." >&2
        echo "protobuf-compiler ripgrep"
    fi
}

function install_essential_deps_linux() {
    log_step "dependencies" "Installing essential Linux dependencies..."
    local pinned_packages
    pinned_packages=$(pinned_tool_packages)
    # $pinned_packages is passed as $1 and left unquoted on purpose, to split into packages.
    $SUDO bash -c "$(declare -f apt_update_with_retry); $(declare -f apt_install_with_retry)"'
        apt_update_with_retry && apt_install_with_retry -y \
            ca-certificates \
            curl \
            git \
            gnupg \
            jq \
            lsb-release \
            $1 \
            software-properties-common \
            zstd \
            wget
  ' bash "$pinned_packages"
    log_step "dependencies" "Essential Linux dependencies installed successfully"
}

function main() {
    log_step "dependencies" "Starting dependencies installation..."
    [ "$(uname)" = "Linux" ] && install_essential_deps_linux
    "${SCRIPT_DIR}/install_llvm19.sh"
    log_step "dependencies" "All dependencies installed successfully!"
}

main "$@"
