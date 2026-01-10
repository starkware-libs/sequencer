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
# shellcheck source=proving_utils_install_common.sh
source "${SCRIPT_DIR}/proving_utils_install_common.sh"

# Configuration.
PACKAGE_NAME="stwo_run_and_prove"
BINARY_NAME="stwo_run_and_prove"

# Build and install directories.
TOOLS_DIR="${REPO_ROOT}/target/tools"
BINARY_PATH="${TOOLS_DIR}/${BINARY_NAME}"

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
