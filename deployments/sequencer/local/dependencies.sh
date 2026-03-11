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

# Install Poetry
pip3 install poetry

# Install cdk8s-cli globally via npm
npm install -g cdk8s-cli@latest

# Install kubectl (binary from official release, download to /tmp)
KUBECTL_VERSION="$(curl -L -s https://dl.k8s.io/release/stable.txt)"
curl -fsSL "https://dl.k8s.io/release/${KUBECTL_VERSION}/bin/linux/amd64/kubectl" -o /tmp/kubectl
curl -fsSL "https://dl.k8s.io/release/${KUBECTL_VERSION}/bin/linux/amd64/kubectl.sha256" -o /tmp/kubectl.sha256
echo "$(cat /tmp/kubectl.sha256)  /tmp/kubectl" | sha256sum --check
install -o root -g root -m 0755 /tmp/kubectl /usr/local/bin/kubectl

# Add Google Cloud CLI apt repo (official docs: newer distributions)
curl -fsSL https://packages.cloud.google.com/apt/doc/apt-key.gpg | gpg --dearmor -o /usr/share/keyrings/cloud.google.gpg
echo "deb [signed-by=/usr/share/keyrings/cloud.google.gpg] https://packages.cloud.google.com/apt cloud-sdk main" | tee /etc/apt/sources.list.d/google-cloud-sdk.list

# Install gosu, sudo, bash-completion, and gcloud CLI (kubectl already installed via binary above)
apt-get update && \
apt-get install -y gosu sudo bash-completion google-cloud-cli google-cloud-cli-gke-gcloud-auth-plugin kubectx && \
rm -rf /var/lib/apt/lists/*
