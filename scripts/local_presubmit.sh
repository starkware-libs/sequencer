#!/bin/bash

PRESUBMIT_DEBUG_LEVEL=0

REPO_LOCATION=$(git rev-parse --show-toplevel)
declare -A ORIGINAL_VARS


log_debug() {
  [[ $DEBUG_LEVEL -ge 1 ]] && echo "[DEBUG] $*"
}

install_yq() {
  if ! command -v yq &> /dev/null; then
    echo "yq not found ❌. Attempting to install using snap. Password may be required."
    if ! command -v snap &> /dev/null; then
        echo "Error: snap is not available on this system. Please install snap or yq manually."
        exit 1
    fi
    sudo snap install yq || {
        echo "Failed to install yq via snap."
        exit 1
    }
    echo "yq installed successfully."
  else
    log_debug "yq is already installed ✅"
  fi
}

setup_env_variables_from_yml() {
  YAML_FILE="${REPO_LOCATION}/.github/workflows/main.yml"

  # Extract and export environment variables from the YAML
  while IFS="=" read -r key value; do
    if [[ "$key" == "CI" ]]; then
      continue  # Do not set CI env since we're not running in CI.
    fi
    ORIGINAL_VARS["$key"]="${!key}"  # Save current value
    log_debug Setting env variable: "$key"="$value"
    export "$key"="$value"
  done < <(yq eval '.env | to_entries | .[] | "\(.key)=\(.value)"' "$YAML_FILE")
}

install_dependencies() {
  packages=("@commitlint/cli" "@commitlint/config-conventional")

  for pkg in "${packages[@]}"; do
    if npm list "$pkg" >/dev/null 2>&1; then
      log_debug "$pkg is already installed ✅"
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
    log_debug "Checking crate: $crate..."

    if echo "$INSTALLED_CRATES" | grep -qx "$crate"; then
      log_debug "$crate is already installed ✅."
    else
      echo "$crate not found ❌. Installing..."
      cargo install "$crate"
    fi
  done

  # Rust env needed. Should be imported from main.yml
  if [[ -z "$EXTRA_RUST_TOOLCHAINS" ]]; then
    echo "Error: EXTRA_RUST_TOOLCHAINS is not set or is empty"
    exit 1
  fi
  if rustup toolchain list | grep -q "${EXTRA_RUST_TOOLCHAINS}"; then
    log_debug "Rust toolchain ${EXTRA_RUST_TOOLCHAINS} is already installed ✅."
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
    log_debug "GitPython is already installed ✅"
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
    log_debug "Virtual environment '$VENV_NAME' already exists."
  fi

  if [ -n "$CURRENT_VENV" ]; then
    log_debug "Storing current venv: $CURRENT_VENV"
  else
    log_debug "No active virtual environment"
  fi

  # Activate presubmit_venv.
  log_debug "Activating $VENV_NAME"
  source "$VENV_NAME/bin/activate"
}

restore_old_env() {
  # Deactivate presubmit_venv
  deactivate
  log_debug "Deactivated $VENV_NAME"

  # Restore previous venv, if any
  if [ -n "$CURRENT_VENV" ]; then
    log_debug "Reactivating previous venv: $CURRENT_VENV"
    source "$CURRENT_VENV/bin/activate"
  else
    log_debug "No previous venv to restore."
  fi

  # Restore PATH
  if [ -n "$ORIGINAL_PATH" ]; then
    export PATH="$ORIGINAL_PATH"
  fi

  # Restore original values of env variables set from main.yml
  for key in "${!ORIGINAL_VARS[@]}"; do
    if [ -z "${ORIGINAL_VARS[$key]}" ] && [ -n "${!key}" ]; then
      unset "$key"  # Was not originally set
      log_debug Unsetting "$key".
    else
      log_debug Setting env variable: "$key" back to original value.
      export "$key"="${ORIGINAL_VARS[$key]}"
    fi
  done
}

add_commit_lint_to_path() {
  # Step 1: Try to locate commitlint using which.
  COMMITLINT_PATH="$(which commitlint)"
  if [ -n "$COMMITLINT_PATH" ]; then
    log_debug 'commitlint found in $PATH'
    return
  fi

  # Step 2: If which fails, use find to search from home directory
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

setup_new_venv
# YQ must be installed for setting up the environment variables and install_dependencies relies on
# the environment variables set from the YAML file.
install_yq
setup_env_variables_from_yml
install_dependencies
add_commit_lint_to_path

# Presubmit checks begin:

# Get the common ancestor commit hash of HEAD and origin/{branch}}
parent_branch=$(head -n 1 ${REPO_LOCATION}/scripts/parent_branch.txt)
ancestor_commit=$(git merge-base HEAD origin/${parent_branch})

# Check if merge-base succeeded
if [ -z "$ancestor_commit" ]; then
  echo "Failed to determine common ancestor of HEAD and ${parent_branch}"
  exit 1
fi

cmd="python3 ${REPO_LOCATION}/scripts/presubmit_fast_checks.py all --extra_rust_toolchains ${EXTRA_RUST_TOOLCHAINS} --from_commit_hash \"$ancestor_commit\" --to_commit_hash HEAD"

echo $cmd

eval "$cmd"
