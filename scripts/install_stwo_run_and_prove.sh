#!/bin/bash
#
# Installs the stwo_run_and_prove binary from the starkware-libs/proving-utils repo.
#
# This script:
# 1. Clones proving-utils to a build cache directory under target/third_party/.
# 2. Checks out the pinned revision.
# 3. Builds stwo_run_and_prove in release mode.
# 4. Copies the binary to target/tools/.
# 5. Prints instructions for adding to PATH or configuring STWO_RUN_AND_PROVE_PATH.
#
# Usage:
#   ./scripts/install_stwo_run_and_prove.sh
#
# Environment Variables:
#   PROVING_UTILS_REV - Override the default pinned revision (see scripts/proving_utils_env.sh).
#   SKIP_BUILD_IF_EXISTS - If set to "1", skip building if binary already exists.
#
# The binary will be installed to: <repo_root>/target/tools/stwo_run_and_prove

# If any command fails, exit immediately.
set -euo pipefail

# Shared proving-utils configuration.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=proving_utils_env.sh
source "${SCRIPT_DIR}/proving_utils_env.sh"

# Configuration.
PACKAGE_NAME="stwo-run-and-prove"
# The compiled binary name uses hyphens (matching the package name).
COMPILED_BINARY_NAME="stwo-run-and-prove"
# The installed binary name uses underscores (matching codebase expectations).
BINARY_NAME="stwo_run_and_prove"

# Build and install directories.
TOOLS_DIR="${REPO_ROOT}/target/tools"
BINARY_PATH="${TOOLS_DIR}/${BINARY_NAME}"

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

# Build the binary.
build_binary() {
    cd "${BUILD_DIR}"

    # The proving-utils repo has its own rust-toolchain.toml, which rustup will use automatically.
    # Log the toolchain being used.
    local toolchain
    toolchain=$(cat rust-toolchain.toml 2>/dev/null | grep 'channel' | sed 's/.*= *"\(.*\)"/\1/' || echo "default")
    info "Building with toolchain: ${toolchain}"
    info "This may take several minutes on first build..."

    cargo build --release -p "${PACKAGE_NAME}" --bin "${COMPILED_BINARY_NAME}"

    if [ ! -f "target/release/${COMPILED_BINARY_NAME}" ]; then
        error "Build succeeded but binary not found at target/release/${COMPILED_BINARY_NAME}"
        exit 1
    fi

    success "Build completed successfully"
}

# Install the binary.
install_binary() {
    mkdir -p "${TOOLS_DIR}"

    info "Installing ${BINARY_NAME} to ${BINARY_PATH}..."
    cp "${BUILD_DIR}/target/release/${COMPILED_BINARY_NAME}" "${BINARY_PATH}"
    chmod +x "${BINARY_PATH}"

    success "Binary installed to ${BINARY_PATH}"
}

# Sync the bootloader from proving-utils into starknet_os_runner resources.
sync_bootloader() {
    local src="${BUILD_DIR}/crates/cairo-program-runner-lib/resources/compiled_programs/bootloaders/simple_bootloader_compiled.json"
    local dst="${REPO_ROOT}/crates/starknet_os_runner/resources/simple_bootloader_compiled.json"

    if [ ! -f "${src}" ]; then
        warn "Bootloader not found at ${src}, skipping sync"
        return 0
    fi

    info "Syncing bootloader to ${dst}..."
    cp "${src}" "${dst}"
    success "Bootloader synced"
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
    echo "The proving_utils crate will automatically find this binary."
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
    clone_or_update_proving_utils "${BUILD_DIR}"
    build_binary
    install_binary
    sync_bootloader
    print_instructions
}

main "$@"
