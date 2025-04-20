#!/bin/bash

NIGHTLY_VERSION="nightly-2024-04-29"

# Set environment variable
export RUSTFLAGS="-D warnings -C link-arg=-fuse-ld=lld"
export RUSTDOCFLAGS="-D warnings -C link-arg=-fuse-ld=lld"
export EXTRA_RUST_TOOLCHAINS=NIGHTLY_VERSION

setup_packages() {
  packages=("@commitlint/cli" "@commitlint/config-conventional")

  for pkg in "${packages[@]}"; do
    if npm list "$pkg" >/dev/null 2>&1; then
      echo "$pkg is already installed ✅"
    else  
      echo "$pkg is NOT installed ❌ — installing..."
      npm install "$pkg"
    fi
  done

  # List of crate names to check/install
  CRATES=("taplo-cli" "cargo-machete")

  # Get the list of installed crates
  INSTALLED_CRATES=$(cargo install --list | grep -Eo '^[^ ]+')

  for crate in "${CRATES[@]}"; do
    echo "Checking crate: $crate..."

    if echo "$INSTALLED_CRATES" | grep -qx "$crate"; then
      echo "$crate is already installed ✅."
    else
      echo "$crate not found ❌. Installing..."
      cargo install "$crate"
    fi
  done

  # Install GitPython if not already installed.
  if ! python3 -c "import git" &> /dev/null; then
    echo "GitPython is not installed. Installing..."
    pip3 install GitPython
  else
    echo "GitPython is already installed ✅"
  fi
}

setup_new_venv() {
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
}

restore_old_env() {
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

  # Restore PATH
  if [ -n "$ORIGINAL_PATH" ]; then
    export PATH="$ORIGINAL_PATH"
  fi
}

add_commit_lint_to_path() {
  # Step 1: Try to locate commitlint using whereis
  COMMITLINT_PATH="$(whereis -b commitlint | awk '{print $2}')"
  if [ -n "$COMMITLINT_PATH" ]; then
    echo 'commitlint found in $PATH'
    return
  fi

  # Step 2: If whereis fails, use find to search from home directory
  echo "commitlint not found via whereis. Consider adding it to your path. Searching with find..."
  COMMITLINT_PATH="$(find ~/ \( -type f -o -type l \) -name commitlint -perm -u+x 2>/dev/null | grep "bin/" | head -n 1 | xargs dirname)"

  # Step 3: Add to path if needed
  if [ -n "$COMMITLINT_PATH" ]; then
    echo "commitlint found at: $COMMITLINT_PATH"
    ORIGINAL_PATH="$PATH"
    export PATH="$COMMITLINT_PATH:$PATH"
  else
    echo "commitlint not found in PATH or local directories."
    exit 1
  fi
}

setup_new_venv
setup_packages
add_commit_lint_to_path

# Presubmit checks begin:

repo_location=$(git rev-parse --show-toplevel)

# Get the common ancestor commit hash of HEAD and origin/main
parent_branch=$(head -n 1 ${repo_location}/scripts/parent_branch.txt)
ancestor_commit=$(git merge-base HEAD origin/${parent_branch})

echo $ancestor_commit

# Check if merge-base succeeded
if [ -z "$ancestor_commit" ]; then
  echo "Failed to determine common ancestor of HEAD and origin/main"
  exit 1
fi


cmd="python3 ${repo_location}/scripts/presubmit_fast_checks.py --extra_rust_toolchains $NIGHTLY_VERSION --from_commit_hash \"$ancestor_commit\" --to_commit_hash HEAD"

echo $cmd

eval "$cmd"

restore_old_env
