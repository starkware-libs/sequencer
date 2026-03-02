#!/bin/bash
#
# Shared configuration and helpers for proving-utils tool installers.
#
# Environment Variables:
#   PROVING_UTILS_REV - Override the default pinned revision.
#
# This file sets:
#   PROVING_UTILS_REPO
#   PROVING_UTILS_REV
#   REPO_ROOT
#   BUILD_DIR
#
# Functions:
#   info, success, warn, error  - Colored log helpers.
#   clone_or_update_proving_utils <dir> - Clone/update proving-utils to <dir>.

# If any command fails, exit immediately.
set -euo pipefail

PROVING_UTILS_REPO="https://github.com/starkware-libs/proving-utils"
PROVING_UTILS_REV_DEFAULT="e16f9d0"
PROVING_UTILS_REV="${PROVING_UTILS_REV:-${PROVING_UTILS_REV_DEFAULT}}"

COMMON_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${COMMON_DIR}/.." && pwd)"

BUILD_DIR="${REPO_ROOT}/build/proving-utils"

# Colors for output.
BLUE='\033[0;34m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

# Clone or update the proving-utils repository to the given directory.
# Usage: clone_or_update_proving_utils <target_dir>
clone_or_update_proving_utils() {
    local target_dir="$1"

    if [ -d "${target_dir}/.git" ]; then
        info "Proving-utils already cloned at ${target_dir}"
        cd "${target_dir}"

        # Check if we're at the right revision.
        local current_rev
        current_rev=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")

        if [[ "${current_rev}" == "${PROVING_UTILS_REV}"* ]]; then
            info "Already at revision ${PROVING_UTILS_REV}"
            return 0
        fi

        info "Fetching and checking out revision ${PROVING_UTILS_REV}..."
        git fetch origin
        git checkout "${PROVING_UTILS_REV}"
    else
        info "Cloning proving-utils to ${target_dir}..."
        rm -rf "${target_dir}"
        mkdir -p "$(dirname "${target_dir}")"
        git clone "${PROVING_UTILS_REPO}" "${target_dir}"
        cd "${target_dir}"
        git checkout "${PROVING_UTILS_REV}"
    fi
}
