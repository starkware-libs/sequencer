#!/bin/bash

add_or_update_var() {
    local file="$1"
    local key="$2"
    local value="$3"

    if grep -q '^\[env\]' "$file"; then
        # If [env] table already exists
        if ! grep -q "^\s*$key\s*=" "$file"; then
            # If the key is not present
            # Write the key just below the [env] line
            awk -v key="$key" -v value="$value" '
                /^\[env\]/ {
                    env_section = 1
                }
                { print }
                env_section {
                    print key " = { value = \"" value "\", relative = false}"
                    env_section = 0
                }
            ' "$file" > "$file.tmp" && mv "$file.tmp" "$file"
        fi
    else 
        # Otherwise, create the [env] table at the end and the new key
        echo "[env]" >> "$file"
        echo "$key = { value = \"$value\", relative = false }" >> "$file"
    fi
}

get_env_vars() {
    if [[ "$(uname)" == "Darwin" ]]; then
        ENV_VARS=(
            "LIBRARY_PATH:/opt/homebrew/lib"
            "LLVM_SYS_181_PREFIX:/opt/homebrew/opt/llvm/"
            "MLIR_SYS_180_PREFIX:/opt/homebrew/opt/llvm/"
            "TABLEGEN_180_PREFIX:/opt/homebrew/opt/llvm/"
        )
    elif [[ "$(uname)" == "Linux" ]]; then
        ENV_VARS=(
            "LLVM_SYS_181_PREFIX:/usr/lib/llvm-18"
            "MLIR_SYS_180_PREFIX:/usr/lib/llvm-18"
            "TABLEGEN_180_PREFIX:/usr/lib/llvm-18"
        )
    else
        echo "Unsupported platform: $(uname)"
        exit 1
    fi
}


# Add specific env vars to the blockifier config toml required for compilation and execution;
# required to locate the LLVM project binaries and the Cairo Native runtime's library.
# If there is no [env] table defined, it will be created and the new flags added
# Otherwise if any of the flags is already defined it won't be overriden.
main () {
    CONFIG_PATH="crates/blockifier/.cargo"
    CONFIG_FILE="${CONFIG_PATH}/config.toml"

    mkdir -p "$CONFIG_PATH"
    if [[ ! -f "$CONFIG_FILE" ]]; then
        touch "$CONFIG_FILE"
    fi

    get_env_vars

    for var in "${ENV_VARS[@]}"; do
        key="${var%%:*}"
        value="${var##*:}"
        add_or_update_var "$CONFIG_FILE" "$key" "$value"
    done

    echo "Configuration updated in $CONFIG_FILE"
}


main "$@"
