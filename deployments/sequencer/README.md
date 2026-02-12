# Sequencer CDK8S Deployment Configuration Guide

CDK8s is an open-source framework for defining Kubernetes applications using programming languages and object-oriented APIs. CDK8s apps synthesize into standard Kubernetes manifests that can be applied to any Kubernetes cluster.

**Official documentation:** [https://cdk8s.io/docs/latest](https://cdk8s.io/docs/latest)

## Table of contents

- [Project structure](#project-structure)
- [Manual Setup](#manual-setup)
- [Containerized Environment](#containerized-environment)
- [Usage examples](#usage-examples)
- [Configuration reference](#configuration-reference)
- [Additional resources](#additional-resources)

---

## Project structure

```
deployments/sequencer/
├── configs/                    # Deployment configuration
│   ├── layouts/                # Base layouts (consolidated, hybrid, distributed)
│   │   ├── consolidated/
│   │   │   └── services/
│   │   └── hybrid/
│   │       ├── common.yaml
│   │       └── services/       # core, gateway, committer, l1, mempool, sierra-compiler
│   └── overlays/               # Environment overrides (e.g. hybrid/testing/node-0)
│       └── hybrid/
│           └── testing/
├── docs/                       # Per-resource configuration docs (ConfigMap, Ingress, etc.)
│   ├── README.md               # Configuration index and quick reference
│   └── LAYOUT_OVERLAY_CONFIGURATION.md
├── local/                      # Docker environment (Dockerfile, run-in-docker.sh)
├── resources/crds/             # Custom resource definitions (GCP, Grafana, etc.)
├── src/                        # CDK8s app (charts, constructs, config loaders)
├── main.py                     # Entry point for cdk8s synth
├── cdk8s.yaml
├── Pipfile
└── dist/                       # Generated manifests (created by cdk8s synth)
```

---

## Manual Setup

### Requirements

You can install the required tools however you prefer; the instructions below are recommended.

**Note:** The following instructions have been tested on clean **Ubuntu 22.04** and **Ubuntu 24.04** environments.

#### Required tools

- Curl
- Python 3.10
- Pipenv (Python package manager)
- Node.js + npm
- cdk8s-cli
- kubectl

### Setup instructions

**Note:** You can skip these steps if you already have the required tools installed on your system.

#### 1. Install basic tools

```shell
sudo apt update
sudo apt install curl software-properties-common
```

#### 2. Install Python

```shell
## Ubuntu 22.04 ##
sudo apt install python3.10-full python3.10-venv

## Ubuntu 24.04 ##
sudo add-apt-repository ppa:deadsnakes/ppa
sudo apt update
sudo apt install python3.10-full python3.10-venv
```

#### 3. Install Pipenv

```shell
## Ubuntu 22.04 ##
pip install --user pipenv
# Add this to your ~/.bashrc or ~/.zshrc
export PATH="$HOME/.local/bin:$PATH"
source ~/.bashrc

## Ubuntu 24.04 ##
sudo apt install pipenv

# verify
pipenv --version
```

#### 4. Install Node

```shell
# Install NVM
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash

source ~/.bashrc # If you are using bash
source ~/.zshrc # If you are using zsh

# Install node
nvm install 22.14.0
nvm use 22.14.0

# Verify
node --version
npm --version
```

#### 5. Install cdk8s

```shell
# Install cdk8s
npm install -g cdk8s-cli@2.204.5

# Verify
cdk8s --version
```

#### 6. Install kubectl

```shell
# See official docs:
# https://kubernetes.io/docs/tasks/tools/install-kubectl-linux/

# Download
curl -LO "https://dl.k8s.io/release/$(curl -L -s https://dl.k8s.io/release/stable.txt)/bin/linux/amd64/kubectl"

# Install
sudo install -o root -g root -m 0755 kubectl /usr/local/bin/kubectl

# Verify
kubectl version --client
```

### Project Setup (Manual)

#### 1. Clone sequencer repo

```shell
git clone https://github.com/starkware-libs/sequencer.git
```

#### 2. Init cdk8s project

```shell
cd deployments/sequencer
pipenv install
cdk8s import
```

#### 3. Generate K8s manifests

**Params:**

- **Namespace:** The Kubernetes namespace you want to deploy to
- **Layout:** The Sequencer deployment config layout (consolidated, hybrid, distributed) – similar to helm values.yaml
- **Overlay:** The Sequencer deployment config overlay – overrides the layout configuration (like in kustomize or helm values-dev.yaml)
- **Image** (Optional): Overrides the sequencer Docker image. If not provided the image in the deployment config will be used

```shell
cdk8s synth --app "pipenv run python -m main --namespace <namespace> -l <layout> -o <overlay> --image <image:tag>"
```

Or use our testing overlay:

```shell
cdk8s synth --app "pipenv run python -m main --namespace <namespace> -l hybrid -o hybrid.testing.node-0"
```

The Kubernetes manifests will be generated to `./dist` by default. You can change the default output directory. See the available options by running: `cdk8s synth --help`

#### 4. Deploy

```shell
# Create namespace
kubectl create namespace <namespace>

# Deploy Sequencer
kubectl apply -R -f ./dist
```

---

## Containerized Environment

### Requirements

You can install the required tools however you prefer; the instructions below are recommended.

**Note:** The following instructions have been tested on clean **Ubuntu 22.04** and **Ubuntu 24.04** environments.

#### Required tools

- Docker

### Setup instructions

**Note:** You can skip these steps if you already have the required tools installed on your system.

#### 1. Install Latest Docker

```shell
# The following instructions are taken from official Docker docs.
# Visit: https://docs.docker.com/engine/install/ubuntu/

sudo apt remove $(dpkg --get-selections docker.io docker-compose docker-compose-v2 docker-doc podman-docker containerd runc | cut -f1)

# Add Docker's official GPG key
sudo apt update
sudo apt install ca-certificates curl
sudo install -m 0755 -d /etc/apt/keyrings
sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
sudo chmod a+r /etc/apt/keyrings/docker.asc

# Add the repository to Apt sources:
# == Copy the entire block ==
sudo tee /etc/apt/sources.list.d/docker.sources <<EOF
Types: deb
URIs: https://download.docker.com/linux/ubuntu
Suites: $(. /etc/os-release && echo "${UBUNTU_CODENAME:-$VERSION_CODENAME}")
Components: stable
Signed-By: /etc/apt/keyrings/docker.asc
EOF
# == Copy the entire block ==

# Install Docker
sudo apt update
sudo apt install docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin

# Verify
docker --version
```

### Project Setup (Docker)

**Duration:** About 5–10 minutes

#### Pre-installed tools

- Python 3.10 (with venv and pip)
- Node.js 22.x (and npm)
- pipenv (Python dependency manager)
- cdk8s-cli (CDK8S CLI, installed globally via npm)
- kubectl (Kubernetes CLI, official binary)
- Google Cloud CLI (gcloud) (and GKE auth plugin if present in the image)
- kubectx (if installed in the image; for switching Kubernetes contexts)
- gosu, sudo, bash-completion (for running as your user and shell helpers)

The Docker setup automates the steps from the manual guide. Instead of installing Python, Node, pipenv, and cdk8s-cli on your machine, you run a single script that:

- Builds a Docker image based on Ubuntu 24.04 with all required tools
- Starts a container with your repo mounted at `/workspace`, so edits on your host are visible in the container and files created in the container appear on your host
- Runs as your host user (same UID/GID) so permissions and ownership match your machine
- Starts an interactive shell in `/workspace/deployments/sequencer`; the entrypoint runs project setup (e.g. `pipenv install`, `cdk8s import`) on first run if needed
- Future runs are cached by Docker — after the first build, later runs reuse the image and start the container quickly; you only pay the build cost when the image or Dockerfile changes

#### Mounting your kubeconfig

Your host `~/.kube` is mounted into the container read-only at `~/.kube-tmp`, so the host directory is never modified. When the container starts, the entrypoint copies that read-only mount into a writable `~/.kube` inside the container. As a result:

- kubectl and kubectx (and similar tools) use your real kubeconfig and have full read/write access inside the container (e.g. switching context, updating credentials)
- Your host `~/.kube` is never written to; only the copy inside the container is changed
- Each time you start the container you get a fresh copy from the host, so any changes you made on the host (new contexts, clusters, credentials) are picked up automatically. Changes you make inside the container (e.g. with kubectx) apply only to the container's copy and do not affect the host

For reference, the script effectively runs:

```shell
# docker run -it --rm \
   -v "${REPO_ROOT}:/workspace" \
   -v "${KUBE_CONFIG_DIR}:/home/${USER_NAME}/.kube-tmp:ro" \
   -w /workspace/deployments/sequencer \
   -e "USER_ID=${USER_ID}" \
   -e "GROUP_ID=${GROUP_ID}" \
   -e "USER_NAME=${USER_NAME}" \
   "${FULL_IMAGE_NAME}" \
   /bin/bash -l
```

#### 1. Clone sequencer repo

```shell
git clone https://github.com/starkware-libs/sequencer.git
cd sequencer
```

#### 2. Execute run-in-docker script

```shell
# Important: make sure current directory is on the Sequencer repository root
./deployments/sequencer/local/run-in-docker.sh
```

#### 3. Generate K8s manifests

**Params:**

- **Namespace:** The Kubernetes namespace you want to deploy to
- **Layout:** The Sequencer deployment config layout (consolidated, hybrid, distributed) – similar to helm values.yaml
- **Overlay:** The Sequencer deployment config overlay – overrides the layout configuration (like in kustomize or helm values-dev.yaml)
- **Image** (Optional): Overrides the sequencer Docker image. If not provided the image in the deployment config will be used

```shell
cdk8s synth --app "pipenv run python -m main --namespace <namespace> -l <layout> -o <overlay> --image <image:tag>"
```

Or use our testing overlay:

```shell
cdk8s synth --app "pipenv run python -m main --namespace <namespace> -l hybrid -o hybrid.testing.node-0"
```

The Kubernetes manifests will be generated to `./dist` by default. You can change the default output directory. See the available options by running: `cdk8s synth --help`

#### 4. Deploy

```shell
# Create namespace
kubectl create namespace <namespace>

# Deploy Sequencer
kubectl apply -R -f ./dist
```

---

## Usage examples

All commands assume you are in `deployments/sequencer` (or inside the Docker container at `/workspace/deployments/sequencer`).

**Consolidated layout (default, no overlay):**

```shell
cdk8s synth --app "pipenv run python -m main --namespace my-namespace"
```

**Hybrid layout with testing overlay:**

```shell
cdk8s synth --app "pipenv run python -m main --namespace sequencer-dev -l hybrid -o hybrid.testing.node-0"
```

**Hybrid with overlay and custom image:**

```shell
cdk8s synth --app "pipenv run python -m main --namespace prod -l hybrid -o hybrid.testing.all-constructs --image my-registry/sequencer:v1.2.3"
```

**Dry-run before deploying:**

```shell
kubectl apply -R -f ./dist --dry-run=client
```

**Apply to a specific namespace:**

```shell
kubectl apply -R -f ./dist -n <namespace>
```

---

## Configuration reference

- **[docs/README.md](docs/README.md)** — Configuration index: layout/overlay system, per-resource docs (ConfigMap, Secret, Ingress, StatefulSet, etc.), quick-reference YAML snippets, and common config (`common.yaml`).
- **[docs/LAYOUT_OVERLAY_CONFIGURATION.md](docs/LAYOUT_OVERLAY_CONFIGURATION.md)** — How layouts and overlays work and how to add new environments.

Layouts: `consolidated`, `hybrid`, `distributed`. Overlay names use dot notation and must start with the layout name (e.g. `hybrid.testing.node-0`).

---

## Additional resources

- [Sequencer CDK8s project documentation](docs/README.md)
- [CDK8s official documentation](https://cdk8s.io/docs/latest/)
- [Kubernetes official documentation](https://kubernetes.io/docs/setup/)
- [Docker official documentation](https://docs.docker.com/get-started/)
