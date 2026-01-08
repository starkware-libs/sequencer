#!/bin/bash
#
# Installs the stwo_verify binary from the starkware-libs/proving-utils repo.
#
# This script:
# 1. Clones proving-utils to a build cache directory under target/third_party/
# 2. Checks out the pinned revision
# 3. Builds stwo_verify in release mode
# 4. Copies the binary to target/tools/
# 5. Prints instructions for adding to PATH or configuring STWO_VERIFY_PATH
#
# Usage:
#   ./scripts/install_stwo_verify.sh
#
# Environment Variables:
#   PROVING_UTILS_REV - Override the default pinned revision (see scripts/proving_utils_env.sh)
#   SKIP_BUILD_IF_EXISTS - If set to "1", skip building if binary already exists
#
# The binary will be installed to: <repo_root>/target/tools/stwo_verify

# If any command fails, exit immediately.
set -euo pipefail

# Shared proving-utils configuration.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=proving_utils_env.sh
source "${SCRIPT_DIR}/proving_utils_env.sh"

# Configuration.
PACKAGE_NAME="stwo_verify"
BINARY_NAME="stwo_verify"

# Build and install directories.
TOOLS_DIR="${REPO_ROOT}/target/tools"
BINARY_PATH="${TOOLS_DIR}/${BINARY_NAME}"

# Colors for output.
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

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

# Check for required tools.
check_requirements() {
    local missing=()

    if ! command -v git &> /dev/null; then
        missing+=("git")
    fi

    if ! command -v cargo &> /dev/null; then
        missing+=("cargo (Rust toolchain)")
    fi

    if ! command -v rustup &> /dev/null; then
        missing+=("rustup")
    fi

    if [ ${#missing[@]} -ne 0 ]; then
        error "Missing required tools: ${missing[*]}"
        error "Please install them and try again."
        exit 1
    fi
}

# Clone or update the proving-utils repository (only if needed).
clone_or_update_repo() {
    mkdir -p "${THIRD_PARTY_DIR}"

    if [ -d "${BUILD_DIR}/.git" ]; then
        info "Proving-utils already cloned at ${BUILD_DIR}"
        cd "${BUILD_DIR}"

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
        info "Cloning proving-utils to ${BUILD_DIR}..."
        rm -rf "${BUILD_DIR}"
        git clone "${PROVING_UTILS_REPO}" "${BUILD_DIR}"
        cd "${BUILD_DIR}"
        git checkout "${PROVING_UTILS_REV}"
    fi
}

# Build the binary
build_binary() {
    cd "${BUILD_DIR}"

    # The proving-utils repo has its own rust-toolchain.toml, which rustup will use automatically.
    # Log the toolchain being used.
    local toolchain
    toolchain=$(cat rust-toolchain.toml 2>/dev/null | grep 'channel' | sed 's/.*= *"\(.*\)"/\1/' || echo "default")
    info "Building with toolchain: ${toolchain}"
    info "This may take several minutes on first build..."

    cargo build --release -p "${PACKAGE_NAME}" --bin "${BINARY_NAME}"

    if [ ! -f "target/release/${BINARY_NAME}" ]; then
        error "Build succeeded but binary not found at target/release/${BINARY_NAME}"
        exit 1
    fi

    success "Build completed successfully"
}

# Install the binary.
install_binary() {
    mkdir -p "${TOOLS_DIR}"

    info "Installing ${BINARY_NAME} to ${BINARY_PATH}..."
    cp "${BUILD_DIR}/target/release/${BINARY_NAME}" "${BINARY_PATH}"
    chmod +x "${BINARY_PATH}"

    success "Binary installed to ${BINARY_PATH}"
}

# Print usage instructions.
print_instructions() {
    echo ""
    echo "=============================================="
    success "${BINARY_NAME} has been installed successfully!"
    echo "=============================================="
    echo ""
    echo "Binary location: ${BINARY_PATH}"
    echo ""
    echo "The starknet_api crate will automatically find this binary."
    echo "No PATH modification is required for development."
    echo ""
    echo "Optional: To use from command line directly, add to PATH:"
    echo "   export PATH=\"${TOOLS_DIR}:\$PATH\""
    echo ""
    echo "Verify installation:"
    echo "   ${BINARY_PATH} --help"
    echo ""
}

# Check if binary already exists and skip if requested.
check_existing() {
    if [ -f "${BINARY_PATH}" ]; then
        if [ "${SKIP_BUILD_IF_EXISTS:-0}" = "1" ]; then
            info "Binary already exists at ${BINARY_PATH}, skipping build (SKIP_BUILD_IF_EXISTS=1)"
            print_instructions
            exit 0
        fi

        warn "Binary already exists at ${BINARY_PATH}, will rebuild"
    fi
}

main() {
    echo ""
    info "Installing ${BINARY_NAME} from proving-utils @ ${PROVING_UTILS_REV}"
    echo ""

    check_requirements
    check_existing
    clone_or_update_repo
    build_binary
    install_binary
    print_instructions
}

main "$@"
