#!/bin/bash

NIGHTLY_VERSION="nightly-2024-04-29"

# Set environment variable
export RUSTFLAGS="-D warnings -C link-arg=-fuse-ld=lld"
export RUSTDOCFLAGS="-D warnings -C link-arg=-fuse-ld=lld"
export EXTRA_RUST_TOOLCHAINS=NIGHTLY_VERSION

# Set up.

# Commit lint.
packages=("@commitlint/cli" "@commitlint/config-conventional")

for pkg in "${packages[@]}"; do
  if npm list "$pkg" >/dev/null 2>&1; then
    echo "$pkg is already installed ✅"
  else
    echo "$pkg is NOT installed ❌ — installing..."
    npm install "$pkg"
  fi
done

# Set up the python venv and switch to it.

# Store current venv (if any)
CURRENT_VENV="$VIRTUAL_ENV"

VENV_NAME="~/presubmit_venv"

# Create venv if it doesn't exist.
if [ ! -d "$VENV_NAME" ]; then
  echo "Creating virtual environment: $VENV_NAME"
  python3 -m venv "$VENV_NAME"
else
  echo "Virtual environment '$VENV_NAME' already exists."
fi

if [ -n "$CURRENT_VENV" ]; then
  echo "Storing current venv: $CURRENT_VENV"
else
  echo "No active virtual environment"
fi

# Activate presubmit_venv.
echo "Activating $VENV_NAME"
source "$VENV_NAME/bin/activate"

# Install GitPython if not already installed.
if ! python3 -c "import git" &> /dev/null; then
  echo "GitPython is not installed. Installing..."
  pip3 install GitPython
else
  echo "GitPython is already installed ✅"
fi


# Presubmit checks begin

# Get the common ancestor commit hash of HEAD and origin/main
ancestor_commit=$(git merge-base HEAD origin/main)

# Check if merge-base succeeded
if [ -z "$ancestor_commit" ]; then
  echo "Failed to determine common ancestor of HEAD and origin/main"
  exit 1
fi

scripts_location=$(git rev-parse --show-toplevel)

cmd="python3 ${scripts_location}/scripts/presubmit_fast_cheks.py --mode local_presubmit --extra_rust_toolchains $NIGHTLY_VERSION --from_commit_hash \"$ancestor_commit\" --to_commit_hash HEAD"

echo $cmd

eval "$cmd"

# Deactivate presubmit_venv
deactivate
echo "Deactivated $VENV_NAME"

# Restore previous venv, if any
if [ -n "$CURRENT_VENV" ]; then
  echo "Reactivating previous venv: $CURRENT_VENV"
  source "$CURRENT_VENV/bin/activate"
else
  echo "No previous venv to restore."
fi