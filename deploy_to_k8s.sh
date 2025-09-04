#!/bin/bash

# Kubernetes Deployment Script for Sequencer Node
# This script helps deploy your sequencer (with concurrent flush improvements!) to k8s

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_step() {
    echo -e "${BLUE}📋 Step $1: $2${NC}"
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

print_info() {
    echo -e "${BLUE}ℹ️  $1${NC}"
}

echo "🚀 Kubernetes Deployment for Sequencer Node"
echo "============================================"
echo "This will deploy your sequencer with concurrent flush improvements to k8s"
echo ""

# Check prerequisites
print_step "0" "Checking Prerequisites"

# Check if kubectl is installed
if ! command -v kubectl &> /dev/null; then
    print_error "kubectl is not installed. Please install it first."
    echo "Install guide: https://kubernetes.io/docs/tasks/tools/"
    exit 1
fi

# Check if kubectx is installed
if ! command -v kubectx &> /dev/null; then
    print_warning "kubectx is not installed. You'll need to use kubectl config manually."
    echo "Install: https://github.com/ahmetb/kubectx"
fi

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ] || [ ! -d "crates/apollo_storage" ]; then
    print_error "Please run this script from the sequencer root directory"
    exit 1
fi

print_success "Prerequisites checked"

# Step 1: Connect to cluster
print_step "1" "Connecting to Kubernetes Cluster"
echo "Attempting to connect to the GKE cluster..."

if command -v kubectx &> /dev/null; then
    if kubectx gke_starkware-dev_us-central1_sequencer-dev; then
        print_success "Connected to cluster using kubectx"
    else
        print_warning "kubectx failed, trying gcloud..."
        if gcloud container clusters get-credentials sequencer-dev \
            --region us-central1 \
            --project starkware-dev; then
            print_success "Connected to cluster using gcloud"
        else
            print_error "Failed to connect to cluster. Check your gcloud authentication."
            exit 1
        fi
    fi
else
    if gcloud container clusters get-credentials sequencer-dev \
        --region us-central1 \
        --project starkware-dev; then
        print_success "Connected to cluster using gcloud"
    else
        print_error "Failed to connect to cluster. Check your gcloud authentication."
        exit 1
    fi
fi

# Step 2: Update configuration files
print_step "2" "Updating Configuration Files"

print_info "Backing up original files..."
cp crates/apollo_deployments/resources/testing_secrets.json crates/apollo_deployments/resources/testing_secrets.json.backup
cp crates/apollo_deployments/resources/deployments/testing/deployment_config_consolidated.json crates/apollo_deployments/resources/deployments/testing/deployment_config_consolidated.json.backup
cp scripts/system_tests/config_secrets_injector.py scripts/system_tests/config_secrets_injector.py.backup

print_info "Updating testing_secrets.json..."
cat > crates/apollo_deployments/resources/testing_secrets.json << 'EOF'
{
    "base_layer_config.node_url": "http://anvil.sequencer-dev.sw-dev.io",
    "consensus_manager_config.network_config.secret_key": "0x0101010101010101010101010101010101010101010101010101010101010101",
    "l1_endpoint_monitor_config.ordered_l1_endpoint_urls": "http://anvil.sequencer-dev.sw-dev.io",
    "l1_gas_price_provider_config.eth_to_strk_oracle_config.url_header_list": "http://dummy-eth2strk-oracle.sequencer-dev.sw-dev.io",
    "mempool_p2p_config.network_config.secret_key": "0x0101010101010101010101010101010101010101010101010101010101010101",
    "recorder_url": "http://dummy-recorder.dummy-recorder.sw-dev.io",
    "state_sync_config.central_sync_client_config.central_source_config.http_headers": "",
    "state_sync_config.network_config.secret_key": "0x0101010101010101010101010101010101010101010101010101010101010101"
}
EOF

print_info "Updating deployment_config_consolidated.json..."
cat > crates/apollo_deployments/resources/deployments/testing/deployment_config_consolidated.json << 'EOF'
{
  "application_config_subdir": "crates/apollo_deployments/resources/",
  "services": [
    {
      "name": "Node",
      "controller": "StatefulSet",
      "config_paths": [
        "app_configs/base_layer_config.json",
        "app_configs/batcher_config.json",
        "app_configs/class_manager_config.json",
        "app_configs/consensus_manager_config.json",
        "app_configs/revert_config.json",
        "app_configs/versioned_constants_overrides_config.json",
        "app_configs/validate_resource_bounds_config.json",
        "app_configs/gateway_config.json",
        "app_configs/http_server_config.json",
        "app_configs/l1_endpoint_monitor_config.json",
        "app_configs/l1_gas_price_provider_config.json",
        "app_configs/l1_gas_price_scraper_config.json",
        "app_configs/l1_provider_config.json",
        "app_configs/l1_scraper_config.json",
        "app_configs/mempool_config.json",
        "app_configs/mempool_p2p_config.json",
        "app_configs/monitoring_endpoint_config.json",
        "app_configs/sierra_compiler_config.json",
        "app_configs/state_sync_config.json",
        "deployments/testing/deployment_config_override.json",
        "deployments/testing/consolidated.json",
        "services/consolidated/node.json"
      ],
      "ingress": null,
      "k8s_service_config": null,
      "autoscale": false,
      "replicas": 1,
      "storage": 2000,
      "toleration": "starkdex-service",
      "resources": {
        "requests": {
          "cpu": 4,
          "memory": 50
        },
        "limits": {
          "cpu": 8,
          "memory": 100
        }
      },
      "external_secret": null,
      "anti_affinity": false,
      "update_strategy_type": "RollingUpdate",
      "ports": {
        "ConsensusP2P": 53080,
        "HttpServer": 8080,
        "MempoolP2p": 53200,
        "MonitoringEndpoint": 8082
      }
    }
  ]
}
EOF

