# Docker Setup for CDK8S Deployment

This directory contains Docker configuration to run the CDK8S deployment setup in a container.

## Quick Start

### Using the Wrapper Script (Recommended)

The easiest way to build and run the container is using the provided wrapper script:

```bash
cd deployments/sequencer
./run-docker.sh
```

This script will:
- Build the Docker image automatically
- Run the container with your host user's UID/GID (not root)
- Mount your current directory to `/workspace`
- Mount your kubectl config (read-only)
- Start an interactive bash session

### Manual Docker Commands

If you prefer to run Docker commands manually:

```bash
# Build the image
cd deployments/sequencer
docker build -t cdk8s-sequencer:latest .

# Run the container
docker run -it --rm \
  -u $(id -u):$(id -g) \
  -v "$(pwd):/workspace" \
  -v "$HOME/.kube:/home/$(id -un)/.kube:ro" \
  -w /workspace \
  cdk8s-sequencer:latest
```

## What's Included

The Docker image includes:
- **Python 3.10+** (installed via setup script)
- **Node.js 22.x** (installed via setup script)
- **pipenv** (Python package manager)
- **cdk8s-cli** (CDK8S command-line tool)
- **kubectl** (Kubernetes CLI)
- **All project dependencies** (installed via `pipenv install`)
- **CDK8S imports** (initialized via `cdk8s import`)

## Usage

Once inside the container, you can use all the tools:

```bash
# Check installed tools
python3 --version
node --version
pipenv --version
cdk8s --version
kubectl version --client

# Setup the project (first time only, or after pulling new changes)
./local/setup.sh

# Generate Kubernetes manifests
cdk8s synth --app "pipenv run python -m main --namespace test"

# Install additional Python packages
pipenv install <package-name>

# Run Python scripts
pipenv run python -m main --namespace test --layout hybrid
```

## Development Workflow

The wrapper script automatically mounts your current directory to `/workspace`, so:
- Edit files on your host machine
- See changes immediately in the container
- Your local `.venv` and `imports/` directories are preserved
- File permissions match your host user (not root)

## Notes

- The container runs as your host user (same UID/GID), not root
- All tools are installed system-wide during build
- The working directory is `/workspace` (mounted from your current directory)
- kubectl config is mounted read-only from `~/.kube`
- The project setup (`pipenv install` and `cdk8s import`) runs automatically when you first enter the container
