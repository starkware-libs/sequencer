#!/usr/bin/env bash
set -euo pipefail

# Dependencies installation script for CDK8S Deployment
# This script installs all system dependencies needed for the project

# Install system dependencies
apt-get update && \
apt-get install -y \
    lsb-release \
    curl \
    ca-certificates \
    software-properties-common \
    apt-transport-https \
    gnupg \
    && rm -rf /var/lib/apt/lists/*

# Install Python 3.10
# Ubuntu 24.04 needs deadsnakes PPA, Ubuntu 22.04 has it in default repos
if [ "$(lsb_release -rs)" = "24.04" ]; then
    add-apt-repository -y ppa:deadsnakes/ppa
    apt-get update
fi

apt-get install -y \
    python3.10-full \
    python3.10-venv \
    python3-pip \
    && rm -rf /var/lib/apt/lists/*

# Configure python3 to use Python 3.10
update-alternatives --install /usr/bin/python3 python3 /usr/bin/python3.10 1
update-alternatives --set python3 /usr/bin/python3.10

# Install Node.js 22.x from NodeSource
curl -fsSL https://deb.nodesource.com/setup_22.x | bash -
apt-get install -y nodejs && \
rm -rf /var/lib/apt/lists/*

# Install pipenv
# Try apt first, then fall back to pip
# If global pip install fails, install with --user and make it available system-wide
apt-get update && \
(apt-get install -y python3-pipenv 2>/dev/null || \
 (pip3 install pipenv 2>/dev/null || \
  (pip3 install --user pipenv && \
   # Copy binary to system location
   cp /root/.local/bin/pipenv /usr/local/bin/pipenv 2>/dev/null || true && \
   # Find and copy Python modules to system location so pipenv can find them
   PYTHON_VERSION=$(python3 -c "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')") && \
   if [ -d "/root/.local/lib/python${PYTHON_VERSION}/site-packages" ]; then \
       mkdir -p "/usr/local/lib/python${PYTHON_VERSION}/dist-packages" && \
       cp -r /root/.local/lib/python${PYTHON_VERSION}/site-packages/pipenv* \
             /usr/local/lib/python${PYTHON_VERSION}/dist-packages/ 2>/dev/null || true; \
   fi))) && \
rm -rf /var/lib/apt/lists/*

# Install cdk8s-cli globally via npm
npm install -g cdk8s-cli@latest

# Install gosu, sudo, and bash-completion for container runtime
apt-get update && \
apt-get install -y gosu sudo bash-completion && \
rm -rf /var/lib/apt/lists/*
