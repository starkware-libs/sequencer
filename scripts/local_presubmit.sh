#!/bin/bash

REPO_LOCATION=$(git rev-parse --show-toplevel)
declare -A ORIGINAL_VARS

setup_env_variables_from_yml() {
  YAML_FILE="${REPO_LOCATION}/.github/workflows/main.yml"

  # Extract top-level env variables using awk, output key=value
  vars=$(awk '
  BEGIN { in_env=0 }

  # Detect the beginning of the env block
  /^env:/ { in_env=1; next }

  # If a line is non-indented (i.e. starts at column 1), we exited the env block
  /^[^[:space:]]/ { in_env=0 }
  
  # If we are inside the env block and the line looks like a YAML key-value pair
  in_env && /^[[:space:]]+[A-Za-z_][A-Za-z0-9_]*:/ {
    match($0, /^[[:space:]]+([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*:[[:space:]]*(.*)$/, arr)
    key = arr[1]
    val = arr[2]
    gsub(/^["'\''"]|["'\''"]$/, "", val)
    print key "=" val
  }
  ' "$YAML_FILE")

  # Save original values and export new ones
  while IFS='=' read -r key val; do
    if [[ "$key" == "CI" ]]; then
      continue  # Do not set CI env since we're not running in CI.
    fi
    ORIGINAL_VARS["$key"]="${!key}"  # Save current value
    echo Setting env variable: "$key"="$val"
    export "$key"="$val"
  done <<< "$vars"
}

install_dependencies() {
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

  # Rust env needed.
  if rustup toolchain list | grep -q "${EXTRA_RUST_TOOLCHAINS}"; then
    echo "Rust toolchain ${EXTRA_RUST_TOOLCHAINS} is already installed ✅."
  else
    echo "Rust toolchain ${EXTRA_RUST_TOOLCHAINS} not found ❌. Installing..."
    rustup toolchain install "${EXTRA_RUST_TOOLCHAINS}"
    rustup component add --toolchain "${EXTRA_RUST_TOOLCHAINS}" rustfmt
  fi

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

  # Restore original values of env variables set from main.yml
  for key in "${!ORIGINAL_VARS[@]}"; do
    if [ -z "${ORIGINAL_VARS[$key]}" ] && [ -n "${!key}" ]; then
      unset "$key"  # Was not originally set
      echo Unsetting "$key".
    else
      echo Setting env variable: "$key" back to original value.
      export "$key"="${ORIGINAL_VARS[$key]}"
    fi
  done
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

trap restore_old_env EXIT
trap restore_old_env INT

setup_env_variables_from_yml
setup_new_venv
install_dependencies
add_commit_lint_to_path

# Presubmit checks begin:

# Get the common ancestor commit hash of HEAD and origin/main
parent_branch=$(head -n 1 ${REPO_LOCATION}/scripts/parent_branch.txt)
ancestor_commit=$(git merge-base HEAD origin/${parent_branch})

# Check if merge-base succeeded
if [ -z "$ancestor_commit" ]; then
  echo "Failed to determine common ancestor of HEAD and origin/main"
  exit 1
fi

if [[ -z "$EXTRA_RUST_TOOLCHAINS" ]]; then
  echo "Error: EXTRA_RUST_TOOLCHAINS is not set or is empty"
  exit 1
fi
cmd="python3 ${REPO_LOCATION}/scripts/presubmit_fast_checks.py all --extra_rust_toolchains ${EXTRA_RUST_TOOLCHAINS} --from_commit_hash \"$ancestor_commit\" --to_commit_hash HEAD"

echo $cmd

eval "$cmd"
