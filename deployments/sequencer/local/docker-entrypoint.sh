#!/bin/bash
set -euo pipefail

# Simple entrypoint to create user entry for the host UID/GID
# This prevents "I have no name!" prompt when using -u $UID:$GID

USER_ID="${USER_ID:-1000}"
GROUP_ID="${GROUP_ID:-1000}"
USER_NAME="${USER_NAME:-user}"

# Set locale so pipenv and other tools don't warn (e.g. "LANG is not set")
export LANG="${LANG:-C.UTF-8}"
export LC_ALL="${LC_ALL:-C.UTF-8}"

# Pipenv: create venv in project (.venv) so we don't need ~/.local
export PIPENV_VENV_IN_PROJECT=1

# Create group entry if it doesn't exist
if ! getent group "$GROUP_ID" &>/dev/null; then
    if ! getent group "$USER_NAME" &>/dev/null; then
        groupadd -g "$GROUP_ID" "$USER_NAME" 2>/dev/null || \
        groupadd "$USER_NAME" 2>/dev/null || true
    fi
fi

# Create user entry if it doesn't exist
if ! getent passwd "$USER_ID" &>/dev/null; then
    if ! getent passwd "$USER_NAME" &>/dev/null; then
        # Get group name for the GID
        GROUP_NAME=$(getent group "$GROUP_ID" | cut -d: -f1 || echo "$USER_NAME")
        useradd -u "$USER_ID" -g "$GROUP_NAME" -m -s /bin/bash "$USER_NAME" 2>/dev/null || \
        useradd -g "$GROUP_NAME" -m -s /bin/bash "$USER_NAME" 2>/dev/null || true
        
        # Copy .bashrc and .bash_profile from /etc/skel if user was just created and home exists
        USER_HOME=$(getent passwd "$USER_NAME" | cut -d: -f6)
        if [[ -n "$USER_HOME" ]] && [[ -d "$USER_HOME" ]]; then
            if [[ ! -f "$USER_HOME/.bashrc" ]]; then
                cp /etc/skel/.bashrc "$USER_HOME/.bashrc" 2>/dev/null || true
                chown "$USER_NAME:$GROUP_NAME" "$USER_HOME/.bashrc" 2>/dev/null || true
            fi
            if [[ ! -f "$USER_HOME/.bash_profile" ]]; then
                cp /etc/skel/.bash_profile "$USER_HOME/.bash_profile" 2>/dev/null || true
                chown "$USER_NAME:$GROUP_NAME" "$USER_HOME/.bash_profile" 2>/dev/null || true
            fi
        fi
    fi
fi

# Fix home dir ownership (Docker may have pre-created it as root for volume mounts like .kube-tmp)
# Only chown the directory itself, not -R: .kube-tmp is a read-only mount from the host
USER_HOME=$(getent passwd "$USER_NAME" | cut -d: -f6)
if [[ -n "$USER_HOME" ]] && [[ -d "$USER_HOME" ]]; then
    chown "$USER_NAME:" "$USER_HOME"
fi

# Copy read-only .kube-tmp (host ~/.kube) to writable .kube so kubectl/kubectx work without touching the host
if [[ -n "$USER_HOME" ]] && [[ -d "$USER_HOME/.kube-tmp" ]]; then
    rm -rf "$USER_HOME/.kube"
    cp -a "$USER_HOME/.kube-tmp" "$USER_HOME/.kube"
    chown -R "$USER_NAME:" "$USER_HOME/.kube"
fi

# Add user to sudoers (passwordless sudo)
if command -v sudo &>/dev/null; then
    SUDOERS_FILE="/etc/sudoers.d/$USER_NAME"
    # Always ensure the sudoers file exists and is correct
    mkdir -p /etc/sudoers.d
    echo "$USER_NAME ALL=(ALL) NOPASSWD:ALL" > "$SUDOERS_FILE"
    chmod 0440 "$SUDOERS_FILE"
    # Validate sudoers file syntax
    if ! visudo -cf "$SUDOERS_FILE" &>/dev/null; then
        echo "Warning: sudoers file validation failed for $SUDOERS_FILE" >&2
    fi
fi

# Run setup.sh automatically if project isn't already set up
# Only run for interactive bash shells (default CMD)
if [ -d "/workspace/deployments/sequencer" ]; then
    cd /workspace/deployments/sequencer
    
    # Check if we're running the default interactive shell command
    # Default CMD is ["/bin/bash", "-l"], so check if first arg is bash-related
    if [[ "$1" == *"bash"* ]] || [[ "$*" == *"bash"* ]] || [ $# -eq 0 ]; then
        # Check if project is already set up (has .venv and imports directory)
        if [ ! -d ".venv" ] || [ ! -d "imports" ]; then
            echo "Running automatic project setup..."
            # Switch to user and run setup in the correct directory, but don't fail if it errors
            gosu "$USER_NAME" bash -c "cd /workspace/deployments/sequencer && /usr/local/bin/setup.sh" || true
        fi
    fi
fi

# Switch to the user and execute the command
# The entrypoint runs as root, so we use gosu to switch to the created user
exec gosu "$USER_NAME" "$@"