print_info "Updating config_secrets_injector.py..."
# Create a temporary Python script to make the changes
python3 << 'EOF'
import re

# Read the file
with open('scripts/system_tests/config_secrets_injector.py', 'r') as f:
    content = f.read()

# Make the replacements
content = re.sub(
    r'config_dir_path = Path\(os\.environ\["GITHUB_WORKSPACE"\]\) / config_dir',
    'config_dir_path = config_dir',
    content
)

content = re.sub(
    r'full_path = config_dir_path / cfg_file\s+if not full_path\.is_file\(\):\s+print\(f"❌ Config file \{full_path\} not found\. Available files in \{config_dir_path\}:"\)\s+for file in config_dir_path\.iterdir\(\):\s+print\(" -", file\.name\)\s+sys\.exit\(1\)',
    'full_path = f"{config_dir_path}/{cfg_file}"',
    content,
    flags=re.DOTALL
)

# Write back
with open('scripts/system_tests/config_secrets_injector.py', 'w') as f:
    f.write(content)
EOF

print_success "Configuration files updated"

# Step 3: Inject secrets
print_step "3" "Injecting Secrets"
python3 scripts/system_tests/config_secrets_injector.py --deployment_config_path "crates/apollo_deployments/resources/deployments/testing/deployment_config_consolidated.json"
print_success "Secrets injected"

# Step 4: Build Docker image
print_step "4" "Building Docker Image"
print_info "You need to build the Docker image on GitHub Actions:"
echo ""
echo "🌐 Go to: https://github.com/starkware-libs/sequencer/actions"
echo "1. Click 'Run workflow'"
echo "2. Select your branch"
echo "3. Start the job"
echo "4. Wait ~20 minutes for completion"
echo "5. Open 'docker build push' → 'Build and Push Docker image'"
echo "6. Look for a line like:"
echo "   pushing manifest for ghcr.io/starkware-libs/sequencer/sequencer:YOUR-BRANCH-NAME-HASH@sha256:..."
echo "7. Copy the image tag (the part before @sha256)"
echo ""
read -p "Press Enter when the Docker build is complete and you have the image tag..."

read -p "Enter the image tag (e.g., ghcr.io/starkware-libs/sequencer/sequencer:your-branch-name-hash): " IMAGE_TAG

if [ -z "$IMAGE_TAG" ]; then
    print_error "Image tag is required"
    exit 1
fi

print_success "Image tag: $IMAGE_TAG"

# Step 5: Generate Kubernetes manifests
print_step "5" "Generating Kubernetes Manifests"

read -p "Enter a namespace for your deployment (e.g., your-name-test): " NAMESPACE

if [ -z "$NAMESPACE" ]; then
    print_error "Namespace is required"
    exit 1
fi

cd deployments/sequencer

print_info "Installing dependencies..."
if ! command -v cdk8s &> /dev/null; then
    npm install -g cdk8s-cli
fi

cdk8s import

if ! command -v pipenv &> /dev/null; then
    pip install pipenv
fi

python3.10 -m pipenv install

print_info "Generating manifests..."
FULL_PATH=$(pwd | sed 's|/deployments/sequencer||')
cdk8s synth --app "pipenv run python main.py \
  --namespace $NAMESPACE \
  --deployment-config-file '$FULL_PATH/crates/apollo_deployments/resources/deployments/testing/deployment_config_consolidated.json' \
  --deployment-image-tag $IMAGE_TAG"

print_success "Kubernetes manifests generated"
cd ../..

# Step 6: Deploy to cluster
print_step "6" "Deploying to Cluster"

print_info "Creating namespace..."
kubectl create namespace $NAMESPACE || print_info "Namespace already exists"

print_info "Applying manifests..."
kubectl apply -R -f deployments/sequencer/dist

print_success "Deployment applied"

# Step 7: Verify deployment
print_step "7" "Verifying Deployment"

if command -v kubens &> /dev/null; then
    kubens $NAMESPACE
else
    kubectl config set-context --current --namespace=$NAMESPACE
fi

print_info "Waiting for pod to start..."
kubectl wait --for=condition=Ready pod -l app=sequencer-node --timeout=300s

print_success "Pod is ready!"

echo ""
print_success "🎉 Deployment Complete!"
echo ""
echo "📊 Your sequencer with concurrent flush improvements is now running!"
echo "🔧 Useful commands:"
echo "   kubectl get pods                    # See pod status"
echo "   kubectl logs <pod-name>             # View logs"
echo "   kubectl logs <pod-name> -f          # Follow logs"
echo "   kubectl describe pod <pod-name>     # Pod details"
echo ""
echo "🌐 Monitoring endpoints will be available at:"
echo "   Port 8082: Metrics endpoint (for Grafana/Prometheus)"
echo "   Port 8080: HTTP server"
echo ""
echo "📈 The concurrent flush metrics will be available at:"
echo "   /monitoring/metrics endpoint"
echo "   Look for 'storage_file_handler_flush_latency_seconds' metric"
echo ""

# Show current pods
kubectl get pods

echo ""
print_info "To restore original configuration files, run:"
echo "   mv crates/apollo_deployments/resources/testing_secrets.json.backup crates/apollo_deployments/resources/testing_secrets.json"
echo "   mv crates/apollo_deployments/resources/deployments/testing/deployment_config_consolidated.json.backup crates/apollo_deployments/resources/deployments/testing/deployment_config_consolidated.json"
echo "   mv scripts/system_tests/config_secrets_injector.py.backup scripts/system_tests/config_secrets_injector.py"
