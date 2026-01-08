#!/bin/bash
#
# Shared configuration for proving-utils tool installers.
#
# Environment Variables:
#   PROVING_UTILS_REV - Override the default pinned revision.
#
# This file sets:
#   PROVING_UTILS_REPO
#   PROVING_UTILS_REV
#   REPO_ROOT
#   THIRD_PARTY_DIR
#   BUILD_DIR

# If any command fails, exit immediately.
set -euo pipefail

PROVING_UTILS_REPO="https://github.com/starkware-libs/proving-utils"
PROVING_UTILS_REV_DEFAULT="3176b4d"
PROVING_UTILS_REV="${PROVING_UTILS_REV:-${PROVING_UTILS_REV_DEFAULT}}"

COMMON_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${COMMON_DIR}/.." && pwd)"

THIRD_PARTY_DIR="${REPO_ROOT}/target/third_party"
BUILD_DIR="${THIRD_PARTY_DIR}/proving-utils-${PROVING_UTILS_REV}"
