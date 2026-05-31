#!/bin/env bash
# Installs LLVM 19 + MLIR/Polly dev packages required by starknet-native-compile.
# Idempotent: skips if llvm-config-19 is already on PATH.
#
# Sourced by both scripts/dependencies.sh and scripts/install_compiler_binaries.sh
# (the latter as an opt-in self-recovery fallback when LLVM 19 is missing).

set -e

[[ ${UID} == "0" ]] || SUDO="sudo"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
if [ -f "${SCRIPT_DIR}/apt_utils.sh" ]; then
    source "${SCRIPT_DIR}/apt_utils.sh"
elif [ -f "./apt_utils.sh" ]; then
    source "./apt_utils.sh"
else
    echo "Error: apt_utils.sh not found in ${SCRIPT_DIR} or current directory" >&2
    exit 1
fi

function install_llvm19() {
    if command -v llvm-config-19 &>/dev/null; then
        log_step "install_llvm19" "LLVM 19 already installed, skipping"
        return 0
    fi
    log_step "install_llvm19" "Setting up LLVM 19 dependencies..."
    case "$(uname)" in
    Darwin)
        echo "Detected macOS, using Homebrew..."
        brew update
        brew install llvm@19
        ;;
    Linux)
        echo "Detected Linux, using apt..."
        local workdir llvm_sh
        workdir=$(mktemp -d)
        llvm_sh="${workdir}/llvm.sh"
        # Bash-specific RAII: the RETURN trap fires when this function returns
        # (success, explicit return, or set -e early exit), guaranteeing cleanup
        # regardless of which step below fails. Equivalent to try/finally scoped
        # to the function.
        trap 'rm -rf "$workdir"' RETURN
        echo "Downloading LLVM installation script..."
        curl --proto "=https" --tlsv1.2 --fail -L -o "$llvm_sh" https://apt.llvm.org/llvm.sh
        echo "Running LLVM 19 installation script..."
        $SUDO bash "$llvm_sh" 19 all
        echo "Installing LLVM-related packages (MLIR, Polly, etc.)..."
        # `bash -c '...'` spawns a fresh shell that doesn't inherit functions
        # from the parent shell, only exported env vars. The apt_utils.sh
        # helpers sourced above therefore aren't reachable inside the sudo
        # subshell. `declare -f` splices the function definitions into the
        # subshell's script text so the apt_*_with_retry calls below resolve.
        $SUDO bash -c "$(declare -f apt_update_with_retry); $(declare -f apt_install_with_retry)"'
        apt_update_with_retry && apt_install_with_retry -y \
            libgmp3-dev \
            libmlir-19-dev \
            libpolly-19-dev \
            libzstd-dev \
            mlir-19-tools
        '
        ;;
    *)
        echo "Error: Unsupported operating system" >&2
        exit 1
        ;;
    esac
    log_step "install_llvm19" "LLVM 19 installed successfully"
}

# Run when invoked directly; allow sourcing without auto-running.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    install_llvm19
fi
