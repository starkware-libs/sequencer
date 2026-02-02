#!/usr/bin/env bash
set -euo pipefail

# CDK8S Deployment Project Setup Script
# This script only handles project setup (pipenv install, cdk8s import)
# System dependencies should be installed via Dockerfile

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Print colored messages
info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

# Check if command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Setup project (pipenv install + cdk8s import)
setup_project() {
    info "Setting up project..."
    
    # Use SCRIPT_DIR if it's a valid project directory (has Pipfile), otherwise use current directory
    if [ -f "$SCRIPT_DIR/Pipfile" ]; then
        cd "$SCRIPT_DIR"
    elif [ -f "./Pipfile" ]; then
        # Already in the right directory
        :
    else
        # Try to find the project directory
        if [ -f "/workspace/deployments/sequencer/Pipfile" ]; then
            cd /workspace/deployments/sequencer
        else
            error "Could not find Pipfile. Please run this script from the project directory."
            return 1
        fi
    fi
    
    # Remove existing virtual environment if it exists (may be incompatible)
    if [[ -d ".venv" ]]; then
        info "Removing existing .venv directory (may be incompatible with current Python version)..."
        rm -rf .venv
        success "Removed existing virtual environment"
    fi
    
    # Ensure ~/.local/bin is in PATH (pipenv might be installed there)
    if [[ -d "$HOME/.local/bin" ]] && [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
        export PATH="$HOME/.local/bin:$PATH"
    fi
    
    # Install Python dependencies
    info "Installing Python dependencies with pipenv..."
    if ! command_exists pipenv; then
        error "pipenv not found. Please ensure pipenv is installed."
        error "If pipenv was just installed, you may need to run: export PATH=\"\$HOME/.local/bin:\$PATH\""
        return 1
    fi
    
    # Use the current Python version (pipenv may complain about version mismatch in Pipfile)
    # This allows the script to work even if Pipfile specifies a different version
    local python_cmd
    if command_exists python3; then
        python_cmd="python3"
    else
        error "python3 not found"
        return 1
    fi
    
    info "Using $python_cmd ($($python_cmd --version 2>&1)) for pipenv virtual environment"
    # Ensure PATH is exported so subprocesses can find pipenv
    export PATH
    pipenv install --python "$python_cmd"
    success "Python dependencies installed"
    
    # Initialize cdk8s imports
    info "Initializing cdk8s imports..."
    if ! command_exists cdk8s; then
        error "cdk8s-cli not found. Please ensure cdk8s-cli is installed."
        return 1
    fi
    
    cdk8s import
    success "cdk8s imports initialized"
    return 0
}

# Main function
main() {
    setup_project
    success "Project setup complete!"
    info "You can now use the project:"
    echo "  cdk8s synth --app \"pipenv run python -m main --namespace <namespace>\" -l <layout> -o <overlay>"
}

# Run main function
main "$@"
