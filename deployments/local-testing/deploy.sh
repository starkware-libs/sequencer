#!/bin/bash

set -euo pipefail

# Prevent pipenv from creating Pipfile in parent directories
# This ensures each cdk8s project uses its own Pipfile
export PIPENV_IGNORE_VIRTUALENVS=1
export PIPENV_VENV_IN_PROJECT=1
export PIPENV_NO_INHERIT=1
# Prevent pipenv from searching up the directory tree - set empty to force explicit PIPENV_PIPFILE
export PIPENV_PIPFILE=""
# Prevent pipenv from searching up the directory tree
export PIPENV_PIPFILE=""

# Configuration
CLUSTER_NAME="sequencer-local"
NAMESPACE="sequencer"
REGISTRY_NAME="sequencer-registry.localhost"
REGISTRY_PORT="5050"
REGISTRY_URL="${REGISTRY_NAME}:${REGISTRY_PORT}"

# Sequencer overlay configuration
SEQUENCER_LAYOUT="hybrid"
SEQUENCER_OVERLAY="hybrid.testing.node-0"

# Anvil configuration (deployed in same namespace as sequencer)
ANVIL_NAMESPACE="${NAMESPACE}"  # Use same namespace as sequencer
ANVIL_PORT="8545"

# Flags (can be set via command line)
SKIP_DOCKER_BUILD=false
SKIP_INSTALL_MONITORING=false
SKIP_BUILD_RUST_BINARIES=false

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SEQUENCER_ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $*"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $*"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $*"
}

# Safety check: Verify we're using the correct k3d cluster
# This prevents accidental operations on production clusters
verify_k3d_cluster() {
    # Check if cluster exists in k3d
    if ! k3d cluster list | grep -q "${CLUSTER_NAME}"; then
        log_error "k3d cluster '${CLUSTER_NAME}' does not exist!"
        log_error "Please create it first with: $0 up"
        log_error "Or create it manually: k3d cluster create ${CLUSTER_NAME} --config ${SCRIPT_DIR}/k3d/cluster-config.yaml"
        exit 1
    fi
    
    # Get current kubectl context
    local current_context=$(kubectl config current-context 2>/dev/null || echo "")
    
    # Expected context format for k3d: k3d-${CLUSTER_NAME}
    local expected_context="k3d-${CLUSTER_NAME}"
    
    if [ -z "$current_context" ]; then
        log_error "No kubectl context is set!"
        log_error "Please set the context to the k3d cluster:"
        log_error "  k3d kubeconfig merge ${CLUSTER_NAME} --kubeconfig-switch-context"
        exit 1
    fi
    
    if [ "$current_context" != "$expected_context" ]; then
        log_error "Current kubectl context is '${current_context}', but expected '${expected_context}'"
        log_error "This script only works with the local k3d cluster '${CLUSTER_NAME}'"
        log_error "To switch to the correct context, run:"
        log_error "  k3d kubeconfig merge ${CLUSTER_NAME} --kubeconfig-switch-context"
        log_error ""
        log_error "For safety, this script will not proceed with a different cluster context."
        exit 1
    fi
    
    # Verify we can actually reach the cluster
    if ! kubectl cluster-info --request-timeout=5s &>/dev/null; then
        log_error "Cannot reach the k3d cluster '${CLUSTER_NAME}'"
        log_error "The cluster may not be running. Check with: k3d cluster list"
        exit 1
    fi
    
    log_info "Verified: Using k3d cluster '${CLUSTER_NAME}' (context: ${current_context})"
}

# Install Anvil if missing
install_anvil() {
    if command -v anvil &> /dev/null; then
        log_info "Anvil is already installed: $(which anvil)"
        return 0
    fi
    
    log_info "Anvil not found, installing..."
    
    # Try to install to ~/.local/bin (common local bin directory)
    local install_dir="${HOME}/.local/bin"
    mkdir -p "$install_dir"
    
    # Check if install directory is in PATH
    if [[ ":$PATH:" != *":${install_dir}:"* ]]; then
        log_warn "~/.local/bin is not in PATH. Adding it temporarily..."
        export PATH="${install_dir}:${PATH}"
    fi
    
    # Download and install Anvil
    log_info "Downloading Anvil v0.3.0..."
    cd "$install_dir" || {
        log_error "Failed to change to ${install_dir}"
        exit 1
    }
    
    curl -L https://github.com/foundry-rs/foundry/releases/download/v0.3.0/foundry_v0.3.0_linux_amd64.tar.gz | tar -xz --wildcards 'anvil' || {
        log_error "Failed to download/install Anvil"
        log_error "Please install manually:"
        log_error "  curl -L https://github.com/foundry-rs/foundry/releases/download/v0.3.0/foundry_v0.3.0_linux_amd64.tar.gz | tar -xz --wildcards 'anvil'"
        log_error "  mv anvil ~/.local/bin/  # or another directory in your PATH"
        exit 1
    }
    
    chmod +x anvil
    cd - > /dev/null
    
    log_info "Anvil installed to ${install_dir}/anvil"
    
    # Verify it's accessible
    if ! command -v anvil &> /dev/null; then
        log_error "Anvil installed but not in PATH. Please add ${install_dir} to your PATH"
        log_error "  export PATH=\"${install_dir}:\$PATH\""
        exit 1
    fi
    
    log_info "Anvil installation verified: $(which anvil)"
}

# Check prerequisites
check_prerequisites() {
    log_info "Checking prerequisites..."
    
    local missing=0
    local missing_tools=()
    
    # Required tools with installation instructions
    declare -A tool_instructions=(
        ["docker"]="Install Docker: https://docs.docker.com/engine/install/"
        ["k3d"]="Install k3d: https://k3d.io/#installation (requires: curl -s https://raw.githubusercontent.com/k3d-io/k3d/main/install.sh | bash)"
        ["kubectl"]="Install kubectl: https://kubernetes.io/docs/tasks/tools/"
        ["helm"]="Install Helm: https://helm.sh/docs/intro/install/"
        ["cargo"]="Install Rust toolchain: https://rustup.rs/ (cargo comes with rust)"
        ["rustc"]="Install Rust toolchain: https://rustup.rs/"
        ["pipenv"]="Install pipenv: pip install pipenv"
        ["cdk8s"]="Install cdk8s-cli: npm install -g cdk8s-cli"
        ["python3"]="Install Python 3.10+: https://www.python.org/downloads/"
        ["curl"]="Install curl: Usually pre-installed on Linux/Mac. For Ubuntu: sudo apt-get install curl"
    )
    
    # Check all required tools
    for cmd in docker k3d kubectl helm cargo rustc pipenv cdk8s python3 curl; do
        printf "\r${GREEN}[INFO]${NC} Checking: %-10s" "$cmd"
        if ! command -v "$cmd" &> /dev/null; then
            printf "\n"
            log_error "✗ $cmd is not installed"
            missing_tools+=("$cmd")
            missing=1
        else
            # Show version for some tools
            printf "\n"
            case "$cmd" in
                docker)
                    local version=$(docker --version 2>/dev/null | head -1)
                    log_info "✓ docker: $version"
                    ;;
                k3d)
                    local version=$(k3d version 2>/dev/null | grep "k3d version" | head -1 || echo "installed")
                    log_info "✓ k3d: $version"
                    ;;
                kubectl)
                    local version=$(kubectl version --client --short 2>/dev/null | head -1 || echo "installed")
                    log_info "✓ kubectl: $version"
                    ;;
                helm)
                    local version=$(helm version --short 2>/dev/null | head -1 || echo "installed")
                    log_info "✓ helm: $version"
                    ;;
                cargo|rustc)
                    local version=$(rustc --version 2>/dev/null | head -1 || echo "installed")
                    log_info "✓ $cmd: $version"
                    ;;
                python3)
                    local version=$(python3 --version 2>/dev/null | head -1 || echo "installed")
                    log_info "✓ python3: $version"
                    ;;
                *)
                    log_info "✓ $cmd: installed"
                    ;;
            esac
        fi
    done
    printf "\n"
    
    if [ "$missing" -eq 1 ]; then
        log_error ""
        log_error "Missing required tools: ${missing_tools[*]}"
        log_error ""
        log_error "Installation instructions:"
        for tool in "${missing_tools[@]}"; do
            if [ -n "${tool_instructions[$tool]}" ]; then
                log_error "  $tool: ${tool_instructions[$tool]}"
            else
                log_error "  $tool: Please install $tool"
            fi
        done
        exit 1
    fi
    
    # Check for Anvil (required for sequencer_node_setup)
    if ! command -v anvil &> /dev/null; then
        log_warn "Anvil not found (required for state generation)"
        log_info "Attempting to install Anvil..."
        install_anvil
    else
        log_info "✓ anvil: $(which anvil)"
    fi
    
    # Verify Docker is running
    if ! docker info > /dev/null 2>&1; then
        log_error "Docker is installed but not running"
        log_error "Please start Docker and try again"
        exit 1
    fi
    log_info "✓ Docker is running"
    
    # Verify overlay path exists (convert dot notation to path)
    local overlay_path="${SEQUENCER_ROOT_DIR}/deployments/sequencer/configs/overlays/$(echo ${SEQUENCER_OVERLAY} | tr '.' '/')"
    if [ ! -d "$overlay_path" ]; then
        log_warn "Overlay path does not exist: ${overlay_path}"
        log_warn "Using overlay: ${SEQUENCER_OVERLAY}"
        log_warn "This may cause cdk8s synth to fail"
    else
        log_info "✓ Sequencer overlay found: ${SEQUENCER_OVERLAY}"
    fi
    
    log_info ""
    log_info "All prerequisites met ✓"
}

# Build Rust binaries locally
build_binaries() {
    log_info "Building Rust binaries..."
    
    cd "${SEQUENCER_ROOT_DIR}"
    
    # Build sequencer_node_setup and sequencer_simulator
    # Note: Python script sequencer_simulator.py calls the Rust sequencer_simulator binary
    cargo build --bin sequencer_node_setup --bin sequencer_simulator || {
        log_error "Failed to build binaries"
        exit 1
    }
    
    # Restore executable permissions (in case they were lost)
    chmod +x ./target/debug/sequencer_node_setup ./target/debug/sequencer_simulator 2>/dev/null || true
    
    log_info "Binaries built successfully"
}

