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

ESSENTIAL_PACKAGES=(
    ca-certificates
    curl
    git
    gnupg
    jq
    lsb-release
    protobuf-compiler
    ripgrep
    software-properties-common
    zstd
    wget
)

# Succeeds only if every package is fully installed. Packages in the `config-files` state
# (removed, config left behind) do not count, which is why `dpkg -s` is not used.
function packages_installed() {
    command -v dpkg-query &>/dev/null || return 1
    local package
    for package in "$@"; do
        [[ "$(dpkg-query -W -f='${db:Status-Status}' "${package}" 2>/dev/null)" == "installed" ]] \
            || return 1
    done
}

function install_essential_deps_linux() {
    if packages_installed "${ESSENTIAL_PACKAGES[@]}"; then
        log_step "dependencies" "Essential Linux dependencies already installed, skipping"
        return 0
    fi
    log_step "dependencies" "Installing essential Linux dependencies..."
    $SUDO bash -c "$(declare -f apt_update_with_retry); $(declare -f apt_install_with_retry)"'
        apt_update_with_retry && apt_install_with_retry -y "$@"
  ' _ "${ESSENTIAL_PACKAGES[@]}"
    log_step "dependencies" "Essential Linux dependencies installed successfully"
}

function main() {
    log_step "dependencies" "Starting dependencies installation..."
    [ "$(uname)" = "Linux" ] && install_essential_deps_linux
    "${SCRIPT_DIR}/install_llvm19.sh"
    log_step "dependencies" "All dependencies installed successfully!"
}

main "$@"
