#!/bin/bash

GIT_DIR=$(git rev-parse --git-common-dir)
HOOK_FILE="$GIT_DIR/hooks/post-checkout"
HOOK_CONTENT="git submodule update --init --recursive"

show_help() {
    echo "Usage: $0 [OPTIONS]"
    echo
    echo "A script to add and remove a post-checkout Git hook for updating submodules."
    echo
    echo "Options:"
    echo "  --install        Install or update the post-checkout hook (default action if no flag is provided)."
    echo "  --uninstall      Remove the submodule update command from the post-checkout hook."
    echo "  --help           Display this help message."
}

function install_hook() {
    # Ensure the hooks directory exists
    mkdir -p "$GIT_DIR/hooks"

    # Check if the hook file exists
    if [ ! -f "$HOOK_FILE" ]; then
        # Create a new hook file with the required content
        echo -e "#!/bin/bash\n$HOOK_CONTENT" >"$HOOK_FILE"
        chmod +x "$HOOK_FILE"
        echo "Created new post-checkout hook."
        echo "Whenever checking out a commit, the hook will run the following command:"
        echo "  $HOOK_CONTENT"
    else
        # Ensure the file starts with the shebang
        if ! grep -q "^#!/bin/bash" "$HOOK_FILE"; then
            sed -i "1i #!/bin/bash" "$HOOK_FILE"
            echo "Added shebang to existing post-checkout hook."
        fi

        # Append the submodule update command if not already present
        if ! grep -q "$HOOK_CONTENT" "$HOOK_FILE"; then
            echo "$HOOK_CONTENT" >>"$HOOK_FILE"
            echo "Appended submodule update command to post-checkout hook."
        else
            echo "post-checkout hook already contains the submodule update command."
        fi

        # Ensure the file is executable
        chmod +x "$HOOK_FILE"
    fi
}

function uninstall_hook() {
    # Check if the hook file exists
    if [ ! -f "$HOOK_FILE" ]; then
        echo "No post-checkout hook found. Nothing to remove."
        exit 0
    fi

    # Check if the hook contains the specific submodule command
    if grep -q "$HOOK_CONTENT" "$HOOK_FILE"; then
        # Remove the specific submodule update command
        sed -i "/$HOOK_CONTENT/d" "$HOOK_FILE"
        echo "Removed submodule update command from post-checkout hook."

        # If the file is now empty or only contains the shebang, remove it entirely
        if [ ! -s "$HOOK_FILE" ] || [ "$(wc -l <"$HOOK_FILE")" -eq 1 ] && grep -q "^#!/bin/bash" "$HOOK_FILE"; then
            rm "$HOOK_FILE"
            echo "Removed empty post-checkout hook."
        fi
    else
        echo "Submodule update command not found in post-checkout hook. No changes made."
    fi
}

function main() {
    case "$1" in
    --install | "")
        install_hook
        ;;
    --uninstall)
        uninstall_hook
        ;;
    --help)
        show_help
        ;;
    *)
        echo "Unknown option: $1"
        echo "Use --help to see available options."
        exit 1
        ;;
    esac
}

main "$@"
