#!/bin/bash

pushd /tmp

sudo apt update && sudo apt install -y \
  ca-certificates \
    curl \
    git \
    gnupg \
    jq \
    libssl-dev \
    lsb-release \
    pkg-config \
    ripgrep \
    software-properties-common \
    zstd \
    wget

sudo curl https://apt.llvm.org/llvm.sh -Lo llvm.sh
sudo bash ./llvm.sh 18 all
sudo apt update && sudo apt install -y \
    libgmp3-dev \
    libmlir-18-dev \
    libpolly-18-dev \
    libzstd-dev \
    mlir-18-tools

popd
