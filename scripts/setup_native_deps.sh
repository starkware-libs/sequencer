#!/bin/bash

install_essential_deps_linux() {
  apt-get update -y
  apt-get install -y \
    curl \
    jq \
    ripgrep \
    wget \
    ca-certificates \
    gnupg \
    git
}

setup_llvm_deps() {
	case "$(uname)" in
	Darwin)
		brew update
		brew install llvm@18

		LIBRARY_PATH=/opt/homebrew/lib
		MLIR_SYS_180_PREFIX="$(brew --prefix llvm@18)"
		LLVM_SYS_181_PREFIX="$MLIR_SYS_180_PREFIX"
		TABLEGEN_180_PREFIX="$MLIR_SYS_180_PREFIX"

		export LIBRARY_PATH
		export MLIR_SYS_180_PREFIX
		export LLVM_SYS_181_PREFIX
		export TABLEGEN_180_PREFIX
		;;
	Linux)
    export DEBIAN_FRONTEND=noninteractive
    export TZ=America/New_York

		CODENAME=$(grep VERSION_CODENAME /etc/os-release | cut -d= -f2)
		[ -z "$CODENAME" ] && { echo "Error: Unable to determine OS codename"; exit 1; }

		echo "deb http://apt.llvm.org/$CODENAME/ llvm-toolchain-$CODENAME-18 main" > /etc/apt/sources.list.d/llvm-18.list
		echo "deb-src http://apt.llvm.org/$CODENAME/ llvm-toolchain-$CODENAME-18 main" >> /etc/apt/sources.list.d/llvm-18.list
		wget -O - https://apt.llvm.org/llvm-snapshot.gpg.key | apt-key add -

    apt-get update && apt-get upgrade -y
    apt-get install -y zstd
    apt-get install llvm-18 llvm-18-dev llvm-18-runtime clang-18 clang-tools-18 lld-18 libpolly-18-dev libmlir-18-dev mlir-18-tools
    apt-get install -y libgmp3-dev

		MLIR_SYS_180_PREFIX=/usr/lib/llvm-18/
		LLVM_SYS_181_PREFIX=/usr/lib/llvm-18/
		TABLEGEN_180_PREFIX=/usr/lib/llvm-18/

		export MLIR_SYS_180_PREFIX
		export LLVM_SYS_181_PREFIX
		export TABLEGEN_180_PREFIX
		;;
	*)
		echo "Error: Unsupported operating system"
		exit 1
		;;
	esac

	# GitHub Actions specific
	[ -n "$GITHUB_ACTIONS" ] && {
    echo "MLIR_SYS_180_PREFIX=$MLIR_SYS_180_PREFIX" >> $GITHUB_ENV
    echo "LLVM_SYS_181_PREFIX=$LLVM_SYS_181_PREFIX" >> $GITHUB_ENV
    echo "TABLEGEN_180_PREFIX=$TABLEGEN_180_PREFIX" >> $GITHUB_ENV
	}
}

install_rust() {
	if command -v cargo >/dev/null 2>&1; then
		echo "Rust is already installed with cargo available in PATH."
		return 0
	fi

	echo "cargo not found. Installing Rust..."
	if ! curl -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path; then
		echo >&2 "Failed to install Rust. Aborting."
		return 1
	fi

	# shellcheck disable=SC1090
	source "$HOME/.cargo/env" || {
		echo >&2 "Failed to source Rust environment. Aborting."
		return 1
	}

	echo "Rust installed successfully."
}

install_cairo_native_runtime() {
  install_rust || { echo "Error: Failed to install Rust"; exit 1; }

	git clone https://github.com/lambdaclass/cairo_native.git
	pushd ./cairo_native || exit 1
	cargo build -p cairo-native-runtime --release --all-features --quiet
	popd || exit 1

	mv ./cairo_native/target/release/libcairo_native_runtime.a ./libcairo_native_runtime.so
	rm -rf ./cairo_native

	export CAIRO_NATIVE_RUNTIME_LIBRARY="$PWD/libcairo_native_runtime.so"

	[ -n "$GITHUB_ACTIONS" ] && echo "CAIRO_NATIVE_RUNTIME_LIBRARY=$CAIRO_NATIVE_RUNTIME_LIBRARY" >> $GITHUB_ENV
}

main() {
  [ "$(uname)" = "Linux" ] && install_essential_deps_linux

	setup_llvm_deps
	install_cairo_native_runtime

	echo "LLVM and Cairo native runtime dependencies installed successfully."
}

main "$@"