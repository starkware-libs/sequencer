#!/bin/env bash
set -e

# Extract version constants from Rust source files
CAIRO_VERSION=$(grep 'CAIRO1_COMPILER_VERSION.*=' crates/apollo_infra_utils/src/cairo_compiler_version.rs | sed 's/.*"\(.*\)".*/\1/')
CAIRO_NATIVE_VERSION=$(grep 'REQUIRED_CAIRO_NATIVE_VERSION.*=' crates/apollo_compile_to_native/src/constants.rs | sed 's/.*"\(.*\)".*/\1/')

echo "📦 Required versions:"
echo "  - Cairo Compiler: ${CAIRO_VERSION}"
echo "  - Cairo Native: ${CAIRO_NATIVE_VERSION}"

function clean() {
    echo "🧹 Cleaning up..."
    deactivate || true
    rm -rf venv || true
}

function install_compiler_binaries() {
    echo "🔧 Installing required compiler binaries..."

    # Install starknet-sierra-compile
    echo "📦 Installing starknet-sierra-compile v${CAIRO_VERSION}..."
    cargo install starknet-sierra-compile --version "${CAIRO_VERSION}" --locked || {
        echo "❌ Failed to install starknet-sierra-compile v${CAIRO_VERSION}"
        return 1
    }

    # Set LLVM environment variables for cairo-native
    export MLIR_SYS_190_PREFIX=/usr/lib/llvm-19
    export LLVM_SYS_191_PREFIX=/usr/lib/llvm-19
    export TABLEGEN_190_PREFIX=/usr/lib/llvm-19

    echo "🔧 Set LLVM environment variables for cairo-native"

    # Install starknet-native-compile
    echo "📦 Installing starknet-native-compile v${CAIRO_NATIVE_VERSION}..."
    cargo install cairo-native --version "${CAIRO_NATIVE_VERSION}" --bin starknet-native-compile --locked || {
        echo "❌ Failed to install starknet-native-compile v${CAIRO_NATIVE_VERSION}"
        return 1
    }

    echo "✅ Compiler binaries installed successfully"
}

function copy_binaries_to_shared_executables() {
    echo "📁 Copying binaries to shared_executables directory..."

    # Create shared_executables directory
    mkdir -p target/release/shared_executables

    # Determine the correct cargo bin directory
    CARGO_BIN_DIR="${CARGO_HOME:-$HOME/.cargo}/bin"

    # Copy binaries from cargo bin to shared_executables
    cp "$CARGO_BIN_DIR/starknet-sierra-compile" target/release/shared_executables/ || {
        echo "❌ Failed to copy starknet-sierra-compile from $CARGO_BIN_DIR"
        return 1
    }

    cp "$CARGO_BIN_DIR/starknet-native-compile" target/release/shared_executables/ || {
        echo "❌ Failed to copy starknet-native-compile from $CARGO_BIN_DIR"
        return 1
    }

    echo "✅ Binaries copied to target/release/shared_executables/"
    ls -la target/release/shared_executables/
}

function build() {
    ret=0
    echo "🔨 Building native blockifier..."

    # Set up Python environment
    pypy3.9 -m venv /tmp/venv
    source /tmp/venv/bin/activate
    rustup toolchain install

    # Install compiler binaries first
    install_compiler_binaries || ret=$?
    if [ $ret -ne 0 ]; then
        clean
        return $ret
    fi

    # Add cargo bin to PATH so build scripts can find the binaries
    export PATH="$HOME/.cargo/bin:$PATH"

    # Build with cairo_native feature
    cargo build --release -p native_blockifier --features "cairo_native" || ret=$?

    # Copy binaries to expected location for CI artifacts
    if [ $ret -eq 0 ]; then
        copy_binaries_to_shared_executables || ret=$?
    fi

    clean
    return $ret
}

build
