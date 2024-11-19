#!/bin/bash
# Add a post-checkout hook to update git submodules automatically when checking out a commit.
function add_chechout_hook_with_submodule_update() {
    # Get the Git directory
    GIT_DIR=$(git rev-parse --git-common-dir)
    HOOK_FILE="$GIT_DIR/hooks/post-checkout"
    HOOK_CONTENT="git submodule update --init --recursive"

    # Ensure the hooks directory exists
    mkdir -p "$GIT_DIR/hooks"

    # Check if the hook file exists
    if [ ! -f "$HOOK_FILE" ]; then
        # Create a new hook file with the required content
        echo -e "#!/bin/bash\n$HOOK_CONTENT" >"$HOOK_FILE"
        chmod +x "$HOOK_FILE"
        echo "Created new post-checkout hook."
        echo "The hook will run the following command:"
        echo ""
        echo "  $HOOK_CONTENT"
        echo ""
        echo "When checking out a commit."
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

add_chechout_hook_with_submodule_update
