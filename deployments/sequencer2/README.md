# CDK8S Deployment Configuration

cdk8s is an open-source software development framework for defining Kubernetes applications and reusable abstractions using familiar programming languages and rich object-oriented APIs. cdk8s apps synthesize into standard Kubernetes manifests that can be applied to any Kubernetes cluster.

- Official documentation: https://cdk8s.io/docs/latest/

## Requirements

**Note**: All installation instructions below are optional. You can use any method you prefer to install the required tools.

### Required Tools

1. **Python** (>= 3.10)
   - Supports Python 3.10, 3.11, 3.12, and future versions
2. **pipenv** (Python package manager)
3. **Node.js** + **npm**
4. **cdk8s-cli**

## Setup Instructions

### 1. Python Setup

The project requires Python 3.10 or higher (supports Python 3.10, 3.11, 3.12, etc.).

#### For Ubuntu 22.04 users:
```bash
sudo apt update
sudo apt install python3.10-full python3.10-venv python3-pip
```

#### For Ubuntu 24.04 users:
```bash
sudo add-apt-repository ppa:deadsnakes/ppa
sudo apt update
sudo apt install python3.10-full python3.10-venv python3-pip
# Or for Python 3.11/3.12:
# sudo apt install python3.11-full python3.11-venv
# sudo apt install python3.12-full python3.12-venv
```

#### Using pyenv (recommended for managing multiple Python versions):
```bash
# Install pyenv
curl https://pyenv.run | bash

# Install Python 3.12 (or any version >= 3.10)
pyenv install 3.12.0
pyenv local 3.12.0  # Set for this project
```

### 2. Install pipenv

```bash
# Using pip (recommended)
pip install --user pipenv

# Or using apt (Ubuntu/Debian)
sudo apt install pipenv

# Verify installation
pipenv --version
```

### 3. Node.js Setup

#### Using nvm (recommended):
```bash
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash

# For bash users
source ~/.bashrc

# For zsh users
source ~/.zshrc

# Install and use Node.js
nvm install 22.14.0
nvm use 22.14.0

# Verify installation
node --version
npm --version
```

#### Alternative: Install Node.js directly
```bash
# Ubuntu/Debian
curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash -
sudo apt-get install -y nodejs
```

### 4. Install cdk8s-cli

```bash
npm install -g cdk8s-cli@latest

# Verify installation
cdk8s --version
```

## Project Setup

### Initial Setup (one-time)

1. **Navigate to the project directory:**
   ```bash
   cd deployments/sequencer2
   ```

2. **Install Python dependencies:**
   ```bash
   pipenv install
   ```
   This will:
   - Create a virtual environment (if it doesn't exist)
   - Install all dependencies from `Pipfile`
   - Generate/update `Pipfile.lock`

3. **Initialize cdk8s imports:**
   ```bash
   cdk8s import
   ```
   This generates the Kubernetes API bindings in the `imports/` directory.

## Usage

### Generate Kubernetes Manifests

Synthesize Kubernetes manifests from your configuration:

```bash
# Basic usage
cdk8s synth --app "pipenv run python -m main --namespace <namespace>"

# Example: Generate manifests for 'production' namespace
cdk8s synth --app "pipenv run python -m main --namespace production"

# Example: Specify custom config paths
cdk8s synth --app "pipenv run python -m main --namespace test --layout consolidated --overlay prod"
```

### Generated Output

The generated Kubernetes manifests are written to the `dist/` directory:
```
dist/
  └── sequencer-<service-name>/
      ├── ConfigMap.*.k8s.yaml
      ├── Deployment.*.k8s.yaml
      ├── Service.*.k8s.yaml
      └── ...
```

### Deploy to Kubernetes

After synthesis, deploy using kubectl:

```bash
# Apply all manifests
kubectl apply -f dist/

# Or apply to specific namespace
kubectl apply -f dist/ -n <namespace>

# Dry-run first (recommended)
kubectl apply --dry-run=client -f dist/
```

## Development

### Running Tests

```bash
# Run with pipenv
pipenv run python test-main.py
```

### Code Formatting

The project uses `black` and `isort` for code formatting:

```bash
# Format code
pipenv run black .
pipenv run isort .

# Check formatting (without changes)
pipenv run black --check .
pipenv run isort --check .
```

### Type Checking

```bash
# Run mypy type checker
pipenv run mypy .
```

## Project Structure

```
deployments/sequencer2/
├── configs/              # Configuration files
│   ├── layouts/          # Base configurations
│   └── overlays/         # Environment-specific overrides
├── dist/                 # Generated Kubernetes manifests (gitignored)
├── docs/                 # Documentation for configuration options
├── imports/              # Generated cdk8s API bindings (gitignored)
├── resources/            # CRD definitions
├── schemas/              # JSON schemas for validation
└── src/
    ├── charts/           # CDK8s Chart definitions
    ├── constructs/       # Kubernetes resource constructs
    ├── config/           # Configuration loading and validation
    └── ...
```

## Configuration

Configuration files are located in `configs/`:
- **Layouts**: Base service configurations (`configs/layouts/`)
- **Overlays**: Environment-specific overrides (`configs/overlays/`)

See `docs/README.md` for detailed configuration documentation for each Kubernetes resource.

## Troubleshooting

### Python Version Issues

If you encounter Python version errors:
```bash
# Check current Python version
python3 --version

# Ensure pipenv uses the correct version
pipenv --python 3.10  # or 3.11, 3.12, etc.
pipenv install
```

### pipenv Lock Issues

If `Pipfile.lock` is out of sync:
```bash
pipenv lock
pipenv install
```

### cdk8s Import Errors

If imports are missing or outdated:
```bash
rm -rf imports/
cdk8s import
```

### Permission Errors

If you encounter permission errors with npm global installs:
```bash
# Configure npm to use a directory you own
mkdir ~/.npm-global
npm config set prefix '~/.npm-global'

# Add to your ~/.bashrc or ~/.zshrc:
export PATH=~/.npm-global/bin:$PATH

# Then reinstall
npm install -g cdk8s-cli
```

## Additional Resources

- [CDK8s Documentation](https://cdk8s.io/docs/latest/)
- [Kubernetes Documentation](https://kubernetes.io/docs/)
- [Pipenv Documentation](https://pipenv.pypa.io/)
