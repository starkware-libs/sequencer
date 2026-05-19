#!/bin/bash
set -euo pipefail

# Only run in Claude Code on the web, not on local machines
if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

echo "Setting up sequencer development environment..."

# Install sccache for faster Rust compilation caching
if ! command -v sccache &> /dev/null; then
  echo "Installing sccache..."
  cargo install sccache
fi

# Install nightly toolchain required by scripts/rust_fmt.sh
if ! rustup run nightly-2024-04-29 rustfmt --version &> /dev/null 2>&1; then
  echo "Installing nightly-2024-04-29 toolchain with rustfmt..."
  rustup toolchain install nightly-2024-04-29 --component rustfmt
fi

# Set up Python venv and install dependencies (includes gitpython)
if [ ! -f "$CLAUDE_PROJECT_DIR/sequencer_venv/bin/activate" ]; then
  echo "Creating Python venv..."
  python3 -m venv "$CLAUDE_PROJECT_DIR/sequencer_venv"
fi
source "$CLAUDE_PROJECT_DIR/sequencer_venv/bin/activate"
echo "Installing Python dependencies..."
pip install -q -r "$CLAUDE_PROJECT_DIR/scripts/requirements.txt"

# Install Graphite CLI for stacked PRs
if ! command -v gt &> /dev/null; then
  echo "Installing Graphite CLI..."
  npm install -g @withgraphite/graphite-cli
fi

echo "Environment setup complete."
