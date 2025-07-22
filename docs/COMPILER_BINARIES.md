# Compiler Binary Management

This document explains how Cairo compiler binaries are managed in this project.

## Overview

The project uses two Cairo compiler binaries:
- **`starknet-sierra-compile`** v2.12.0-dev.1 - for Sierra → CASM compilation
- **`starknet-native-compile`** v0.5.0-rc.6 - for Sierra → Native compilation

## System Requirements

### LLVM Dependencies

The `starknet-native-compile` binary requires **LLVM 19** to be installed with specific environment variables:

```bash
# Required environment variables
export MLIR_SYS_190_PREFIX=/usr/lib/llvm-19
export LLVM_SYS_191_PREFIX=/usr/lib/llvm-19
export TABLEGEN_190_PREFIX=/usr/lib/llvm-19
```

#### Installing LLVM 19

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install llvm-19-dev libmlir-19-dev
```

**Or use the project dependencies script:**
```bash
sudo ./scripts/dependencies.sh
```

For complete cairo-native requirements, see the [official documentation](https://github.com/lambdaclass/cairo_native/blob/main/README.md).

## Build Script Behavior

### Validation Only Approach

Build scripts in `apollo_compile_to_casm` and `apollo_compile_to_native` now **validate** that required binaries exist instead of installing them. This provides:

- ✅ **Faster builds** - no network dependencies during build
- ✅ **Clear error messages** - tells you exactly what to install
- ✅ **Offline builds** - works without internet access
- ✅ **CI flexibility** - different installation strategies per environment

### Error Messages

If a required binary is missing, you'll see:

```
❌ ERROR: Required binary 'starknet-native-compile' version '0.5.0-rc.6' not found!

🔧 To install it:
  1. First ensure LLVM 19 is installed:
     sudo apt update && sudo apt install llvm-19-dev libmlir-19-dev
     # OR run: sudo ./scripts/dependencies.sh

  2. Set required environment variables:
     export MLIR_SYS_190_PREFIX=/usr/lib/llvm-19
     export LLVM_SYS_191_PREFIX=/usr/lib/llvm-19
     export TABLEGEN_190_PREFIX=/usr/lib/llvm-19

  3. Install the binary:
     cargo install cairo-native --version 0.5.0-rc.6 --bin starknet-native-compile --locked

📋 Or use the installation script:
   ./scripts/install_compilers.sh
```

## Installation Methods

### For Developers

```bash
# Install both binaries automatically (handles LLVM setup)
./scripts/install_compilers.sh
```

The script will:
- ✅ Detect LLVM 19 installation
- ✅ Set required environment variables
- ✅ Install both compiler binaries
- ✅ Provide helpful error messages if dependencies are missing

### For CI Workflows

Different CI workflows use different installation strategies:

#### 1. blockifier_ci.yml
```yaml
- name: Install compiler binaries
  run: ./scripts/install_compilers.sh
```

#### 2. upload_artifacts_workflow.yml
Uses `build_native_blockifier.sh` which installs binaries and places them in `shared_executables/`.

#### 3. sequencer_docker-test.yml
The docker build process installs binaries during image build in the Dockerfile.

## Docker Integration

### Sequencer Dockerfile

The sequencer dockerfile (`deployments/images/sequencer/Dockerfile`) installs binaries during build with LLVM support:

```dockerfile
# Install required compiler binaries before building
RUN echo "📦 Installing compiler binaries..." && \
    CAIRO_VERSION=$(grep 'CAIRO1_COMPILER_VERSION.*=' crates/apollo_infra_utils/src/cairo_compiler_version.rs | sed 's/.*"\(.*\)".*/\1/') && \
    CAIRO_NATIVE_VERSION=$(grep 'REQUIRED_CAIRO_NATIVE_VERSION.*=' crates/apollo_compile_to_native/src/constants.rs | sed 's/.*"\(.*\)".*/\1/') && \
    cargo install starknet-sierra-compile --version "$CAIRO_VERSION" --locked && \
    export MLIR_SYS_190_PREFIX=/usr/lib/llvm-19 && \
    export LLVM_SYS_191_PREFIX=/usr/lib/llvm-19 && \
    export TABLEGEN_190_PREFIX=/usr/lib/llvm-19 && \
    cargo install cairo-native --version "$CAIRO_NATIVE_VERSION" --bin starknet-native-compile --locked
```

This ensures the binaries are available at `shared_executables/` path where the final stage expects them.

## Version Management

Versions are defined in Rust constants:

- **Cairo Compiler**: `crates/apollo_infra_utils/src/cairo_compiler_version.rs`
- **Cairo Native**: `crates/apollo_compile_to_native/src/constants.rs`

All scripts automatically extract these versions, ensuring consistency.

## Troubleshooting

### Build Fails with "binary not found"
1. Run `./scripts/install_compilers.sh`
2. Ensure `~/.cargo/bin` is in your PATH
3. Verify versions with `starknet-sierra-compile --version`

### LLVM Related Errors
```
failed to find correct version (19.x.x) of llvm-config
```

**Solution:**
1. Install LLVM 19: `sudo apt install llvm-19-dev libmlir-19-dev`
2. Set environment variables (see System Requirements above)
3. Re-run installation: `./scripts/install_compilers.sh`

### Docker Build Fails
- Check that system dependencies are installed (LLVM, GMP)
- Ensure network access for `cargo install` during docker build
- Verify LLVM environment variables are set correctly

### CI Fails with Binary Errors
- Verify the workflow includes binary installation step
- Check that bootstrap actions install system dependencies
- Ensure PATH includes `~/.cargo/bin`
- Verify LLVM 19 is installed and environment variables are set

## Migration Notes

This change converts from "install during build" to "validate during build" approach. Key changes:

1. **Build scripts**: Now validate instead of install
2. **CI workflows**: Explicitly install binaries where needed
3. **Docker builds**: Install binaries during docker build process
4. **Error handling**: Clear error messages guide installation
5. **LLVM support**: Proper environment variable setup for cairo-native
