# Local Testing Environment

A complete local Kubernetes development environment for the Starkware Sequencer using k3d.

This environment can be deployed in two ways:
1. **Vagrant VM** (Recommended) - Isolated VM with all prerequisites pre-installed
2. **Direct k3d** - Run k3d directly on your host machine

## Deployment Methods

### Option 1: Vagrant VM (Recommended)

The Vagrant setup provides a fully isolated development environment with all prerequisites pre-installed. This is the recommended approach for consistent, reproducible environments.

#### Vagrant Prerequisites

**On Ubuntu 24.04:**
```bash
# Run the automated installer (installs Vagrant + libvirt)
# This script is idempotent - safe to run multiple times
./install-vagrant.sh
```

The `install-vagrant.sh` script automatically:
- Installs KVM/libvirt and required packages
- Adds your user to `libvirt` and `kvm` groups
- Enables and starts the `libvirtd` service
- Installs Vagrant from HashiCorp repository
- Installs the `vagrant-libvirt` plugin
- Verifies the installation

**Important:** After running `install-vagrant.sh`, you need to log out and log back in (or run `newgrp libvirt`) for group changes to take effect.

**Manual Installation (if needed):**
- **Vagrant** >= 2.4.0 (install from [HashiCorp](https://www.vagrantup.com/downloads))
- **libvirt** (KVM) - For Linux hosts
  ```bash
  sudo apt-get install qemu-kvm libvirt-daemon-system libvirt-clients
  sudo usermod -aG libvirt $USER
  ```
- **vagrant-libvirt plugin**
  ```bash
  vagrant plugin install vagrant-libvirt
  ```
- **VirtualBox** (alternative for macOS/Windows)
  ```bash
  vagrant plugin install vagrant-vbguest
  ```

#### Vagrant Quick Start

```bash
# First time: Install Vagrant + libvirt (Ubuntu)
./install-vagrant.sh

# Start the VM (uses pre-baked box if available, otherwise base image)
vagrant up

# SSH into the VM
vagrant ssh

# Inside the VM, deploy the sequencer stack
cd ~/sequencer/deployments/local-testing
./manifests/deploy.sh up
```

#### Vagrant Features

**Pre-baked Box Support:**
- The Vagrantfile automatically detects and uses a pre-baked box (`sequencer-dev`) if available
- Pre-baked boxes include all prerequisites pre-installed, making `vagrant up` much faster
- To force using the base image: `BOX=cloud-image/ubuntu-24.04 vagrant up`
- To explicitly use pre-baked box: `BOX=sequencer-dev vagrant up`

**Dynamic Resource Allocation:**
- **CPU**: Automatically allocates all host CPUs minus 2 (minimum 4 CPUs)
- **Memory**: 16 GB RAM
- **Disk**: 100 GB (automatically expanded on first boot)

**Port Forwarding:**
All services are accessible from your host machine:
- Grafana: `http://localhost:3000`
- Prometheus: `http://localhost:9090`
- Sequencer HTTP: `http://localhost:8080`
- Docker Registry: `http://localhost:5050`

**File Synchronization:**
- Uses `rsync` for one-way sync from host to VM
- Excludes large directories: `.git/`, `target/`, `node_modules/`, `.venv/`, `output/`, `*.box`
- To re-sync after making changes: `vagrant rsync`

**Pre-installed Prerequisites:**
The VM comes with everything pre-installed:
- Docker (with insecure registry configured for k3d)
- k3d, kubectl, Helm
- Rust toolchain
- Python 3.10, pipenv
- Node.js 20.x, cdk8s-cli
- Anvil (Foundry v0.3.0)
- kubectx, kubens
- All build tools and dependencies

**VM Management:**
```bash
vagrant up          # Start the VM
vagrant halt        # Stop the VM
vagrant destroy     # Delete the VM (destroys all data)
vagrant ssh         # SSH into the VM
vagrant rsync       # Re-sync files from host to VM
vagrant status      # Check VM status
vagrant box list    # List installed Vagrant boxes
```

**Workflow Tips:**
- After making code changes on your host, run `vagrant rsync` to sync them to the VM
- The VM starts in `~/sequencer` directory automatically
- All prerequisites are pre-installed, so you can immediately run `./manifests/deploy.sh up`
- The VM uses bash with helpful aliases (`k=kubectl`, `kctx=kubectx`, `kns=kubens`)

### Option 2: Direct k3d (Host Machine)

Run k3d directly on your host machine. Requires all prerequisites to be installed manually.

#### Direct k3d Prerequisites

- Rust toolchain (for building `sequencer_node_setup` and `sequencer_simulator`)
- Docker >= 24.0
- k3d >= 5.6.0
- kubectl >= 1.28
- Helm >= 3.12
- Python 3.10+ (for Grafana dashboard upload)
- pipenv (install via `pip install pipenv`)
- cdk8s-cli (install via `npm install -g cdk8s-cli`)
- **Anvil** (Foundry v0.3.0) - Required for state generation. The script will attempt to install it automatically if missing, or install manually:
  ```bash
  curl -L https://github.com/foundry-rs/foundry/releases/download/v0.3.0/foundry_v0.3.0_linux_amd64.tar.gz | tar -xz --wildcards 'anvil'
  mv anvil ~/.local/bin/  # or another directory in your PATH
  ```

## Quick Start

### Using Vagrant VM

```bash
# 1. Install Vagrant + libvirt (first time only, Ubuntu)
./install-vagrant.sh

# 2. Start the VM
vagrant up

# 3. SSH into the VM
vagrant ssh

# 4. Inside the VM, deploy everything
cd ~/sequencer/deployments/local-testing
./manifests/deploy.sh up
```

### Using Direct k3d

```bash
# Deploy everything (cluster, binaries, state, images, services, monitoring)
# Note: State is automatically copied to sequencer pod after it becomes ready
./manifests/deploy.sh up

# Tear down everything
./manifests/deploy.sh down

# Just rebuild and push Docker images
./deploy.sh build

# Build Rust binaries locally
./manifests/deploy.sh build-binaries

# Generate initial sequencer state
./manifests/deploy.sh generate-state

# Copy state to sequencer pod and restart (after deploying sequencer)
./manifests/deploy.sh copy-state

# View sequencer logs
./manifests/deploy.sh logs

# Check status
./manifests/deploy.sh status
```

## Architecture

The local testing environment includes:

- **k3d cluster** with local registry (port 5050)
- **Prometheus** (port 9090) - Metrics collection
- **Grafana** (port 3000) - Visualization and dashboards
- **Sequencer Node** - Main sequencer service (deployed via cdk8s with overlay `hybrid.testing.node-0`)
- **Supporting services** - Dummy recorder, dummy ETH-STRK oracle (deployed via cdk8s)
- **Local binaries** - `sequencer_node_setup` and `sequencer_simulator` built and run locally
- **Centralized manifests** - All cdk8s-generated manifests stored in `manifests/` subdirectories

## Access Points

### Grafana
Grafana is exposed via Traefik Ingress (no port-forward needed!):

- **URL**: `http://localhost:3000`
- **Username**: `admin`
- **Password**: `admin`
- Anonymous access is also enabled

**How it works**: k3d maps host port 3000 → Traefik port 80 (Ingress controller) → routes to Grafana service

### Prometheus
Prometheus is exposed via NodePort (no port-forward needed!):

- **URL**: `http://localhost:9090`

**How it works**: k3d maps host port 9090 → Prometheus NodePort 30090 → Prometheus service

### Sequencer Services
- Sequencer HTTP: http://localhost:8080 (requires port-forward)
- Sequencer Monitoring: http://localhost:8082 (requires port-forward)

## Configuration

The deployment uses:
- **Local Rust binaries** - `sequencer_node_setup` and `sequencer_simulator` built with `cargo build`
- **State generation** - `sequencer_node_setup` runs locally to generate initial state data
- Local Docker images built from `deployments/images/sequencer/`
- **cdk8s-generated manifests** stored in `manifests/` subdirectories:
  - `manifests/dummy-recorder/` - Dummy recorder manifests
  - `manifests/dummy-eth2strk-oracle/` - Dummy ETH-STRK oracle manifests
  - `manifests/sequencer/` - Sequencer manifests (using overlay `hybrid.testing.node-0`)
- Helm chart for monitoring stack (kube-prometheus-stack)
- Reuses dashboard builders from `deployments/monitoring/src/`

**Sequencer Overlay**: The sequencer uses overlay `hybrid.testing.node-0` by default. To change this, edit `SEQUENCER_OVERLAY` in `manifests/deploy.sh`.

**Configuration**: Sequencer configuration is handled via cdk8s overlays. Set `recorder_url`, `l1_gas_price_provider_config`, ports, etc. directly in your overlay YAML files at `deployments/sequencer/configs/overlays/hybrid/testing/node-0/`.

**State Management**: 
1. `sequencer_node_setup` runs locally to generate state in `deployments/local-testing/output/data/node_0`
2. `manifests/deploy.sh up` automatically waits for the sequencer pod to be ready and copies state to it
3. If automatic state copy fails or you need to retry, run `./manifests/deploy.sh copy-state` manually
4. The sequencer pod will read the state from its PVC on restart

## Service Deployment

All services are deployed using their existing cdk8s projects. Manifests are generated and stored in `manifests/`:
- `manifests/dummy-recorder/` - Generated from `deployments/dummy_recorder/`
- `manifests/dummy-eth2strk-oracle/` - Generated from `deployments/dummy_eth2strk_oracle/`
- `manifests/sequencer/` - Generated from `deployments/sequencer/` using overlay `hybrid.testing.node-0`
- `sequencer_simulator` - Deploy separately via `deployments/sequencer_simulator/` (cdk8s) if needed

**Sequencer Overlay**: The sequencer uses the overlay `hybrid.testing.node-0` by default. To change this, edit the `SEQUENCER_OVERLAY` variable in `deploy.sh`.

## Troubleshooting

### Vagrant Issues

**VM won't start:**
- Ensure libvirt/KVM is installed and running: `systemctl status libvirtd`
- Check user is in libvirt group: `groups | grep libvirt`
- If not, run `newgrp libvirt` or log out/in after running `install-vagrant.sh`
- For VirtualBox users: Ensure VirtualBox is installed and running

**Pre-baked box not found:**
- Pre-baked boxes are optional - Vagrant will use the base Ubuntu image if not available
- To check installed boxes: `vagrant box list`
- To use base image explicitly: `BOX=cloud-image/ubuntu-24.04 vagrant up`

**File sync issues:**
- Run `vagrant rsync` to manually sync files from host to VM
- Check rsync exclusions in Vagrantfile if files are missing
- Large files (target/, node_modules/, etc.) are excluded by default

**Port forwarding not working:**
- Ensure ports 3000, 9090, 8080, 5050 are not in use on host
- Check VM is running: `vagrant status`
- Verify port forwarding: `vagrant port`

**VM out of disk space:**
- The VM automatically expands disk on first boot
- If issues persist, check disk: `vagrant ssh` then `df -h`
- Increase disk size in Vagrantfile: `libvirt.machine_virtual_size = 200`

### Cluster creation fails
- Ensure Docker is running
- Check k3d version: `k3d version`
- Try deleting existing cluster: `k3d cluster delete sequencer-local`

### Images not found
- Run `./manifests/deploy.sh build` to rebuild and push images
- Check registry: `docker ps | grep sequencer-registry`

### Monitoring installation timeout
- The kube-prometheus-stack chart is large and may take 10-15 minutes to install
- If installation times out, you can retry: `./manifests/deploy.sh install-monitoring`
- Check installation progress: `kubectl get pods -n sequencer | grep prometheus`
- The deployment will continue even if monitoring installation fails

### Pods not starting
- Check pod status: `kubectl -n sequencer get pods`
- View logs: `kubectl -n sequencer logs <pod-name>`
- Ensure state was generated: Check that `deployments/local-testing/output/data/node_0` exists
- Ensure state was copied: Run `./manifests/deploy.sh copy-state` after deploying sequencer

### State issues
- State is generated locally by running `sequencer_node_setup` binary
- Generated state is in `deployments/local-testing/output/data/node_0`
- Copy state to pod: `./manifests/deploy.sh copy-state` (this also restarts the pod)
- Verify state in pod: `kubectl exec -n sequencer <pod-name> -- ls -la /data`

### Config issues
- Config is customized via cdk8s overlays
- Check your overlay YAML for correct service URLs (recorder_url, oracle URLs, etc.)
- Service names in k8s: `dummy-recorder`, `dummy-eth2strk-oracle` (use these in overlay config)

