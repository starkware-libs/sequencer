#!/bin/env bash

set -e

function install_pypy() {
  pushd /opt
  $USE_SUDO bash -c '
  curl -Lo pypy3.9-v7.3.11-linux64.tar.bz2 https://downloads.python.org/pypy/pypy3.9-v7.3.11-linux64.tar.bz2
  tar -xf pypy3.9-v7.3.11-linux64.tar.bz2
  rm pypy3.9-v7.3.11-linux64.tar.bz2
  chmod +x pypy3.9-v7.3.11-linux64/bin/pypy3

  if [ -L /usr/local/bin/pypy3.9 ]; then
      unlink /usr/local/bin/pypy3.9
  fi

  ln -s /opt/pypy3.9-v7.3.11-linux64/bin/pypy3 /usr/local/bin/pypy3.9

  if [ -L /opt/pypy3.9 ]; then
      unlink /opt/pypy3.9
  fi

  ln -s /opt/pypy3.9-v7.3.11-linux64 /opt/pypy3.9
  pypy3.9 -m ensurepip
  pypy3.9 -m pip install wheel
  '
  popd
}

function install_rust () {
    curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path
}

function install_llvm() {
  apt update -y && apt install -y \
      wget \
      gnupg
  echo "deb http://apt.llvm.org/focal/ llvm-toolchain-focal-18 main" > /etc/apt/sources.list.d/llvm-18.list
  echo "deb-src http://apt.llvm.org/focal/ llvm-toolchain-focal-18 main" >> /etc/apt/sources.list.d/llvm-18.list
  wget -O - https://apt.llvm.org/llvm-snapshot.gpg.key | apt-key add -

  apt update -y && apt upgrade -y
  apt install -y zstd
  apt install -y llvm-18 llvm-18-dev llvm-18-runtime clang-18 clang-tools-18 lld-18 libpolly-18-dev libmlir-18-dev mlir-18-tools
  apt install -y libgmp3-dev
}

install_llvm

install_pypy &
install_rust &
wait
