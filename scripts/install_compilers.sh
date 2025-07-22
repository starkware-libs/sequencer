#!/bin/bash
set -euo pipefail

# Extract version constants from Rust source files
CAIRO_VERSION=$(grep 'CAIRO1_COMPILER_VERSION.*=' crates/apollo_infra_utils/src/cairo_compiler_version.rs | sed 's/.*"\(.*\)".*/\1/')
CAIRO_NATIVE_VERSION=$(grep 'REQUIRED_CAIRO_NATIVE_VERSION.*=' crates/apollo_compile_to_native/src/constants.rs | sed 's/.*"\(.*\)".*/\1/')

echo "📦 Installing Cairo Compiler Binaries"
echo "  - Cairo Compiler: ${CAIRO_VERSION}"
echo "  - Cairo Native: ${CAIRO_NATIVE_VERSION}"

# Function to check if binary exists and has correct version
check_binary() {
    local binary_name="$1"
    local expected_version="$2"

    if command -v "$binary_name" &> /dev/null; then
        if "$binary_name" --version 2>/dev/null | grep -q "$expected_version"; then
            echo "✅ $binary_name v$expected_version already installed"
            return 0
        else
            echo "⚠️  $binary_name found but wrong version, reinstalling..."
        fi
    else
        echo "🔍 $binary_name not found, installing..."
    fi
    return 1
}

# Function to check and setup LLVM dependencies for cairo-native
setup_llvm_for_cairo_native() {
    echo "🔍 Checking LLVM dependencies for cairo-native..."

    # Check if LLVM 19 is available
    local llvm_path=""

    # Common LLVM 19 installation paths
    for path in /usr/lib/llvm-19 /usr/local/lib/llvm-19 /opt/llvm-19; do
        if [ -d "$path" ] && [ -f "$path/bin/llvm-config" ]; then
            llvm_path="$path"
            break
        fi
    done

    # If not found in standard locations, try to find llvm-config-19
    if [ -z "$llvm_path" ] && command -v llvm-config-19 &> /dev/null; then
        llvm_path=$(dirname $(dirname $(which llvm-config-19)))
    fi

    if [ -z "$llvm_path" ]; then
        echo "❌ ERROR: LLVM 19 not found!"
        echo ""
        echo "cairo-native requires LLVM 19. Please install it:"
        echo "  - On Ubuntu/Debian:"
        echo "    sudo apt update"
        echo "    sudo apt install llvm-19-dev libmlir-19-dev"
        echo ""
        echo "  - Or run the dependencies script:"
        echo "    sudo ./scripts/dependencies.sh"
        echo ""
        echo "For more information, see: https://github.com/lambdaclass/cairo_native/blob/main/README.md"
        return 1
    fi

    echo "✅ Found LLVM 19 at: $llvm_path"

    # Set required environment variables for cairo-native compilation
    export MLIR_SYS_190_PREFIX="$llvm_path"
    export LLVM_SYS_191_PREFIX="$llvm_path"
    export TABLEGEN_190_PREFIX="$llvm_path"

    echo "🔧 Set LLVM environment variables:"
    echo "  MLIR_SYS_190_PREFIX=$MLIR_SYS_190_PREFIX"
    echo "  LLVM_SYS_191_PREFIX=$LLVM_SYS_191_PREFIX"
    echo "  TABLEGEN_190_PREFIX=$TABLEGEN_190_PREFIX"

    return 0
}

# Install starknet-sierra-compile
if ! check_binary "starknet-sierra-compile" "$CAIRO_VERSION"; then
    echo "📦 Installing starknet-sierra-compile v${CAIRO_VERSION}..."
    cargo install starknet-sierra-compile --version "$CAIRO_VERSION" --locked
    echo "✅ starknet-sierra-compile v$CAIRO_VERSION installed"
fi

# Install starknet-native-compile (requires LLVM setup)
if ! check_binary "starknet-native-compile" "$CAIRO_NATIVE_VERSION"; then
    echo "📦 Installing starknet-native-compile v${CAIRO_NATIVE_VERSION}..."

    # Setup LLVM dependencies before attempting installation
    if ! setup_llvm_for_cairo_native; then
        echo "❌ Failed to setup LLVM dependencies for cairo-native"
        exit 1
    fi

    echo "📦 Installing cairo-native with LLVM support..."
    cargo install cairo-native --version "$CAIRO_NATIVE_VERSION" --bin starknet-native-compile --locked
    echo "✅ starknet-native-compile v$CAIRO_NATIVE_VERSION installed"
fi

echo "🎉 All compiler binaries are ready!"

# Verify installations
echo "🔍 Verifying installations:"
starknet-sierra-compile --version
starknet-native-compile --version
