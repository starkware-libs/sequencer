#!/bin/bash

# Common apt utilities with retry logic for CI environments.
# This file should be sourced by other scripts.

# Log a step with a visible separator for CI readability.
# Usage: log_step "script_name" "message"
function log_step() {
    local prefix="${1:-script}"
    local message="${2:-}"
    echo ""
    echo "========================================"
    echo "[${prefix}] ${message}"
    echo "========================================"
}

# Retry apt-get update with cache cleanup to handle transient mirror sync issues.
function apt_update_with_retry() {
    local max_attempts=5
    local attempt=1
    local delay=5

    while [ $attempt -le $max_attempts ]; do
        echo "apt-get update attempt $attempt of $max_attempts..."
        if apt-get update; then
            echo "apt-get update succeeded on attempt $attempt"
            return 0
        fi

        echo "apt-get update failed on attempt $attempt"

        if [ $attempt -lt $max_attempts ]; then
            echo "Cleaning apt cache and retrying in ${delay}s..."
            rm -rf /var/lib/apt/lists/*
            sleep $delay
            delay=$((delay * 2))
        fi

        attempt=$((attempt + 1))
    done

    echo "apt-get update failed after $max_attempts attempts"
    return 1
}

# Retry apt-get install to handle transient network issues.
function apt_install_with_retry() {
    local max_attempts=5
    local attempt=1
    local delay=5

    while [ $attempt -le $max_attempts ]; do
        echo "apt-get install attempt $attempt of $max_attempts..."
        if apt-get install "$@"; then
            echo "apt-get install succeeded on attempt $attempt"
            return 0
        fi

        echo "apt-get install failed on attempt $attempt"

        if [ $attempt -lt $max_attempts ]; then
            echo "Retrying in ${delay}s..."
            sleep $delay
            delay=$((delay * 2))
        fi

        attempt=$((attempt + 1))
    done

    echo "apt-get install failed after $max_attempts attempts"
    return 1
}