# Generate initial state data
generate_state() {
    log_info "Generating initial sequencer state..."
    
    # Ensure Anvil is in PATH
    if ! command -v anvil &> /dev/null; then
        log_error "Anvil not found in PATH. Please ensure Anvil is installed and in PATH"
        log_error "Run: ./deploy.sh up (which will install Anvil automatically)"
        exit 1
    fi
    
    log_info "Using Anvil: $(which anvil)"
    
    # Use output directory under local-testing project
    local output_dir="${SCRIPT_DIR}/output"
    local data_prefix="/data"
    
    # Clean output directory if it exists (to avoid KeyAlreadyExists errors)
    if [ -d "${output_dir}" ]; then
        log_info "Cleaning existing output directory: ${output_dir}"
        rm -rf "${output_dir}" || {
            log_warn "Failed to clean output directory, continuing anyway..."
        }
    fi
    
    # Run sequencer_node_setup from sequencer root (needs access to crates, etc.)
    cd "${SEQUENCER_ROOT_DIR}"
    
    # Run sequencer_node_setup to generate state (using absolute path for output)
    ./target/debug/sequencer_node_setup \
        --output-base-dir "${output_dir}" \
        --data-prefix-path "${data_prefix}" \
        --n-consolidated 1 \
        --n-hybrid 0 \
        --n-distributed 0 \
        --n-validator 0 || {
        log_error "Failed to generate state"
        log_error "Make sure Anvil is installed and accessible: which anvil"
        exit 1
    }
    
    cd - > /dev/null
    
    log_info "State generated in ${output_dir}/data/node_0"
}

# Copy state to sequencer pod and restart it
copy_state_and_restart() {
    verify_k3d_cluster
    log_info "Copying state to sequencer pod and restarting..."
    
    local data_dir="${SCRIPT_DIR}/output/data/node_0"
    
    if [ ! -d "$data_dir" ]; then
        log_error "State directory not found: ${data_dir}"
        log_error "Run 'generate_state' first or ensure sequencer_node_setup completed"
        exit 1
    fi
    
    # Determine which service to copy state to based on layout
    local service_label
    if [ "$SEQUENCER_LAYOUT" == "hybrid" ]; then
        # For hybrid layout, state goes to the "core" service
        service_label="service=sequencer-core"
        log_info "Using hybrid layout - copying state to core service"
    else
        # For consolidated/distributed, use app label
        service_label="app=sequencer"
        log_info "Using ${SEQUENCER_LAYOUT} layout"
    fi
    
    # Find sequencer pod
    local pod_name
    pod_name=$(kubectl get pods -n "${NAMESPACE}" -l "${service_label}" -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || echo "")
    
    if [ -z "$pod_name" ]; then
        log_warn "No sequencer pod found with label ${service_label}"
        log_warn "Make sure sequencer is deployed first. Check with: kubectl get pods -n ${NAMESPACE}"
        return
    fi
    
    log_info "Found sequencer pod: ${pod_name}"
    
    # Copy state data to pod
    log_info "Copying state data to pod..."
    kubectl cp "${data_dir}/." "${NAMESPACE}/${pod_name}:/data" --retries=3 || {
        log_error "Failed to copy state to pod"
        exit 1
    }
    
    # Restart pod to read the new state
    log_info "Restarting pod to load new state..."
    kubectl delete pod "${pod_name}" -n "${NAMESPACE}" || {
        log_error "Failed to restart pod"
        exit 1
    }
    
    # Wait for pod to be ready
    log_info "Waiting for pod to become ready..."
    kubectl wait --for=condition=Ready \
        pod -l "${service_label}" \
        -n "${NAMESPACE}" \
        --timeout=5m || {
        log_error "Pod did not become ready"
        exit 1
    }
    
    log_info "State copied and pod restarted successfully"
}

# Build and push Docker images
build_images() {
    log_info "Building and pushing Docker images..."
    
    export DOCKER_BUILDKIT=1
    export COMPOSE_DOCKER_CLI_BUILD=1
    
    # Ensure registry is running
    if ! docker ps | grep -q "${REGISTRY_NAME}"; then
        log_warn "Registry not running, starting cluster first..."
        create_cluster
    fi
    
    # Build images
    # Note: sequencer-simulator Docker image is NOT needed - simulator runs as Python script locally
    local images=(
        "dummy-recorder:deployments/images/sequencer/dummy_recorder.Dockerfile"
        "dummy-eth-to-strk-oracle:deployments/images/sequencer/dummy_eth_to_strk_oracle.Dockerfile"
        "sequencer:deployments/images/sequencer/Dockerfile"
    )
    
    for image_spec in "${images[@]}"; do
        IFS=':' read -r image_name dockerfile_path <<< "$image_spec"
        log_info "Building ${image_name}..."
        
        local build_args=""
        if [ "$image_name" == "sequencer" ]; then
            build_args="--build-arg BUILD_MODE=debug"
        fi
        
        docker build \
            $build_args \
            -f "${SEQUENCER_ROOT_DIR}/${dockerfile_path}" \
            -t "${REGISTRY_URL}/${image_name}:local" \
            "${SEQUENCER_ROOT_DIR}"
        
        log_info "Pushing ${image_name}..."
        docker push "${REGISTRY_URL}/${image_name}:local"
    done
    
    log_info "All images built and pushed"
}

# Rollout restart all deployments and statefulsets to pick up new images
rollout_restart_all() {
    verify_k3d_cluster
    log_info "Restarting all deployments and statefulsets to pick up new images..."
    
    # Restart all deployments
    local deployments=$(kubectl get deployments -n "${NAMESPACE}" -o name 2>/dev/null || echo "")
    if [ -n "$deployments" ]; then
        log_info "Restarting deployments..."
        kubectl rollout restart deployments -n "${NAMESPACE}"
    fi
    
    # Restart all statefulsets
    local statefulsets=$(kubectl get statefulsets -n "${NAMESPACE}" -o name 2>/dev/null || echo "")
    if [ -n "$statefulsets" ]; then
        log_info "Restarting statefulsets..."
        kubectl rollout restart statefulsets -n "${NAMESPACE}"
    fi
    
    # Wait for rollouts to complete
    log_info "Waiting for rollouts to complete..."
    kubectl rollout status deployments -n "${NAMESPACE}" --timeout=300s 2>/dev/null || true
    kubectl rollout status statefulsets -n "${NAMESPACE}" --timeout=300s 2>/dev/null || true
    
    log_info "All workloads restarted ✓"
}

# Prompt user to deploy after build
prompt_deploy_after_build() {
    echo ""
    log_info "════════════════════════════════════════════════════════════"
    log_info "  Images built successfully!"
    log_info "════════════════════════════════════════════════════════════"
    echo ""
    read -p "$(echo -e ${YELLOW}[PROMPT]${NC}) Deploy new images to the cluster? (restart all pods) [y/N]: " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rollout_restart_all
    else
        log_info "Skipped deployment. To deploy later, run:"
        log_info "  kubectl rollout restart deployments -n ${NAMESPACE}"
        log_info "  kubectl rollout restart statefulsets -n ${NAMESPACE}"
    fi
}

# Create k3d cluster
create_cluster() {
    log_info "Creating k3d cluster..."
    
    if k3d cluster list | grep -q "${CLUSTER_NAME}"; then
        log_warn "Cluster ${CLUSTER_NAME} already exists"
        log_info "Note: If you changed port mappings in k3d/cluster-config.yaml, you may need to delete and recreate the cluster"
        return
    fi
    
    k3d cluster create "${CLUSTER_NAME}" \
        --config "${SCRIPT_DIR}/k3d/cluster-config.yaml" \
        --wait
    
    # Connect kubectl to cluster
    k3d kubeconfig merge "${CLUSTER_NAME}" --kubeconfig-switch-context
    
    # Wait for cluster to be ready
    log_info "Waiting for cluster to be ready..."
    kubectl cluster-info --request-timeout=30s
    
    log_info "Cluster created successfully"
}

# Delete k3d cluster
delete_cluster() {
    log_info "Deleting k3d cluster..."
    
    if k3d cluster list | grep -q "${CLUSTER_NAME}"; then
        k3d cluster delete "${CLUSTER_NAME}"
        log_info "Cluster deleted"
    else
        log_warn "Cluster ${CLUSTER_NAME} does not exist"
    fi
}

# Install monitoring stack
install_monitoring() {
    verify_k3d_cluster
    log_info "Installing Prometheus/Grafana stack..."
    
    # Add prometheus-community repo if not already added
    if ! helm repo list | grep -q prometheus-community; then
        log_info "Adding prometheus-community Helm repository..."
        helm repo add prometheus-community https://prometheus-community.github.io/helm-charts || {
            log_error "Failed to add prometheus-community repo"
            exit 1
        }
        helm repo update || {
            log_error "Failed to update Helm repos"
            exit 1
        }
    fi
    
    # Install or upgrade without --wait to avoid hanging on LoadBalancer services
    # LoadBalancer services in k3d may not get external IPs immediately, causing --wait to hang
    log_info "Installing/upgrading kube-prometheus-stack (without wait to avoid hanging on LoadBalancer)..."
    helm upgrade --install kube-prometheus-stack \
        prometheus-community/kube-prometheus-stack \
        --namespace "${NAMESPACE}" \
        --create-namespace \
        --values "${SCRIPT_DIR}/helm/prometheus-stack-values.yaml" \
        --timeout 5m || {
        log_warn "Helm install/upgrade had issues, but continuing..."
    }
    
    # Wait for pods to be ready manually (more reliable than Helm's --wait for LoadBalancer)
    local max_wait=180
    local waited=0
    log_info "Waiting for monitoring pods to be ready (timeout: ${max_wait}s)..."
    while [ $waited -lt $max_wait ]; do
        local grafana_ready=$(kubectl get pods -n "${NAMESPACE}" -l app.kubernetes.io/name=grafana -o jsonpath='{.items[0].status.phase}' 2>/dev/null || echo "Pending")
        local prometheus_ready=$(kubectl get pods -n "${NAMESPACE}" -l app.kubernetes.io/name=prometheus -o jsonpath='{.items[0].status.phase}' 2>/dev/null || echo "Pending")
        
        # Show progress with elapsed time and status
        printf "\r${GREEN}[INFO]${NC} Waiting... %3ds/%3ds | Grafana: %-10s | Prometheus: %-10s" "$waited" "$max_wait" "${grafana_ready:-Pending}" "${prometheus_ready:-Pending}"
        
        if [ "$grafana_ready" = "Running" ] && [ "$prometheus_ready" = "Running" ]; then
            printf "\n"
            log_info "Monitoring pods are ready ✓"
            break
        fi
        sleep 2
        waited=$((waited + 2))
    done
    
    if [ $waited -ge $max_wait ]; then
        printf "\n"
        log_warn "Monitoring pods did not become ready within ${max_wait}s timeout, but continuing..."
    fi
    
    # Patch Prometheus service to NodePort (Prometheus Operator may not respect Helm values)
    log_info "Patching Prometheus service to NodePort for direct access on localhost:9090..."
    kubectl patch svc kube-prometheus-stack-prometheus -n "${NAMESPACE}" \
        -p '{"spec":{"type":"NodePort","ports":[{"name":"web","port":9090,"targetPort":9090,"protocol":"TCP","nodePort":30090},{"name":"reloader-web","port":8080,"targetPort":8080,"protocol":"TCP"}]}}' \
        2>/dev/null || {
        log_warn "Failed to patch Prometheus service, but continuing..."
    }
    
    # Create ServiceMonitor for sequencer metrics (scrapes /monitoring/metrics on port 8082)
    log_info "Creating ServiceMonitor for sequencer metrics..."
    kubectl apply -f "${SCRIPT_DIR}/manifests/servicemonitor-sequencer.yaml" 2>/dev/null || {
        log_warn "Failed to create ServiceMonitor, but continuing..."
    }
    
    # Note: Helm chart creates Prometheus datasource with UID "prometheus" (lowercase)
    # The upload_dashboards function handles updating datasource UIDs as needed
    
    log_info "Monitoring stack installed successfully"
    
    # Upload dashboards right after monitoring is installed
    upload_dashboards || {
        log_warn "Dashboard upload had issues, but continuing with deployment..."
    }
}

