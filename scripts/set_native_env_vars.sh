#!/bin/bash

detect_default_triple() {
    if [[ "$(uname)" == "Darwin" ]]; then
        if [[ "$(uname -m)" == "arm64" ]]; then
            echo "aarch64-apple-darwin"
        else
            echo "x86_64-apple-darwin"
        fi
    elif [[ "$(uname)" == "Linux" ]]; then
        if [[ "$(uname -m)" == "x86_64" ]]; then
            echo "x86_64-unknown-linux-gnu"
        else
            echo "$(uname -m)-unknown-linux-gnu"
        fi
    else
        echo "unsupported"
    fi
}

detect_target_flag() {
    target_option_idx=0
    for i in "$@"; do
        if [[ "$i" == "--target" ]]; then
            break
        fi
        target_option_idx=$((target_option_idx + 1))
    done
        
    if [[ "$target_option_idx" -lt "$#" ]]; then
        target_value_idx=$((target_option_idx+1))
        echo "${!target_value_idx}"
    else 
        default_target=$(detect_default_triple)
        echo "$default_target"
    fi
}


target_value=$(detect_target_flag "$@")

if [[ "$target_value" == "aarch64-apple-darwin" ]]; then
    exec /usr/bin/env LLVM_SYS_181_PREFIX=/opt/homebrew/opt/llvm/ \
                      MLIR_SYS_180_PREFIX=/opt/homebrew/opt/llvm/ \
                      TABLEGEN_180_PREFIX=/opt/homebrew/opt/llvm/ \
                      LIBRARY_PATH=/opt/homebrew/lib/ \
                      "$@"
elif [[ "$target_value" == "x86_64-unknown-linux-gnu" ]]; then 
    exec /usr/bin/env LLVM_SYS_181_PREFIX=/usr/lib/llvm-18 \
                      MLIR_SYS_180_PREFIX=/usr/lib/llvm-18 \
                      TABLEGEN_180_PREFIX=/usr/lib/llvm-18 \
                      "$@"
else
    echo "Error: Target machine '$target_value' is not supported."
    exec /usr/bin/env "$@"
fi


