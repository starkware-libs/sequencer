# Local Testing Environment (k3d)

A complete local Kubernetes development environment for the Starkware Sequencer using k3d.

## Prerequisites

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

```bash
# Deploy everything (cluster, binaries, state, images, services, monitoring)
# Note: State is automatically copied to sequencer pod after it becomes ready
./deploy.sh up

# Tear down everything
./deploy.sh down

# Just rebuild and push Docker images
./deploy.sh build

# Build Rust binaries locally
./deploy.sh build-binaries

# Generate initial sequencer state
./deploy.sh generate-state

# Copy state to sequencer pod and restart (after deploying sequencer)
./deploy.sh copy-state

# View sequencer logs
./deploy.sh logs

# Check status
./deploy.sh status
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

**Sequencer Overlay**: The sequencer uses overlay `hybrid.testing.node-0` by default. To change this, edit `SEQUENCER_OVERLAY` in `deploy.sh`.

**Configuration**: Sequencer configuration is handled via cdk8s overlays. Set `recorder_url`, `l1_gas_price_provider_config`, ports, etc. directly in your overlay YAML files at `deployments/sequencer/configs/overlays/hybrid/testing/node-0/`.

**State Management**: 
1. `sequencer_node_setup` runs locally to generate state in `deployments/local-testing/output/data/node_0`
2. `deploy.sh up` automatically waits for the sequencer pod to be ready and copies state to it
3. If automatic state copy fails or you need to retry, run `./deploy.sh copy-state` manually
4. The sequencer pod will read the state from its PVC on restart

## Service Deployment

All services are deployed using their existing cdk8s projects. Manifests are generated and stored in `manifests/`:
- `manifests/dummy-recorder/` - Generated from `deployments/dummy_recorder/`
- `manifests/dummy-eth2strk-oracle/` - Generated from `deployments/dummy_eth2strk_oracle/`
- `manifests/sequencer/` - Generated from `deployments/sequencer/` using overlay `hybrid.testing.node-0`
- `sequencer_simulator` - Deploy separately via `deployments/sequencer_simulator/` (cdk8s) if needed

**Sequencer Overlay**: The sequencer uses the overlay `hybrid.testing.node-0` by default. To change this, edit the `SEQUENCER_OVERLAY` variable in `deploy.sh`.

## Troubleshooting

### Cluster creation fails
- Ensure Docker is running
- Check k3d version: `k3d version`
- Try deleting existing cluster: `k3d cluster delete sequencer-local`

### Images not found
- Run `./deploy.sh build` to rebuild and push images
- Check registry: `docker ps | grep sequencer-registry`

### Monitoring installation timeout
- The kube-prometheus-stack chart is large and may take 10-15 minutes to install
- If installation times out, you can retry: `./deploy.sh install-monitoring`
- Check installation progress: `kubectl get pods -n sequencer | grep prometheus`
- The deployment will continue even if monitoring installation fails

### Pods not starting
- Check pod status: `kubectl -n sequencer get pods`
- View logs: `kubectl -n sequencer logs <pod-name>`
- Ensure state was generated: Check that `deployments/local-testing/output/data/node_0` exists
- Ensure state was copied: Run `./deploy.sh copy-state` after deploying sequencer

### State issues
- State is generated locally by running `sequencer_node_setup` binary
- Generated state is in `deployments/local-testing/output/data/node_0`
- Copy state to pod: `./deploy.sh copy-state` (this also restarts the pod)
- Verify state in pod: `kubectl exec -n sequencer <pod-name> -- ls -la /data`

### Config issues
- Config is customized via cdk8s overlays
- Check your overlay YAML for correct service URLs (recorder_url, oracle URLs, etc.)
- Service names in k8s: `dummy-recorder`, `dummy-eth2strk-oracle` (use these in overlay config)