# Deploy Anvil
deploy_anvil() {
    verify_k3d_cluster
    log_info "Deploying Anvil..."
    
    local anvil_path="${SEQUENCER_ROOT_DIR}/deployments/anvil"
    local anvil_output="${SCRIPT_DIR}/manifests/anvil"
    mkdir -p "$anvil_output"
    
    # Ensure namespace exists (Anvil uses same namespace as sequencer)
    kubectl create namespace "${ANVIL_NAMESPACE}" 2>/dev/null || true
    
    cd "$anvil_path"
    
    # Install dependencies (use explicit PIPFILE to avoid conflicts)
    # Always sync dependencies to ensure virtualenv is up to date
    if [ ! -f "Pipfile.lock" ]; then
        log_info "Installing pipenv dependencies for anvil..."
        # Explicitly set PIPFILE to ensure we use the local one and prevent creating files in parent dirs
        PIPENV_PIPFILE="${anvil_path}/Pipfile" PIPENV_VENV_IN_PROJECT=1 pipenv install || {
            log_error "Failed to install pipenv dependencies for anvil"
            exit 1
        }
    else
        log_info "Syncing pipenv dependencies for anvil..."
        # Sync to ensure virtualenv has all dependencies even if Pipfile.lock exists
        PIPENV_PIPFILE="${anvil_path}/Pipfile" PIPENV_VENV_IN_PROJECT=1 pipenv sync || {
            log_warn "pipenv sync failed, trying pipenv install..."
            PIPENV_PIPFILE="${anvil_path}/Pipfile" PIPENV_VENV_IN_PROJECT=1 pipenv install || {
                log_error "Failed to install pipenv dependencies for anvil"
                exit 1
            }
        }
    fi
    
    # Import cdk8s dependencies if needed
    if [ ! -d "imports" ]; then
        log_info "Running cdk8s import for anvil..."
        cdk8s import || {
            log_error "Failed to import cdk8s dependencies for anvil"
            exit 1
        }
    fi
    
    # Use explicit PIPFILE when running pipenv to prevent creating files in parent dirs
    PIPENV_PIPFILE="${anvil_path}/Pipfile" cdk8s synth --app "pipenv run python main.py --namespace ${ANVIL_NAMESPACE}" --output "$anvil_output" || {
        log_error "Failed to generate anvil manifests"
        exit 1
    }
    cd - > /dev/null
    
    # Apply Anvil manifests
    kubectl create namespace "${ANVIL_NAMESPACE}" || true
    kubectl apply -R -f "$anvil_output" || {
        log_error "Failed to apply anvil manifests"
        exit 1
    }
    
    # Wait for Anvil to be ready
    log_info "Waiting for Anvil to become ready..."
    kubectl wait --namespace "${ANVIL_NAMESPACE}" --for=condition=Ready -l app=anvil pod --timeout=60s || {
        log_error "Anvil pod did not become ready"
        exit 1
    }
    
    log_info "Anvil deployed successfully"
}

# Extract Anvil addresses from logs
extract_anvil_addresses() {
    verify_k3d_cluster
    log_info "Extracting Anvil addresses from logs..."
    
    local anvil_pod
    anvil_pod=$(kubectl get pods -n "${ANVIL_NAMESPACE}" -l app=anvil -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || echo "")
    
    if [ -z "$anvil_pod" ]; then
        log_error "Anvil pod not found"
        return 1
    fi
    
    # Wait a bit for Anvil to start and log addresses
    sleep 3
    
    local addresses
    addresses=$(kubectl logs -n "${ANVIL_NAMESPACE}" "$anvil_pod" 2>/dev/null | grep -oP '0x[a-fA-F0-9]{40}' | head -n 2 || echo "")
    
    if [ -z "$addresses" ]; then
        log_warn "Could not extract Anvil addresses from logs"
        log_warn "You may need to provide sender/receiver addresses manually for simulator"
        return 1
    fi
    
    SENDER_ADDRESS=$(echo "$addresses" | head -n 1)
    RECEIVER_ADDRESS=$(echo "$addresses" | tail -n 1)
    
    log_info "SENDER_ADDRESS=$SENDER_ADDRESS"
    log_info "RECEIVER_ADDRESS=$RECEIVER_ADDRESS"
    
    export SENDER_ADDRESS
    export RECEIVER_ADDRESS
}

# Rerun simulator test with fresh state
# Wipes PVC data, regenerates state, copies to pod, runs simulator
rerun_simulator() {
    verify_k3d_cluster
    log_info "Resetting sequencer state and running simulator test..."
    
    # Determine which service to target based on layout
    local service_label
    local sts_name
    if [ "$SEQUENCER_LAYOUT" == "hybrid" ]; then
        service_label="service=sequencer-core"
    else
        service_label="app=sequencer"
    fi
    
    # Find the StatefulSet name
    sts_name=$(kubectl get statefulsets -n "${NAMESPACE}" -l "${service_label}" -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || echo "")
    
    if [ -z "$sts_name" ]; then
        # Try to find by common naming pattern
        sts_name=$(kubectl get statefulsets -n "${NAMESPACE}" -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || echo "")
    fi
    
    if [ -z "$sts_name" ]; then
        log_error "No StatefulSet found in namespace ${NAMESPACE}"
        exit 1
    fi
    
    log_info "Found StatefulSet: ${sts_name}"
    
    # Step 1: Scale down StatefulSet to 0 (stops pod without recreation)
    log_info "Scaling down StatefulSet to 0 replicas..."
    kubectl scale statefulset -n "${NAMESPACE}" "${sts_name}" --replicas=0
    
    # Wait for pod to terminate
    log_info "Waiting for pod to terminate..."
    local max_wait=60
    local waited=0
    while [ $waited -lt $max_wait ]; do
        local pod_count=$(kubectl get pods -n "${NAMESPACE}" -l "${service_label}" --no-headers 2>/dev/null | wc -l)
        if [ "$pod_count" -eq 0 ]; then
            log_info "Pod terminated successfully"
            break
        fi
        printf "\r${GREEN}[INFO]${NC} Waiting for pod termination... %ds/%ds" "$waited" "$max_wait"
        sleep 2
        waited=$((waited + 2))
    done
    echo ""  # New line after progress
    
    # Step 2: Delete sequencer PVC (now safe since no pod is using it)
    # StatefulSet PVCs follow pattern: <volumeClaimTemplateName>-<statefulsetName>-<ordinal>
    log_info "Wiping sequencer PVC data..."
    local pvc_name=""
    
    # Try to find PVC by label first
    pvc_name=$(kubectl get pvc -n "${NAMESPACE}" -l "${service_label}" -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || echo "")
    
    # If not found by label, try common naming patterns for the StatefulSet
    if [ -z "$pvc_name" ]; then
        # Common patterns: data-<sts>-0, <sts>-data-0, sequencer-core-data
        for pattern in "data-${sts_name}-0" "${sts_name}-data" "sequencer-core-data"; do
            if kubectl get pvc -n "${NAMESPACE}" "${pattern}" &>/dev/null; then
                pvc_name="${pattern}"
                break
            fi
        done
    fi
    
    if [ -n "$pvc_name" ]; then
        log_info "Deleting PVC: ${pvc_name}"
        kubectl delete pvc -n "${NAMESPACE}" "${pvc_name}" --wait=true || true
        log_info "PVC ${pvc_name} deleted"
    else
        log_warn "No sequencer PVC found to delete"
        # List available PVCs for debugging
        log_info "Available PVCs in namespace:"
        kubectl get pvc -n "${NAMESPACE}" -o custom-columns=NAME:.metadata.name,STATUS:.status.phase 2>/dev/null || true
    fi
    
    # Step 3: Regenerate state
    log_info "Regenerating state..."
    generate_state
    
    # Step 4: Reapply the sequencer manifests (this recreates the PVC and scales up)
    log_info "Reapplying sequencer manifests..."
    local sequencer_manifest_dir="${SCRIPT_DIR}/manifests/sequencer"
    kubectl apply -R -f "$sequencer_manifest_dir" || {
        log_error "Failed to reapply sequencer manifests"
        exit 1
    }
    
    # Step 5: Ensure StatefulSet is scaled back to 1
    log_info "Scaling StatefulSet back to 1 replica..."
    kubectl scale statefulset -n "${NAMESPACE}" "${sts_name}" --replicas=1
    
    # Step 6: Wait for pod and copy state
    log_info "Waiting for new pod and copying state..."
    wait_and_copy_state
    
    # Step 7: Extract Anvil addresses and run simulator
    log_info "Running simulator test..."
    extract_anvil_addresses || {
        log_error "Could not extract Anvil addresses"
        exit 1
    }
    run_simulator
    
    log_info "Reset and test complete ✓"
}

