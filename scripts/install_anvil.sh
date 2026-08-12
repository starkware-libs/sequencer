#!/usr/bin/env bash
# Installs Foundry's Anvil (plus forge/cast) via foundryup, retrying both the
# foundryup installer download and the anvil install itself.
#
# Why retry at all: foundryup already retries its own downloads internally
# (observed "curl: (56) Connection died, tried 5 times before giving up") and
# still fails on a transient upstream 503/connection drop. Without an outer
# retry, that transient failure lands in a setup step, so a broken CI run
# looks like a broken commit even though the commit under test is untouched.
#
# Usage:
#   scripts/install_anvil.sh <foundry_version>
#
# <foundry_version> is passed straight to `foundryup --install`, e.g. v1.5.1.
#
# Env:
#   FOUNDRY_DIR - install root for foundryup and the foundry binaries.
#                 Defaults to $HOME/.foundry, and is exported so foundryup uses the same root.
#
# On success, appends the foundry bin directory to $GITHUB_PATH (when set)
# so later CI steps find anvil/forge/cast on PATH, and prints the installed
# anvil version as a self-check so a silently broken install fails here
# rather than surfacing as a confusing failure in a later step.

set -euo pipefail

readonly max_attempts=5
readonly backoff_seconds_per_attempt=10

if [ $# -lt 1 ] || [ -z "$1" ]; then
    echo "Usage: $0 <foundry_version>" >&2
    echo "Example: $0 v1.5.1" >&2
    exit 1
fi

readonly foundry_version="$1"
# Exported, not just read: the foundryup installer defaults to "${XDG_CONFIG_HOME:-$HOME}/.foundry",
# so leaving FOUNDRY_DIR unset would install under the XDG path while this script looks under $HOME.
# Exporting a concrete value makes the installer and this script agree in either environment.
export FOUNDRY_DIR="${FOUNDRY_DIR:-$HOME/.foundry}"
readonly foundry_dir="${FOUNDRY_DIR}"
readonly foundry_bin_dir="${foundry_dir}/bin"

# Runs "$@" up to max_attempts times with increasing backoff between
# attempts. Logs each failed attempt and the final give-up to stderr so the
# CI log makes it obvious this was a retried transient failure, not a
# deterministic break in the commit under test.
function retry_with_backoff() {
    local attempt=1
    while [ "${attempt}" -le "${max_attempts}" ]; do
        if "$@"; then
            return 0
        fi
        echo "install_anvil: attempt ${attempt}/${max_attempts} failed for: $*" >&2
        if [ "${attempt}" -lt "${max_attempts}" ]; then
            local backoff_seconds=$((attempt * backoff_seconds_per_attempt))
            echo "install_anvil: retrying in ${backoff_seconds}s..." >&2
            sleep "${backoff_seconds}"
        fi
        attempt=$((attempt + 1))
    done
    echo "install_anvil: giving up after ${max_attempts} attempts for: $*" >&2
    return 1
}

# Installs foundryup itself. `pipefail` (set above) is what makes a curl
# failure here fail the whole pipeline instead of being swallowed by bash's
# success-of-last-command exit status.
function install_foundryup() {
    curl -sSf -L https://foundry.paradigm.xyz | bash
}

# Installs anvil/forge/cast at foundry_version via the foundryup binary that
# install_foundryup placed in foundry_bin_dir.
function install_foundry_binaries() {
    "${foundry_bin_dir}/foundryup" --install "${foundry_version}"
}

retry_with_backoff install_foundryup
retry_with_backoff install_foundry_binaries

# Only append to GITHUB_PATH when running in CI; a local invocation of this
# script (e.g. for verification) has no such file and no such step to feed.
if [ -n "${GITHUB_PATH:-}" ]; then
    echo "${foundry_bin_dir}" >> "${GITHUB_PATH}"
fi

# Self-check: fail this step, with a clear anvil-specific message, rather
# than letting a broken install surface later as an opaque "anvil: command
# not found" in an unrelated step.
"${foundry_bin_dir}/anvil" --version
