#!/usr/bin/env bash
# Installs Foundry's Anvil from the release tarball, retrying a failed download.
#
# The tarball is one pinned request. foundryup instead fetches an unpinned installer and makes
# several further requests, each one a chance to fail in a setup step.
# crates/apollo_base_layer_tests/src/anvil_base_layer.rs documents the same URL for local installs;
# keep the version here in step with it.
#
# Usage:
#   scripts/install_anvil.sh <foundry_version>     # e.g. v1.5.1
#
# Env:
#   ANVIL_INSTALL_DIR - where the anvil binary is placed. Defaults to $HOME/.foundry/bin.
#
# Appends the install directory to $GITHUB_PATH when set, and prints the installed version so a
# broken install fails here rather than as "anvil: command not found" in a later step.

set -euo pipefail

readonly max_attempts=5
readonly backoff_seconds_per_attempt=10

if [ $# -lt 1 ] || [ -z "$1" ]; then
    echo "Usage: $0 <foundry_version>" >&2
    echo "Example: $0 v1.5.1" >&2
    exit 1
fi

readonly foundry_version="$1"
readonly install_dir="${ANVIL_INSTALL_DIR:-$HOME/.foundry/bin}"
readonly release_url="https://github.com/foundry-rs/foundry/releases/download/${foundry_version}/foundry_${foundry_version}_linux_amd64.tar.gz"

# Streams into tar, so the 80MB archive is never written to disk. Extracts only anvil; nothing in
# CI uses forge, cast or chisel.
#
# curl's --retry would restart the transfer and corrupt tar's input mid-stream, so the outer loop
# owns restarting.
function download_anvil() {
    rm -f "${install_dir}/anvil"

    local curl_stderr tar_stderr pipe_status
    curl_stderr=$(mktemp)
    tar_stderr=$(mktemp)

    # PIPESTATUS is reset by every subsequent command, including an assignment, so copy the whole
    # array at once. `set -e` is suppressed here because retry_with_backoff calls this function as
    # an `if` condition.
    curl --fail --silent --show-error --location "${release_url}" 2>"${curl_stderr}" \
        | tar -xz -C "${install_dir}" --wildcards 'anvil' 2>"${tar_stderr}"
    pipe_status=("${PIPESTATUS[@]}")

    # The extracted binary is the source of truth: tar can finish before curl and leave curl
    # killed by SIGPIPE on a good extraction. Both stderrs are reported on failure, because a bad
    # URL fails curl and then surfaces as an empty tar error.
    if [ ! -s "${install_dir}/anvil" ]; then
        echo "install_anvil: download failed (curl=${pipe_status[0]}, tar=${pipe_status[1]}):" \
            "$(cat "${curl_stderr}") $(cat "${tar_stderr}")" >&2
        rm -f "${curl_stderr}" "${tar_stderr}"
        return 1
    fi

    rm -f "${curl_stderr}" "${tar_stderr}"
}

# Runs "$@" up to max_attempts times with increasing backoff, logging each failure to stderr so a
# retried transient failure is distinguishable from a broken commit.
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

mkdir -p "${install_dir}"
retry_with_backoff download_anvil
chmod +x "${install_dir}/anvil"

# Set only in CI; a local invocation has no such file.
if [ -n "${GITHUB_PATH:-}" ]; then
    echo "${install_dir}" >> "${GITHUB_PATH}"
fi

"${install_dir}/anvil" --version