# Prepare box for Vagrant packaging
# Builds Docker images and Rust binaries to warm caches, then cleans up for smaller box size
prepare_box() {
    log_info "════════════════════════════════════════════════════════════"
    log_info "  Preparing VM for Vagrant box packaging"
    log_info "════════════════════════════════════════════════════════════"
    
    export DOCKER_BUILDKIT=1
    export COMPOSE_DOCKER_CLI_BUILD=1
    
    # Step 1: Build Docker images (cache layers locally)
    log_info ""
    log_info "Step 1/4: Building Docker images to cache layers..."
    log_info "─────────────────────────────────────────────────────────────"
    
    local images=(
        "dummy-recorder:deployments/images/sequencer/dummy_recorder.Dockerfile"
        "dummy-eth-to-strk-oracle:deployments/images/sequencer/dummy_eth_to_strk_oracle.Dockerfile"
        "sequencer:deployments/images/sequencer/Dockerfile"
    )
    
    for image_spec in "${images[@]}"; do
        IFS=':' read -r image_name dockerfile_path <<< "$image_spec"
        log_info "Building ${image_name}..."
        
        local build_args=""
        if [ "$image_name" == "sequencer" ]; then
            build_args="--build-arg BUILD_MODE=debug"
        fi
        
        # Build locally only (no push, no registry needed)
        docker build \
            $build_args \
            -f "${SEQUENCER_ROOT_DIR}/${dockerfile_path}" \
            -t "${image_name}:local" \
            "${SEQUENCER_ROOT_DIR}" || {
            log_warn "Failed to build ${image_name}, continuing..."
        }
    done
    log_info "Docker images built ✓"
    
    # Step 2: Build Rust binaries (warm cargo cache)
    log_info ""
    log_info "Step 2/4: Building Rust binaries to warm cargo cache..."
    log_info "─────────────────────────────────────────────────────────────"
    
    cd "${SEQUENCER_ROOT_DIR}"
    cargo build --release --bin sequencer_node || log_warn "sequencer_node build failed"
    cargo build --release --bin sequencer_node_setup || log_warn "sequencer_node_setup build failed"
    cargo build --release --bin sequencer_simulator || log_warn "sequencer_simulator build failed"
    cd "${SCRIPT_DIR}"
    log_info "Rust binaries built ✓"
    
    # Step 3: Clean up caches and logs
    log_info ""
    log_info "Step 3/4: Cleaning up unnecessary files..."
    log_info "─────────────────────────────────────────────────────────────"
    
    # Clean apt cache
    log_info "Cleaning apt cache..."
    sudo apt-get clean
    sudo apt-get autoremove -y
    
    # Clean old logs
    log_info "Cleaning logs..."
    sudo journalctl --vacuum-time=1d 2>/dev/null || true
    sudo rm -rf /var/log/*.gz /var/log/*.1 /var/log/*.old 2>/dev/null || true
    
    # Clean npm cache if exists
    rm -rf ~/.npm/_cacache/ 2>/dev/null || true
    
    # Keep Rust target directory (contains built binaries)
    # Only clean incremental build cache to save space
    log_info "Cleaning Rust incremental build cache (keeping binaries)..."
    rm -rf "${SEQUENCER_ROOT_DIR}/target/release/incremental/" 2>/dev/null || true
    rm -rf "${SEQUENCER_ROOT_DIR}/target/debug/incremental/" 2>/dev/null || true
    rm -rf "${SEQUENCER_ROOT_DIR}/target/release/.fingerprint/" 2>/dev/null || true
    rm -rf "${SEQUENCER_ROOT_DIR}/target/debug/.fingerprint/" 2>/dev/null || true
    
    # Keep cargo registry index but remove extracted crate cache
    rm -rf ~/.cargo/registry/cache/ 2>/dev/null || true
    
    # Clean pipenv cache
    rm -rf ~/.cache/pipenv/ 2>/dev/null || true
    
    log_info "Cleanup complete ✓"
    
    # Step 4: Zero free space for better compression
    log_info ""
    log_info "Step 4/4: Zeroing free space for better compression..."
    log_info "─────────────────────────────────────────────────────────────"
    log_info "This may take a few minutes..."
    
    sudo dd if=/dev/zero of=/zero.fill bs=1M 2>/dev/null || true
    sudo rm -f /zero.fill
    
    log_info "Free space zeroed ✓"
    
    # Done
    log_info ""
    log_info "════════════════════════════════════════════════════════════"
    log_info "  Box preparation complete!"
    log_info "════════════════════════════════════════════════════════════"
    log_info ""
    log_info "  Next steps (run on HOST, not in VM):"
    log_info ""
    log_info "    1. Exit the VM:        exit"
    log_info "    2. Halt the VM:        vagrant halt"
    log_info "    3. Package the box:    vagrant package --output sequencer-dev.box"
    log_info ""
    log_info "  The box will include:"
    log_info "    ✓ All prerequisites (k3d, helm, docker, rust, etc.)"
    log_info "    ✓ Docker image layers (faster rebuilds)"
    log_info "    ✓ Rust binaries (ready to use)"
    log_info "    ✓ Cargo registry cache (faster Rust builds)"
    log_info ""
}

# Run sequencer simulator
# Run simulator binary directly
run_simulator() {
    log_info "Running sequencer simulator to test transactions..."
    
    # Check if addresses were extracted
    if [ -z "${SENDER_ADDRESS:-}" ] || [ -z "${RECEIVER_ADDRESS:-}" ]; then
        log_warn "Anvil addresses not available, skipping simulator"
        log_warn "You can run simulator manually:"
        log_warn "  cd ${SEQUENCER_ROOT_DIR}"
        log_warn "  pipenv run python ./scripts/system_tests/sequencer_simulator.py \\"
        log_warn "    --state_sync_monitoring_endpoint_port 8082 \\"
        log_warn "    --http_server_port 8080 \\"
        log_warn "    --node_type ${SEQUENCER_LAYOUT} \\"
        log_warn "    --sender_address <address> \\"
        log_warn "    --receiver_address <address>"
        return 1
    fi
    
    # Port-forward Anvil to localhost
    log_info "Setting up port-forward to Anvil..."
    local anvil_pod
    anvil_pod=$(kubectl get pods -n "${ANVIL_NAMESPACE}" -l app=anvil -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || echo "")
    
    if [ -z "$anvil_pod" ]; then
        log_error "Anvil pod not found"
        return 1
    fi
    
    # Start port-forward in background
    kubectl port-forward -n "${ANVIL_NAMESPACE}" "$anvil_pod" "${ANVIL_PORT}:${ANVIL_PORT}" > /dev/null 2>&1 &
    local pf_pid=$!
    sleep 2
    
    # Run simulator
    cd "${SEQUENCER_ROOT_DIR}"
    
    # Create and use a virtual environment for simulator dependencies
    local venv_dir="${SCRIPT_DIR}/.venv"
    if [ ! -d "$venv_dir" ]; then
        log_info "Creating virtual environment for simulator..."
        python3 -m venv "$venv_dir" || {
            log_error "Failed to create virtual environment"
            kill $pf_pid 2>/dev/null || true
            return 1
        }
    fi
    
    # Install Python dependencies in venv
    local venv_python="${venv_dir}/bin/python"
    local venv_pip="${venv_dir}/bin/pip"
    
    if ! "$venv_python" -c "import kubernetes" 2>/dev/null; then
        log_info "Installing Python dependencies for simulator..."
        if [ -f "scripts/requirements.txt" ]; then
            "$venv_pip" install -q -r scripts/requirements.txt || {
                log_error "Failed to install Python dependencies"
                kill $pf_pid 2>/dev/null || true
                return 1
            }
        else
            "$venv_pip" install -q kubernetes || {
                log_error "Failed to install Python dependencies"
                kill $pf_pid 2>/dev/null || true
                return 1
            }
        fi
    fi
    
    log_info "Running sequencer simulator..."
    
    # Clean up any existing port-forwards that might conflict
    log_info "Cleaning up any existing port-forwards on ports 8080 and 8082..."
    pkill -f "kubectl.*port-forward.*8080" 2>/dev/null || true
    pkill -f "kubectl.*port-forward.*8082" 2>/dev/null || true
    sleep 1
    
    # Set default namespace for kubectl commands (simulator uses kubectl without explicit namespace)
    local current_context=$(kubectl config current-context 2>/dev/null || echo "")
    if [ -n "$current_context" ]; then
        kubectl config set-context --current --namespace="${NAMESPACE}" || {
            log_warn "Failed to set namespace context, simulator may fail"
        }
    fi
    
    "$venv_python" ./scripts/system_tests/sequencer_simulator.py \
        --state_sync_monitoring_endpoint_port 8082 \
        --http_server_port 8080 \
        --node_type "${SEQUENCER_LAYOUT}" \
        --sender_address "${SENDER_ADDRESS}" \
        --receiver_address "${RECEIVER_ADDRESS}" || {
        log_error "Simulator failed"
        # Restore original namespace if we changed it
        if [ -n "$current_context" ]; then
            kubectl config set-context --current --namespace=default 2>/dev/null || true
        fi
        kill $pf_pid 2>/dev/null || true
        return 1
    }
    
    # Restore original namespace
    if [ -n "$current_context" ]; then
        kubectl config set-context --current --namespace=default 2>/dev/null || true
    fi
    
    # Clean up port-forward
    kill $pf_pid 2>/dev/null || true
    
    log_info "Simulator completed successfully!"
    cd - > /dev/null
}

# Generate cdk8s manifests for services
generate_cdk8s_manifests() {
    log_info "Generating Kubernetes manifests via cdk8s..."
    
    local manifests_dir="${SCRIPT_DIR}/manifests"
    
    # Generate manifests for supporting services (dummy services)
    # Format: "directory_name:output_subdir:image_name"
    local services=(
        "dummy_recorder:dummy-recorder:${REGISTRY_URL}/dummy-recorder:local"
        "dummy_eth2strk_oracle:dummy-eth2strk-oracle:${REGISTRY_URL}/dummy-eth-to-strk-oracle:local"
    )
    
    for service_spec in "${services[@]}"; do
        IFS=':' read -r service_dir output_subdir image <<< "$service_spec"
        log_info "Generating manifests for ${service_dir}..."
        
        local service_path="${SEQUENCER_ROOT_DIR}/deployments/${service_dir}"
        if [ ! -d "$service_path" ]; then
            log_error "Service directory not found: ${service_path}"
            exit 1
        fi
        
        local output_dir="${manifests_dir}/${output_subdir}"
        mkdir -p "$output_dir"
        
        # Run cdk8s synth with custom output directory
        cd "$service_path"
        
        # Install dependencies (use explicit PIPFILE to avoid conflicts)
        # Always sync dependencies to ensure virtualenv is up to date
        if [ ! -f "Pipfile.lock" ]; then
            log_info "Installing pipenv dependencies for ${service_dir}..."
            # Explicitly set PIPFILE to ensure we use the local one and prevent creating files in parent dirs
            PIPENV_PIPFILE="${service_path}/Pipfile" PIPENV_VENV_IN_PROJECT=1 pipenv install || {
                log_error "Failed to install pipenv dependencies for ${service_dir}"
                exit 1
            }
        else
            log_info "Syncing pipenv dependencies for ${service_dir}..."
            # Sync to ensure virtualenv has all dependencies even if Pipfile.lock exists
            PIPENV_PIPFILE="${service_path}/Pipfile" PIPENV_VENV_IN_PROJECT=1 pipenv sync || {
                log_warn "pipenv sync failed, trying pipenv install..."
                PIPENV_PIPFILE="${service_path}/Pipfile" PIPENV_VENV_IN_PROJECT=1 pipenv install || {
                    log_error "Failed to install pipenv dependencies for ${service_dir}"
                    exit 1
                }
            }
        fi
        
        # Import cdk8s dependencies if needed
        if [ ! -d "imports" ]; then
            log_info "Running cdk8s import for ${service_dir}..."
            cdk8s import || {
                log_error "Failed to import cdk8s dependencies for ${service_dir}"
                exit 1
            }
        fi
        
        # Use explicit PIPFILE when running pipenv to prevent creating files in parent dirs
        PIPENV_PIPFILE="${service_path}/Pipfile" cdk8s synth --app "pipenv run python main.py --namespace ${NAMESPACE} --image ${image}" --output "$output_dir" || {
            log_error "Failed to generate manifests for ${service_dir}"
            exit 1
        }
        cd - > /dev/null
    done
    
    log_info "Dummy service manifests generated in ${manifests_dir}/"
}

# Generate sequencer manifests only
generate_sequencer_manifests() {
    log_info "Generating sequencer manifests with overlay ${SEQUENCER_OVERLAY}..."
    local manifests_dir="${SCRIPT_DIR}/manifests"
    local sequencer_path="${SEQUENCER_ROOT_DIR}/deployments/sequencer"
    local sequencer_output="${manifests_dir}/sequencer"
    mkdir -p "$sequencer_output"
    
    cd "$sequencer_path"
    
    # Install dependencies and import if needed
    if [ ! -d "imports" ]; then
        log_info "Running cdk8s import for sequencer..."
        cdk8s import || {
            log_error "Failed to import cdk8s dependencies"
            exit 1
        }
    fi
    
    # Install dependencies (use explicit PIPFILE to avoid conflicts)
    # Always sync dependencies to ensure virtualenv is up to date
    if [ ! -f "Pipfile.lock" ]; then
        log_info "Installing pipenv dependencies for sequencer..."
        # Explicitly set PIPFILE to ensure we use the local one and prevent creating files in parent dirs
        PIPENV_PIPFILE="${sequencer_path}/Pipfile" PIPENV_VENV_IN_PROJECT=1 pipenv install || {
            log_error "Failed to install pipenv dependencies"
            exit 1
        }
    else
        log_info "Syncing pipenv dependencies for sequencer..."
        # Sync to ensure virtualenv has all dependencies even if Pipfile.lock exists
        PIPENV_PIPFILE="${sequencer_path}/Pipfile" PIPENV_VENV_IN_PROJECT=1 pipenv sync || {
            log_warn "pipenv sync failed, trying pipenv install..."
            PIPENV_PIPFILE="${sequencer_path}/Pipfile" PIPENV_VENV_IN_PROJECT=1 pipenv install || {
                log_error "Failed to install pipenv dependencies"
                exit 1
            }
        }
    fi
    
    # Use explicit PIPFILE when running pipenv to prevent creating files in parent dirs
    PIPENV_PIPFILE="${sequencer_path}/Pipfile" cdk8s synth --app "pipenv run python main.py --namespace ${NAMESPACE} -l ${SEQUENCER_LAYOUT} -o ${SEQUENCER_OVERLAY} --image ${REGISTRY_URL}/sequencer:local" --output "$sequencer_output" || {
        log_error "Failed to generate sequencer manifests"
        exit 1
    }
    cd - > /dev/null
    
    log_info "Sequencer manifests generated in ${sequencer_output}/"
}

# Apply dummy services
apply_dummy_services() {
    verify_k3d_cluster
    log_info "Applying dummy service manifests..."
    
    # Create namespace first
    kubectl apply -f "${SCRIPT_DIR}/manifests/namespace.yaml"
    kubectl wait --for=jsonpath='{.status.phase}'=Active namespace/"${NAMESPACE}" --timeout=30s || true
    
    local manifests_dir="${SCRIPT_DIR}/manifests"
    
    # Apply dummy services
    for service_subdir in dummy-recorder dummy-eth2strk-oracle; do
        local service_manifest_dir="${manifests_dir}/${service_subdir}"
        if [ -d "$service_manifest_dir" ] && [ "$(ls -A "$service_manifest_dir" 2>/dev/null)" ]; then
            log_info "Applying manifests from ${service_subdir}..."
            kubectl apply -R -f "$service_manifest_dir" || {
                log_error "Failed to apply manifests from ${service_manifest_dir}"
                exit 1
            }
        else
            log_error "No manifests found for ${service_subdir}"
            exit 1
        fi
    done
    
    # Wait for dummy services to be ready
    log_info "Waiting for dummy services to be ready..."
    kubectl wait --for=condition=available deployment/dummy-recorder-deployment -n "${NAMESPACE}" --timeout=120s || {
        log_error "Dummy recorder did not become ready"
        exit 1
    }
    kubectl wait --for=condition=available deployment/dummy-eth2strk-oracle-deployment -n "${NAMESPACE}" --timeout=120s || {
        log_error "Dummy ETH-STRK oracle did not become ready"
        exit 1
    }
    
    log_info "Dummy services deployed and ready"
}

# Apply sequencer manifests
apply_sequencer() {
    verify_k3d_cluster
    log_info "Applying sequencer manifests..."
    
    local manifests_dir="${SCRIPT_DIR}/manifests"
    local sequencer_manifest_dir="${manifests_dir}/sequencer"
    
    if [ -d "$sequencer_manifest_dir" ] && [ "$(ls -A "$sequencer_manifest_dir" 2>/dev/null)" ]; then
        log_info "Applying sequencer manifests..."
        kubectl apply -R -f "$sequencer_manifest_dir" || {
            log_error "Failed to apply sequencer manifests"
            exit 1
        }
    else
        log_error "No sequencer manifests found. Run generate_sequencer_manifests first."
        exit 1
    fi
    
    log_info "Sequencer manifests applied"
}

# Wait for sequencer pod and copy state
wait_and_copy_state() {
    verify_k3d_cluster
    log_info "Waiting for sequencer pod to be ready before copying state (timeout: 180s)..."
    
    local service_label
    if [ "$SEQUENCER_LAYOUT" == "hybrid" ]; then
        service_label="service=sequencer-core"
    else
        service_label="app=sequencer"
    fi
    
    # Wait for pod to exist and be ready
    local max_wait=180
    local waited=0
    local pod_ready=false
    
    while [ $waited -lt $max_wait ]; do
        local pod_name
        pod_name=$(kubectl get pods -n "${NAMESPACE}" -l "${service_label}" -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || echo "")
        
        local pod_phase="NotFound"
        if [ -n "$pod_name" ]; then
            pod_phase=$(kubectl get pod -n "${NAMESPACE}" "$pod_name" -o jsonpath='{.status.phase}' 2>/dev/null || echo "Unknown")
            
            if [ "$pod_phase" == "Running" ]; then
                printf "\n"
                log_info "Sequencer pod is running ✓, copying state..."
                if copy_state_and_restart; then
                    pod_ready=true
                    break
                else
                    log_error "State copy failed"
                    exit 1
                fi
            fi
        fi
        
        # Show progress with elapsed time and status
        printf "\r${GREEN}[INFO]${NC} Waiting... %3ds/%ds | Pod: %-12s" "$waited" "$max_wait" "$pod_phase"
        
        sleep 3
        waited=$((waited + 3))
    done
    
    if [ "$pod_ready" = false ]; then
        printf "\n"
        log_error "Sequencer pod did not become ready in time (waited ${waited}s)"
        exit 1
    fi
}

# Upload Grafana dashboards (optional, can be done manually)
upload_dashboards() {
    verify_k3d_cluster
    log_info "Uploading Grafana dashboards..."
    
    # Wait for Grafana to be ready
    kubectl wait --for=condition=available \
        --timeout=2m \
        deployment/kube-prometheus-stack-grafana \
        -n "${NAMESPACE}" 2>/dev/null || {
        log_warn "Grafana not ready, skipping dashboard upload"
        log_warn "You can upload dashboards manually later via Grafana UI"
        return
    }
    
    # Use the same virtual environment as the simulator (or create one if it doesn't exist)
    local venv_dir="${SCRIPT_DIR}/.venv"
    if [ ! -d "$venv_dir" ]; then
        log_info "Creating virtual environment for dashboard upload..."
        python3 -m venv "$venv_dir" || {
            log_warn "Failed to create virtual environment, skipping dashboard upload"
            return
        }
    fi
    
    # Install Python dependencies in venv
    local monitoring_dir="${SEQUENCER_ROOT_DIR}/deployments/monitoring"
    local venv_pip="${venv_dir}/bin/pip"
    local venv_python="${venv_dir}/bin/python"
    
    if [ -f "${monitoring_dir}/src/requirements.txt" ]; then
        "$venv_pip" install -q -r "${monitoring_dir}/src/requirements.txt" 2>/dev/null || {
            log_warn "Failed to install Python dependencies, skipping dashboard upload"
            return
        }
    fi
    
    # Create/update Prometheus datasource with correct UID for dashboards
    # Dashboards expect UID "Prometheus" but Helm may create "prometheus"
    log_info "Ensuring Prometheus datasource has correct UID..."
    local max_wait=90
    local waited=0
    local datasource_created=false
    
    # Wait for Grafana API to be ready
    log_info "Waiting for Grafana API to be ready (timeout: ${max_wait}s)..."
    while [ $waited -lt $max_wait ]; do
        if curl -s -f -u admin:admin http://localhost:3000/api/health > /dev/null 2>&1; then
            printf "\n"
            log_info "Grafana API is ready ✓"
            break
        fi
        printf "\r${GREEN}[INFO]${NC} Waiting... %3ds/%ds | Grafana API: Connecting..." "$waited" "$max_wait"
        sleep 3
        waited=$((waited + 3))
    done
    
    if [ $waited -ge $max_wait ]; then
        printf "\n"
        log_error "Grafana API not ready after ${max_wait}s, cannot create datasource"
        return 1
    fi
    
    # Check if datasource with UID "Prometheus" already exists
    log_info "Checking for existing Prometheus datasource..."
    local existing=$(curl -s -f -u admin:admin http://localhost:3000/api/datasources/uid/Prometheus 2>/dev/null)
    if echo "$existing" | grep -q '"uid":"Prometheus"'; then
        log_info "✓ Prometheus datasource with UID 'Prometheus' already exists"
        datasource_created=true
    else
        # Get all datasources and try to update existing one or create new
        log_info "No datasource with UID 'Prometheus' found, checking for existing Prometheus datasources..."
        local all_ds=$(curl -s -f -u admin:admin http://localhost:3000/api/datasources 2>/dev/null || echo "[]")
        local prom_ds=$(echo "$all_ds" | python3 -c "
import sys, json
ds = json.load(sys.stdin)
prom = [d for d in ds if d.get('type') == 'prometheus']
if prom:
    print(f\"{prom[0]['id']}|{prom[0].get('uid', '')}|{prom[0].get('name', '')}\")
" 2>/dev/null || echo "")
        
        if [ -n "$prom_ds" ] && [ "$prom_ds" != "None" ]; then
            IFS='|' read -r prom_ds_id prom_ds_uid prom_ds_name <<< "$prom_ds"
            log_info "Found existing Prometheus datasource (ID: ${prom_ds_id}, UID: ${prom_ds_uid}, Name: ${prom_ds_name})"
            
            # Try to update the existing datasource with correct UID
            log_info "Attempting to update existing datasource with UID 'Prometheus'..."
            local update_result=$(curl -s -w "\n%{http_code}" -u admin:admin -X PUT -H "Content-Type: application/json" \
                -d "{\"id\":${prom_ds_id},\"uid\":\"Prometheus\",\"name\":\"Prometheus\",\"type\":\"prometheus\",\"url\":\"http://kube-prometheus-stack-prometheus.sequencer:9090/\",\"access\":\"proxy\",\"isDefault\":true,\"jsonData\":{\"httpMethod\":\"POST\",\"manageAlerts\":true,\"prometheusType\":\"Prometheus\",\"prometheusVersion\":\"2.44.0\"}}" \
                "http://localhost:3000/api/datasources/${prom_ds_id}" 2>&1)
            local update_http_code=$(echo "$update_result" | tail -1)
            local update_response=$(echo "$update_result" | head -n -1)
            
            if [ "$update_http_code" = "200" ] && echo "$update_response" | grep -q '"uid":"Prometheus"'; then
                log_info "✓ Prometheus datasource updated successfully with UID 'Prometheus'"
                datasource_created=true
        else
            log_warn "Failed to update datasource (HTTP ${update_http_code}: ${update_response})"
            log_warn "Datasource may be read-only from Helm. Will continue with existing datasource."
            log_warn "Note: Dashboards may not work if datasource UID is not 'Prometheus'"
            # Don't fail - continue anyway, dashboards might still work
            datasource_created=true
        fi
        else
            # No existing datasource, create new one
            log_info "Creating new Prometheus datasource with UID 'Prometheus'..."
            local result=$(curl -s -w "\n%{http_code}" -u admin:admin -X POST -H "Content-Type: application/json" \
                -d '{"uid":"Prometheus","name":"Prometheus","type":"prometheus","url":"http://kube-prometheus-stack-prometheus.sequencer:9090/","access":"proxy","isDefault":true,"jsonData":{"httpMethod":"POST","manageAlerts":true,"prometheusType":"Prometheus","prometheusVersion":"2.44.0"}}' \
                http://localhost:3000/api/datasources 2>&1)
            local http_code=$(echo "$result" | tail -1)
            local response=$(echo "$result" | head -n -1)
            
            if [ "$http_code" = "200" ] && echo "$response" | grep -q '"uid":"Prometheus"'; then
                log_info "✓ Prometheus datasource created successfully with UID 'Prometheus'"
                datasource_created=true
            else
                log_error "Failed to create Prometheus datasource (HTTP ${http_code})"
                log_error "Response: ${response}"
            fi
        fi
    fi
    
    # Verify datasource was created and is accessible
    if [ "$datasource_created" = true ]; then
        log_info "Verifying datasource connectivity..."
        local health=$(curl -s -f -u admin:admin http://localhost:3000/api/datasources/uid/Prometheus/health 2>/dev/null)
        if echo "$health" | grep -q '"status":"OK"'; then
            log_info "✓ Datasource is healthy and connected to Prometheus"
        else
            log_warn "Datasource created but health check failed: ${health}"
        fi
    else
        log_warn "Could not create/update Prometheus datasource with UID 'Prometheus'"
        log_warn "Dashboards may not work correctly. Attempting to continue anyway..."
        log_warn "You may need to manually update the datasource UID in Grafana UI:"
        log_warn "  1. Go to http://localhost:3000"
        log_warn "  2. Navigate to Connections → Data sources"
        log_warn "  3. Edit the Prometheus datasource and change UID to 'Prometheus' (capital P)"
        # Don't return 1 - continue with dashboard upload anyway
        # The dashboards might work if we update their datasource references later
    fi
    
    # Get the actual datasource UID (may be "prometheus" lowercase from Helm)
    log_info "Getting actual Prometheus datasource UID..."
    local all_ds_final=$(curl -s -f -u admin:admin http://localhost:3000/api/datasources 2>/dev/null || echo "[]")
    local actual_ds_uid=$(echo "$all_ds_final" | python3 -c "
import sys, json
ds = json.load(sys.stdin)
prom = [d for d in ds if d.get('type') == 'prometheus']
if prom:
    print(prom[0].get('uid', 'prometheus'))
else:
    print('prometheus')
" 2>/dev/null)
    
    log_info "Actual Prometheus datasource UID: '${actual_ds_uid}'"
    
    # If dashboards expect "Prometheus" but we have "prometheus", we'll fix it after upload
    if [ "$actual_ds_uid" != "Prometheus" ]; then
        log_warn "Datasource UID is '${actual_ds_uid}' but dashboards expect 'Prometheus'"
        log_info "Will update dashboard datasource references after upload"
    fi
    
    # Delete existing sequencer dashboards before re-uploading
    log_info "Deleting existing sequencer dashboards..."
    local existing_dashboards=$(curl -s -u admin:admin 'http://localhost:3000/api/search?query=&type=dash-db' 2>/dev/null || echo "[]")
    local dashboard_uids=$(echo "$existing_dashboards" | python3 -c "
import sys, json
dashboards = json.load(sys.stdin)
for d in dashboards:
    title = d.get('title', '').lower()
    url = d.get('url', '').lower()
    if 'sequencer' in title or 'sequencer' in url:
        print(d.get('uid', ''))
" 2>/dev/null)
    
    local deleted_dash_count=0
    for dash_uid in $dashboard_uids; do
        if [ -n "$dash_uid" ]; then
            curl -s -X DELETE -u admin:admin "http://localhost:3000/api/dashboards/uid/${dash_uid}" 2>/dev/null
            deleted_dash_count=$((deleted_dash_count + 1))
        fi
    done
    
    if [ $deleted_dash_count -gt 0 ]; then
        log_info "Deleted ${deleted_dash_count} existing sequencer dashboard(s)"
    else
        log_info "No existing sequencer dashboards to delete"
    fi
    
    # Delete existing alert rules in the Sequencer folder before re-uploading
    log_info "Deleting existing alert rules..."
    local folder_uid=$(curl -s -u admin:admin http://localhost:3000/api/folders 2>/dev/null | python3 -c "
import sys, json
folders = json.load(sys.stdin)
for f in folders:
    if f.get('title') == 'Sequencer':
        print(f.get('uid', ''))
        break
" 2>/dev/null)
    
    if [ -n "$folder_uid" ]; then
        local alert_rules=$(curl -s -u admin:admin "http://localhost:3000/api/v1/provisioning/alert-rules" 2>/dev/null)
        local rule_uids=$(echo "$alert_rules" | python3 -c "
import sys, json
rules = json.load(sys.stdin)
for rule in rules:
    if rule.get('folderUID') == '$folder_uid':
        print(rule.get('uid', ''))
" 2>/dev/null)
        
        local deleted_alert_count=0
        for rule_uid in $rule_uids; do
            if [ -n "$rule_uid" ]; then
                curl -s -X DELETE -u admin:admin "http://localhost:3000/api/v1/provisioning/alert-rules/${rule_uid}" 2>/dev/null
                deleted_alert_count=$((deleted_alert_count + 1))
            fi
        done
        
        if [ $deleted_alert_count -gt 0 ]; then
            log_info "Deleted ${deleted_alert_count} existing alert rule(s)"
        else
            log_info "No existing alert rules to delete"
        fi
    else
        log_info "Sequencer folder not found, no alerts to delete"
    fi
    
    # Upload dashboards using existing builder with retries
    # Note: This requires Grafana to be accessible on localhost:3000
    log_info "Uploading dashboards (this may take a moment)..."
    local upload_retries=0
    local upload_success=false
    local upload_output=""
    
    while [ $upload_retries -lt 3 ]; do
        if [ $upload_retries -gt 0 ]; then
            log_info "Retry ${upload_retries}/3: Waiting 5 seconds before retry..."
            sleep 5
        fi
        
        upload_output=$("$venv_python" "${monitoring_dir}/src/main.py" \
            --dev-dashboards-file "${SEQUENCER_ROOT_DIR}/crates/apollo_dashboard/resources/dev_grafana.json" \
            --dev-alerts-file "${SEQUENCER_ROOT_DIR}/crates/apollo_dashboard/resources/dev_grafana_alerts.json" \
            --out-dir /tmp/grafana_builder \
            --env dev \
            --grafana-url "http://localhost:3000" \
            --datasource-uid "${actual_ds_uid}" 2>&1)
        local upload_exit_code=$?
        
        if [ $upload_exit_code -eq 0 ]; then
            log_info "Dashboard upload script completed successfully"
            echo "$upload_output" | grep -i "dashboard\|success\|upload" | head -5 || true
            upload_success=true
            break
        else
            log_warn "Upload attempt ${upload_retries} failed (exit code: ${upload_exit_code})"
            echo "$upload_output" | tail -10 || true
        fi
        
        upload_retries=$((upload_retries + 1))
    done
    
    if [ "$upload_success" = false ]; then
        log_error "Dashboard upload failed after 3 retries"
        log_error "Last output:"
        echo "$upload_output" | tail -20
        log_error "You can upload dashboards manually later via Grafana UI at http://localhost:3000"
        return 1
    fi
    
    # Wait for Grafana to process the dashboards
    log_info "Waiting for Grafana to process dashboards..."
    sleep 8
    
    # Verify dashboards and datasource were actually created (with retries)
        log_info "Verifying dashboards and datasource..."
        local verify_retries=0
        local dashboard_count=0
        local ds_count=0
        
        while [ $verify_retries -lt 8 ]; do
            # Check datasource
            local ds_final=$(curl -s -f -u admin:admin http://localhost:3000/api/datasources 2>/dev/null || echo "[]")
            ds_count=$(echo "$ds_final" | python3 -c "
import sys, json
try:
    ds = json.load(sys.stdin)
    prom_ds = [d for d in ds if d.get('type') == 'prometheus']
    print(len(prom_ds))
    if prom_ds:
        print(f\"Found: {prom_ds[0].get('name', 'N/A')} (UID: {prom_ds[0].get('uid', 'N/A')})\", file=sys.stderr)
except Exception as e:
    print(0)
    print(f\"Error: {e}\", file=sys.stderr)
" 2>/dev/null)
            
            # Check dashboards (search for dash-db type specifically)
            local dashboards=$(curl -s -u admin:admin 'http://localhost:3000/api/search?query=&type=dash-db' 2>/dev/null || echo "[]")
            dashboard_count=$(echo "$dashboards" | python3 -c "
import sys, json
try:
    dash = json.load(sys.stdin)
    sequencer_dash = [d for d in dash if 'sequencer' in d.get('title', '').lower() or 'sequencer' in d.get('url', '').lower()]
    print(len(sequencer_dash))
    if sequencer_dash:
        for d in sequencer_dash:
            print(f\"Found: {d.get('title', 'N/A')}\", file=sys.stderr)
except Exception as e:
    print(0)
    print(f\"Error: {e}\", file=sys.stderr)
" 2>/dev/null)
            
            if [ "$dashboard_count" -gt 0 ] && [ "$ds_count" -gt 0 ]; then
                log_info "✓ Verification successful on attempt $((verify_retries + 1))"
                break
            fi
            
            if [ $((verify_retries % 2)) -eq 0 ]; then
                log_info "Verification attempt $((verify_retries + 1))/8: Found ${ds_count} datasource(s), ${dashboard_count} dashboard(s)..."
            fi
            
            sleep 3
            verify_retries=$((verify_retries + 1))
        done
        
        # Final verification with detailed output
        log_info "Final verification..."
        local ds_final=$(curl -s -f -u admin:admin http://localhost:3000/api/datasources 2>/dev/null || echo "[]")
        local all_dashboards=$(curl -s -u admin:admin 'http://localhost:3000/api/search?query=&type=dash-db' 2>/dev/null || echo "[]")
        
        ds_count=$(echo "$ds_final" | python3 -c "import sys, json; ds = json.load(sys.stdin); print(len([d for d in ds if d.get('type') == 'prometheus']))" 2>/dev/null || echo "0")
        dashboard_count=$(echo "$all_dashboards" | python3 -c "import sys, json; dash = json.load(sys.stdin); print(len([d for d in dash if 'sequencer' in d.get('title', '').lower() or 'sequencer' in d.get('url', '').lower()]))" 2>/dev/null || echo "0")
        
        if [ "$ds_count" -eq 0 ]; then
            log_error "⚠ No Prometheus datasources found in Grafana!"
            log_error "This is a problem. The datasource should exist."
            log_error "Try checking Grafana UI manually: http://localhost:3000"
        else
            log_info "✓ Datasource verified (${ds_count} Prometheus datasource(s))"
            echo "$ds_final" | python3 -c "
import sys, json
try:
    ds = json.load(sys.stdin)
    for d in ds:
        if d.get('type') == 'prometheus':
            print(f\"    - {d.get('name', 'N/A')} (UID: {d.get('uid', 'N/A')})\")
except:
    pass
" 2>/dev/null || true
        fi
        
        if [ "$dashboard_count" -gt 0 ]; then
            log_info "✓ Successfully verified ${dashboard_count} sequencer dashboard(s) in Grafana"
            echo "$all_dashboards" | python3 -c "
import sys, json
try:
    dash = json.load(sys.stdin)
    for d in dash:
        if 'sequencer' in d.get('title', '').lower() or 'sequencer' in d.get('url', '').lower():
            print(f\"    - {d.get('title', 'N/A')}: http://localhost:3000{d.get('url', '')}\")
except:
    pass
" 2>/dev/null || true
            log_info ""
            log_info "✓ Dashboards are ready! Access Grafana at http://localhost:3000"
            
            # Fix datasource UID in dashboards if it doesn't match
            if [ "$actual_ds_uid" != "Prometheus" ]; then
                log_info "Updating dashboard datasource references from 'Prometheus' to '${actual_ds_uid}'..."
                local fixed_count=0
                for dashboard_uid in $(echo "$dashboards" | python3 -c "import sys, json; dash = json.load(sys.stdin); print(' '.join([d.get('uid', '') for d in dash if d.get('uid')]))" 2>/dev/null); do
                    if [ -n "$dashboard_uid" ]; then
                        # Get dashboard JSON
                        local dash_json=$(curl -s -u admin:admin "http://localhost:3000/api/dashboards/uid/${dashboard_uid}" 2>/dev/null)
                        if [ -n "$dash_json" ]; then
                            # Update datasource references
                            local fixed_json=$(echo "$dash_json" | python3 -c "
import sys, json
dash = json.load(sys.stdin)
dashboard = dash.get('dashboard', {})
updated = False

# Update datasource in panels
for panel in dashboard.get('panels', []):
    for target in panel.get('targets', []):
        ds = target.get('datasource', {})
        if isinstance(ds, dict) and ds.get('uid') == 'Prometheus':
            ds['uid'] = '${actual_ds_uid}'
            updated = True

# Update datasource in templating variables
for var in dashboard.get('templating', {}).get('list', []):
    ds = var.get('datasource', {})
    if isinstance(ds, dict) and ds.get('uid') == 'Prometheus':
        ds['uid'] = '${actual_ds_uid}'
        updated = True

if updated:
    print(json.dumps(dash))
else:
    print('')
" 2>/dev/null)
                            
                            if [ -n "$fixed_json" ]; then
                                # Update dashboard
                                curl -s -u admin:admin -X POST -H "Content-Type: application/json" \
                                    -d "$fixed_json" \
                                    "http://localhost:3000/api/dashboards/db" > /dev/null 2>&1 && {
                                    fixed_count=$((fixed_count + 1))
                                }
                            fi
                        fi
                    fi
                done
                if [ "$fixed_count" -gt 0 ]; then
                    log_info "✓ Updated datasource references in ${fixed_count} dashboard(s)"
                fi
            fi
            
            # Show dashboard URLs
            echo "$dashboards" | python3 -c "import sys, json; dash = json.load(sys.stdin); [print(f\"    - {d.get('title', 'N/A')}: http://localhost:3000{d.get('url', '')}\") for d in dash]" 2>/dev/null || true
            log_info ""
            log_info "If dashboards don't appear in your browser, try:"
            log_info "  1. Hard refresh: Ctrl+Shift+R (Linux/Windows) or Cmd+Shift+R (Mac)"
            log_info "  2. Clear browser cache for localhost:3000"
            log_info "  3. Open in incognito/private window"
        else
            log_warn "Dashboard upload completed but verification found 0 dashboards"
            log_warn "This might be a timing issue. Try:"
            log_warn "  1. Wait 10-20 seconds and refresh Grafana"
            log_warn "  2. Check Grafana UI: http://localhost:3000"
            log_warn "  3. Run: ./deploy.sh install-monitoring (to retry upload)"
        fi
}

# Upload Grafana alert rules only (for quick iteration on alerts)
# Usage: upload_alerts [dev|testnet|mainnet]
upload_alerts() {
    # Use the base alerts file path
    local alerts_file="${SEQUENCER_ROOT_DIR}/crates/apollo_dashboard/resources/dev_grafana_alerts.json"
    
    verify_k3d_cluster
    log_info "Uploading Grafana alert rules (env: ${alert_env})..."
    
    # Wait for Grafana to be ready
    kubectl wait --for=condition=available \
        --timeout=2m \
        deployment/kube-prometheus-stack-grafana \
        -n "${NAMESPACE}" 2>/dev/null || {
        log_warn "Grafana not ready, skipping alert upload"
        return 1
    }
    
    # Use the same virtual environment as the simulator (or create one if it doesn't exist)
    local venv_dir="${SCRIPT_DIR}/.venv"
    if [ ! -d "$venv_dir" ]; then
        log_info "Creating virtual environment for alert upload..."
        python3 -m venv "$venv_dir" || {
            log_error "Failed to create virtual environment"
            return 1
        }
    fi
    
    # Install Python dependencies in venv
    local monitoring_dir="${SEQUENCER_ROOT_DIR}/deployments/monitoring"
    local venv_pip="${venv_dir}/bin/pip"
    local venv_python="${venv_dir}/bin/python"
    
    if [ -f "${monitoring_dir}/src/requirements.txt" ]; then
        "$venv_pip" install -q -r "${monitoring_dir}/src/requirements.txt" 2>/dev/null || {
            log_error "Failed to install Python dependencies"
            return 1
        }
    fi
    
    # Wait for Grafana API to be ready
    log_info "Waiting for Grafana API to be ready (timeout: 60s)..."
    local max_wait=60
    local waited=0
    while [ $waited -lt $max_wait ]; do
        if curl -s -f -u admin:admin http://localhost:3000/api/health > /dev/null 2>&1; then
            printf "\n"
            log_info "Grafana API is ready ✓"
            break
        fi
        printf "\r${GREEN}[INFO]${NC} Waiting... %3ds/%ds | Grafana API: Connecting..." "$waited" "$max_wait"
        sleep 3
        waited=$((waited + 3))
    done
    
    if [ $waited -ge $max_wait ]; then
        printf "\n"
        log_error "Grafana API not ready after ${max_wait}s"
        return 1
    fi
    
    # Get the actual datasource UID
    local all_ds=$(curl -s -f -u admin:admin http://localhost:3000/api/datasources 2>/dev/null || echo "[]")
    local actual_ds_uid=$(echo "$all_ds" | python3 -c "
import sys, json
ds = json.load(sys.stdin)
prom = [d for d in ds if d.get('type') == 'prometheus']
if prom:
    print(prom[0].get('uid', 'prometheus'))
else:
    print('prometheus')
" 2>/dev/null)
    
    log_info "Using Prometheus datasource UID: '${actual_ds_uid}'"
    
    # Delete existing alert rules in the Sequencer folder before re-uploading
    log_info "Deleting existing alert rules..."
    local folder_uid=$(curl -s -u admin:admin http://localhost:3000/api/folders 2>/dev/null | python3 -c "
import sys, json
folders = json.load(sys.stdin)
for f in folders:
    if f.get('title') == 'Sequencer':
        print(f.get('uid', ''))
        break
" 2>/dev/null)
    
    if [ -n "$folder_uid" ]; then
        # Get all alert rules and delete them
        local alert_rules=$(curl -s -u admin:admin "http://localhost:3000/api/v1/provisioning/alert-rules" 2>/dev/null)
        local rule_uids=$(echo "$alert_rules" | python3 -c "
import sys, json
rules = json.load(sys.stdin)
for rule in rules:
    if rule.get('folderUID') == '$folder_uid':
        print(rule.get('uid', ''))
" 2>/dev/null)
        
        local deleted_count=0
        for rule_uid in $rule_uids; do
            if [ -n "$rule_uid" ]; then
                curl -s -X DELETE -u admin:admin "http://localhost:3000/api/v1/provisioning/alert-rules/${rule_uid}" 2>/dev/null
                deleted_count=$((deleted_count + 1))
            fi
        done
        
        if [ $deleted_count -gt 0 ]; then
            log_info "Deleted ${deleted_count} existing alert rule(s)"
        else
            log_info "No existing alert rules to delete"
        fi
    else
        log_info "Sequencer folder not found, no alerts to delete"
    fi
    
    # Upload alert rules only (no dashboards file)
    # The Python script will resolve to dev_grafana_alerts.json based on env
    # dev->mainnet, testnet->testnet, mainnet->mainnet
    local resolved_suffix
    case "$alert_env" in
        dev) resolved_suffix="mainnet" ;;
        testnet) resolved_suffix="testnet" ;;
        mainnet) resolved_suffix="mainnet" ;;
    esac
    log_info "Uploading alert rules (env: ${alert_env} -> dev_grafana_alerts.json)"
    local upload_output=""
    upload_output=$("$venv_python" "${monitoring_dir}/src/main.py" \
        --dev-alerts-file "$alerts_file" \
        --out-dir /tmp/grafana_builder \
        --env "$alert_env" \
        --grafana-url "http://localhost:3000" \
        --datasource-uid "${actual_ds_uid}" 2>&1)
    local upload_exit_code=$?
    
    if [ $upload_exit_code -eq 0 ]; then
        # Count how many alerts were uploaded
        local uploaded_count=$(echo "$upload_output" | grep -c "uploaded to Grafana successfully" || echo "0")
        log_info "✓ ${uploaded_count} alert rules uploaded successfully"
        # Show summary of what was uploaded
        echo "$upload_output" | grep "uploaded to Grafana successfully" | tail -5
        if [ "$uploaded_count" -gt 5 ]; then
            log_info "  ... and $((uploaded_count - 5)) more"
        fi
        log_info ""
        log_info "View alerts at: http://localhost:3000/alerting/list"
    else
        log_error "Alert upload failed (exit code: ${upload_exit_code})"
        echo "$upload_output" | tail -20
        return 1
    fi
}

# Main deployment
deploy_up() {
    log_info "Starting deployment..."
    
    check_prerequisites
    create_cluster
    
    # Step 1: Install monitoring stack first (Grafana/Prometheus)
    # This allows monitoring to collect metrics from the start
    if [ "$SKIP_INSTALL_MONITORING" = true ]; then
        log_info "Skipping monitoring installation (--skip-install-monitoring flag set)"
    else
        install_monitoring || {
            log_warn "Monitoring installation had issues, but continuing with deployment..."
        }
    fi
    
    if [ "$SKIP_DOCKER_BUILD" = true ]; then
        log_info "Skipping Docker image build (--skip-docker-build flag set)"
    else
        build_images
    fi
    
    # Step 2: Deploy Anvil (required for l1 service and simulator)
    deploy_anvil
    extract_anvil_addresses || {
        log_warn "Could not extract Anvil addresses, simulator may not work"
    }
    
    # Step 3: Generate and deploy dummy services
    generate_cdk8s_manifests
    apply_dummy_services
    
    # Step 4: Generate state (after dummy services are ready)
    if [ "$SKIP_BUILD_RUST_BINARIES" = true ]; then
        log_info "Skipping Rust binary build (--skip-build-rust-binaries flag set)"
    else
        build_binaries
    fi
    generate_state
    
    # Step 5: Generate and deploy sequencer
    generate_sequencer_manifests
    apply_sequencer
    
    # Step 6: Copy state and restart sequencer core
    wait_and_copy_state
    
    # Step 7: Run simulator to test transactions
    run_simulator || {
        log_warn "Simulator test failed or skipped"
    }
    
    log_info ""
    log_info "Deployment complete!"
    log_info ""
    log_info "Deployment steps completed:"
    log_info "  ✅ Anvil deployed"
    log_info "  ✅ Dummy services deployed"
    log_info "  ✅ State generated and copied to sequencer"
    log_info "  ✅ Sequencer deployed and running"
    log_info "  ✅ Simulator test completed"
    log_info ""
    log_info "Access points:"
    log_info "  - Grafana: http://localhost:3000 (via Ingress, no port-forward needed)"
    log_info "  - Prometheus: http://localhost:9090 (via NodePort, no port-forward needed)"
    log_info ""
    log_info "If dashboards don't appear in your browser, try:"
    log_info "  1. Hard refresh: Ctrl+Shift+R (Linux/Windows) or Cmd+Shift+R (Mac)"
    log_info "  2. Clear browser cache for localhost:3000"
    log_info "  3. Open in incognito/private window"
    log_info ""
    log_info "Useful commands:"
    log_info "  - Check status: ./deploy.sh status"
    log_info "  - View logs: ./deploy.sh logs"
    log_info "  - Rerun simulator test: ./deploy.sh rerun-simulator"
}

# Teardown
deploy_down() {
    # Verify cluster exists before deleting (safety check)
    if ! k3d cluster list | grep -q "${CLUSTER_NAME}"; then
        log_warn "Cluster ${CLUSTER_NAME} does not exist, nothing to tear down"
        return 0
    fi
    
    # Verify we're using the correct context (safety check)
    local current_context=$(kubectl config current-context 2>/dev/null || echo "")
    local expected_context="k3d-${CLUSTER_NAME}"
    
    if [ -n "$current_context" ] && [ "$current_context" != "$expected_context" ]; then
        log_error "Current kubectl context is '${current_context}', but expected '${expected_context}'"
        log_error "This script only works with the local k3d cluster '${CLUSTER_NAME}'"
        log_error "To switch to the correct context, run:"
        log_error "  k3d kubeconfig merge ${CLUSTER_NAME} --kubeconfig-switch-context"
        log_error ""
        log_error "For safety, this script will not delete a different cluster."
        exit 1
    fi
    
    log_info "Tearing down deployment..."
    
    delete_cluster
    
    log_info "Teardown complete"
}

# Show logs
show_logs() {
    verify_k3d_cluster
    # Try to find pods based on layout
    local selector
    if [ "$SEQUENCER_LAYOUT" == "hybrid" ]; then
        # For hybrid, show core service logs (main sequencer service)
        selector="service=sequencer-core"
    else
        selector="app=sequencer"
    fi
    
    if kubectl get pods -n "${NAMESPACE}" -l "${selector}" &>/dev/null; then
        kubectl logs -f -l "${selector}" -n "${NAMESPACE}" || {
            log_warn "Failed to get logs, showing all pods:"
            kubectl get pods -n "${NAMESPACE}"
        }
    else
        log_warn "No sequencer pods found with selector ${selector}, showing all pods:"
        kubectl get pods -n "${NAMESPACE}"
    fi
}

# Show status
show_status() {
    verify_k3d_cluster
    log_info "Cluster status:"
    k3d cluster list
    
    log_info ""
    log_info "Namespace resources:"
    kubectl get all,jobs,pvc -n "${NAMESPACE}"
}

# Main command handler
main() {
    # Parse flags
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --skip-docker-build)
                SKIP_DOCKER_BUILD=true
                shift
                ;;
            --skip-install-monitoring)
                SKIP_INSTALL_MONITORING=true
                shift
                ;;
            --skip-build-rust-binaries)
                SKIP_BUILD_RUST_BINARIES=true
                shift
                ;;
            -*)
                log_error "Unknown flag: $1"
                exit 1
                ;;
            *)
                break
                ;;
        esac
    done
    
    case "${1:-}" in
        up)
            deploy_up
            ;;
        down)
            deploy_down
            ;;
        create-cluster)
            create_cluster
            ;;
        build)
            check_prerequisites
            build_images
            prompt_deploy_after_build
            ;;
        restart)
            check_prerequisites
            rollout_restart_all
            ;;
        build-binaries)
            check_prerequisites
            build_binaries
            ;;
        generate-state)
            check_prerequisites
            build_binaries
            generate_state
            ;;
        copy-state)
            check_prerequisites
            copy_state_and_restart
            ;;
        install-monitoring)
            check_prerequisites
            install_monitoring
            ;;
        update-dashboards)
            check_prerequisites
            upload_dashboards
            ;;
        update-alerts)
            check_prerequisites
            upload_alerts "${2:-dev}"  # Default to dev, accepts: dev, testnet, mainnet
            ;;
        rerun-simulator)
            check_prerequisites
            rerun_simulator
            ;;
        prepare-box)
            prepare_box
            ;;
        logs)
            show_logs
            ;;
        status)
            show_status
            ;;
        *)
            echo "Usage: $0 [flags] {up|down|create-cluster|build|restart|build-binaries|generate-state|copy-state|install-monitoring|update-dashboards|update-alerts|rerun-simulator|prepare-box|logs|status}"
            echo ""
            echo "Flags:"
            echo "  --skip-docker-build       - Skip Docker image build (use existing images)"
            echo "  --skip-install-monitoring - Skip Prometheus/Grafana installation"
            echo "  --skip-build-rust-binaries - Skip Rust binary compilation"
            echo ""
            echo "Commands:"
            echo "  up                      - Full deployment: Anvil, dummy services, state, sequencer, simulator test"
            echo "  down                    - Tear down cluster and clean up"
            echo "  create-cluster          - Create k3d cluster only (without deploying services)"
            echo "  build                   - Rebuild Docker images and optionally deploy them"
            echo "  restart                 - Restart all pods to pick up new images"
            echo "  build-binaries          - Build Rust binaries (sequencer_node_setup, sequencer_simulator)"
            echo "  generate-state          - Build binaries and generate initial sequencer state"
            echo "  copy-state              - Copy generated state to sequencer pod and restart it"
            echo "  install-monitoring      - Install/retry Prometheus/Grafana stack installation"
            echo "  update-dashboards       - Upload/update Grafana dashboards and alert rules"
            echo "  update-alerts [ENV]     - Upload/update Grafana alert rules"
            echo "                            ENV: dev (uses mainnet alerts), testnet, mainnet; default: dev"
            echo "  rerun-simulator         - Wipe state, regenerate, copy to pod, run simulator test"
            echo "  prepare-box             - Prepare VM for Vagrant box: build images, binaries, cleanup"
            echo "  logs                    - Follow sequencer logs"
            echo "  status                  - Show pod/service status"
            exit 1
            ;;
    esac
}

main "$@"
