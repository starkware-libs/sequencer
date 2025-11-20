#!/bin/bash
# Kubernetes Deployment Script for Batching Performance Test
# Adapted from deploy_to_k8s.sh- simplified for test execution
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Configuration
NAMESPACE="${K8S_NAMESPACE:-batching-test}"
BLOCKS_TO_SYNC="${BLOCKS_TO_SYNC:-5000}"
BATCH_SIZE="${BATCH_SIZE:-100}"
IMAGE_NAME="${DOCKER_IMAGE:-}"  # Can be set via env var to skip prompts

# Check if required tools are installed
check_prerequisites() {
    print_status "Checking prerequisites..."
    if ! command -v kubectl &> /dev/null; then
        print_error "kubectl is not installed. Please install kubectl first."
        exit 1
    fi
    if ! command -v gcloud &> /dev/null; then
        print_warning "gcloud CLI not found. You may need it for GKE clusters."
    fi
    
    # Check if test scripts exist
    if [ ! -f "../../test_batching.sh" ]; then
        print_error "test_batching.sh not found. Please run from deployments/batching-test/"
        exit 1
    fi
    
    print_success "Prerequisites check completed (Docker not required - using pre-built images)"
}

# Check kubectl connection
check_k8s_connection() {
    print_status "Checking Kubernetes connection..."
    if ! kubectl cluster-info &> /dev/null; then
        print_error "Cannot connect to Kubernetes cluster."
        print_status "Please ensure you're connected to a cluster:"
        echo "  - For GKE: gcloud container clusters get-credentials CLUSTER_NAME --zone ZONE"
        echo "  - For local: kubectl config use-context docker-desktop"
        echo "  - Check contexts: kubectl config get-contexts"
        exit 1
    fi
    CURRENT_CONTEXT=$(kubectl config current-context)
    print_success "Connected to cluster: $CURRENT_CONTEXT"
}

# Get pre-built Docker image from GitHub Container Registry
get_docker_image() {
    print_status "Using pre-built Docker image from GitHub Container Registry"
    echo ""
    echo "✓ This deployment uses images already built via GitHub Actions."
    echo "✓ No local Docker build will occur - your disk space is safe!"
    echo "✓ All storage (100GB+) will be in the K8s cluster, not your machine!"
    echo ""
    
    # Check if IMAGE_NAME was provided via environment variable
    if [ ! -z "$IMAGE_NAME" ]; then
        print_success "Using image from DOCKER_IMAGE environment variable: $IMAGE_NAME"
        return
    fi
    
    # Interactive prompts
    echo "Enter your pre-built image from GitHub:"
    echo ""
    echo "Format: ghcr.io/ORGANIZATION/REPOSITORY:TAG"
    echo "Example: ghcr.io/starkware-libs/sequencer:dean-k8s_batching_test"
    echo ""
    
    # Option 1: Full image URL
    read -p "Enter full image URL (or press Enter to build it step-by-step): " FULL_IMAGE
    
    if [ ! -z "$FULL_IMAGE" ]; then
        IMAGE_NAME="$FULL_IMAGE"
    else
        # Option 2: Step-by-step
        echo ""
        echo "Let's build the image URL step-by-step:"
        echo ""
        
        # Get registry URL
        read -p "Registry URL (default: ghcr.io): " REGISTRY_URL
        REGISTRY_URL=${REGISTRY_URL:-ghcr.io}
        
        # Get organization/owner
        read -p "Organization/Owner (e.g., starkware-libs): " ORG_NAME
        if [ -z "$ORG_NAME" ]; then
            print_error "Organization/Owner is required"
            exit 1
        fi
        
        # Get repository name
        read -p "Repository name (default: sequencer): " REPO_NAME
        REPO_NAME=${REPO_NAME:-sequencer}
        
        # Get image tag
        echo ""
        echo "Common tag formats:"
        echo "  - Branch name: dean-k8s_batching_test"
        echo "  - With commit: dean-k8s_batching_test-abc1234"
        echo "  - SHA: sha-abc1234567"
        echo ""
        read -p "Image tag: " IMAGE_TAG
        if [ -z "$IMAGE_TAG" ]; then
            print_error "Image tag is required"
            exit 1
        fi
        
        # Construct full image name
        IMAGE_NAME="$REGISTRY_URL/$ORG_NAME/$REPO_NAME:$IMAGE_TAG"
    fi
    
    echo ""
    print_success "Will use image: $IMAGE_NAME"
    echo ""
    print_warning "IMPORTANT: No local disk space will be used!"
    print_warning "All 100GB+ databases will be stored in the K8s cluster."
    echo ""
}

# Create namespace
create_namespace() {
    print_status "Creating namespace: $NAMESPACE"
    kubectl create namespace $NAMESPACE --dry-run=client -o yaml | kubectl apply -f -
    print_success "Namespace ready"
}

# Create ConfigMap with test scripts
create_test_scripts_configmap() {
    print_status "Creating ConfigMap with test scripts..."
    
    kubectl create configmap batching-test-scripts \
        --from-file=test_batching.sh=../../test_batching.sh \
        --from-file=analyze_batching_test.sh=../../analyze_batching_test.sh \
        --namespace=$NAMESPACE \
        --dry-run=client -o yaml | kubectl apply -f -
    
    print_success "Test scripts ConfigMap created"
}

# Create ConfigMap with all necessary config files
create_config_files_configmap() {
    print_status "Creating ConfigMap with node config files..."
    
    # Go to repo root
    cd ../..
    
    # Create minimal node config for testing (only state_sync enabled)
    cat > /tmp/minimal_node_config.json << 'CONFIGEOF'
{
  "chain_id": "SN_MAIN",
  "validator_id": "0x0",
  "eth_fee_token_address": "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7",
  "strk_fee_token_address": "0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d",
  "starknet_url": "https://feeder.alpha-mainnet.starknet.io/",
  "versioned_constants_overrides.invoke_tx_max_n_steps": 10000000,
  "versioned_constants_overrides.max_n_events": 1000,
  "versioned_constants_overrides.max_recursion_depth": 50,
  "versioned_constants_overrides.validate_max_n_steps": 1000000
}
CONFIGEOF
    
    # Create ConfigMap with ALL config files (easier than discovering required fields one by one)
    kubectl create configmap sequencer-configs \
        --from-file=crates/apollo_deployments/resources/app_configs/ \
        --from-file=minimal_node_config.json=/tmp/minimal_node_config.json \
        --from-file=mainnet_deployment=crates/apollo_deployments/resources/deployments/mainnet/deployment_config_override.json \
        --from-file=mainnet_hybrid=crates/apollo_deployments/resources/deployments/mainnet/hybrid_0.json \
        --from-file=node_config=crates/apollo_deployments/resources/services/consolidated/node.json \
        --from-file=mainnet_secrets.json=crates/apollo_deployments/resources/mainnet_secrets.json \
        --namespace=$NAMESPACE \
        --dry-run=client -o yaml | kubectl apply -f -
    
    # Return to deployment directory
    cd deployments/batching-test
    
    print_success "Config files ConfigMap created"
}

# Create storage (PVCs)
create_storage() {
    print_status "Creating storage (PVCs)..."
    
    cat <<EOF | kubectl apply -f -
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: test-results-pvc
  namespace: $NAMESPACE
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 10Gi
---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: sequencer-database-pvc
  namespace: $NAMESPACE
spec:
  accessModes:
    - ReadWriteOnce
  storageClassName: premium-rwo
  resources:
    requests:
      storage: 100Gi
EOF
    
    print_success "PVCs created"
}

# Deploy test job
deploy_test_job() {
    print_status "Deploying batching test job..."
    print_status "Configuration:"
    echo "  Namespace: $NAMESPACE"
    echo "  Image: $IMAGE_NAME"
    echo "  Blocks to sync: $BLOCKS_TO_SYNC"
    echo "  Batch size: $BATCH_SIZE"
    echo ""
    
    # Create job with substituted values
    sed -e "s|namespace: default|namespace: $NAMESPACE|g" \
        -e "s|image: ghcr.io/starkware-libs/sequencer/sequencer:latest|image: $IMAGE_NAME|g" \
        -e "s|value: \"5000\"|value: \"$BLOCKS_TO_SYNC\"|g" \
        -e "s|value: \"100\"|value: \"$BATCH_SIZE\"|g" \
        test-job.yaml | kubectl apply -f -
    
    print_success "Test job deployed"
}

# Monitor test execution
monitor_test() {
    print_status "Monitoring test execution..."
    
    echo ""
    print_status "Waiting for pod to start..."
    sleep 5
    
    # Get pod name
    POD_NAME=$(kubectl get pods -n $NAMESPACE -l app=sequencer-batching-test -o jsonpath='{.items[0].metadata.name}' 2>/dev/null)
    
    if [ -z "$POD_NAME" ]; then
        print_warning "Pod not found yet. Check status with:"
        echo "  kubectl get pods -n $NAMESPACE"
        return
    fi
    
    print_success "Pod started: $POD_NAME"
    echo ""
    print_status "Streaming logs (Ctrl+C to stop, test continues)..."
    echo ""
    
    # Stream logs
    kubectl logs -f $POD_NAME -n $NAMESPACE || true
}

# Get test results
get_results() {
    print_status "Collecting test results..."
    
    # Wait for job completion
    print_status "Waiting for job to complete (timeout: 3 hours)..."
    if kubectl wait --for=condition=complete --timeout=3h job/sequencer-batching-test -n $NAMESPACE; then
        print_success "Test completed successfully!"
    else
        print_warning "Job did not complete (may have failed or timed out)"
        kubectl get job sequencer-batching-test -n $NAMESPACE
    fi
    
    # Get logs
    print_status "Saving logs to ./results/"
    mkdir -p results
    kubectl logs job/sequencer-batching-test -n $NAMESPACE > results/test_results.log
    
    print_success "Results saved to ./results/test_results.log"
    
    # Show summary
    echo ""
    print_status "Test Summary:"
    tail -50 results/test_results.log | grep -A 20 "RESULTS" || echo "Check full log for results"
}

# Cleanup function
cleanup_deployment() {
    print_status "Cleaning up test deployment..."
    
    # Delete job
    kubectl delete job sequencer-batching-test -n $NAMESPACE --ignore-not-found=true
    kubectl delete configmap batching-test-scripts -n $NAMESPACE --ignore-not-found=true
    
    echo ""
    read -p "Delete PVCs (will delete test data)? (y/N): " DELETE_PVC
    if [[ $DELETE_PVC =~ ^[Yy]$ ]]; then
        kubectl delete pvc test-results-pvc sequencer-database-pvc -n $NAMESPACE --ignore-not-found=true
        print_success "PVCs deleted"
    else
        print_status "PVCs preserved (delete manually later if needed)"
    fi
    
    print_success "Cleanup completed"
}

# Main execution
main() {
    echo ""
    print_status "Starting Batching Performance Test K8s Deployment..."
    echo ""
    
    # Execute deployment steps
    check_prerequisites
    check_k8s_connection
    get_docker_image
    create_namespace
    create_test_scripts_configmap
    create_config_files_configmap
    create_storage
    deploy_test_job
    
    echo ""
    print_success "Deployment completed!"
    echo ""
    print_status "Next steps:"
    echo "  1. Monitor logs: kubectl logs -f -n $NAMESPACE -l app=sequencer-batching-test"
    echo "  2. Check status: kubectl get jobs -n $NAMESPACE"
    echo "  3. Get results: kubectl logs job/sequencer-batching-test -n $NAMESPACE"
    echo ""
    
    read -p "Would you like to monitor the test now? (Y/n): " MONITOR
    if [[ ! $MONITOR =~ ^[Nn]$ ]]; then
        monitor_test
    fi
}

# Handle script arguments
case "${1:-}" in
    --help|-h)
        echo "Usage: $0 [OPTIONS]"
        echo ""
        echo "Deploy sequencer batching performance test to Kubernetes"
        echo ""
        echo "This script deploys a pre-built Docker image to K8s."
        echo "NO local Docker build occurs - your disk space is safe!"
        echo ""
        echo "Options:"
        echo "  --help, -h        Show this help message"
        echo "  --monitor         Monitor an already running test"
        echo "  --results         Get results from completed test"
        echo "  --cleanup         Clean up deployment"
        echo ""
        echo "Environment variables:"
        echo "  K8S_NAMESPACE     Kubernetes namespace (default: batching-test)"
        echo "  BLOCKS_TO_SYNC    Number of blocks to test (default: 5000)"
        echo "  BATCH_SIZE        Blocks per batch (default: 100)"
        echo "  DOCKER_IMAGE      Pre-built image URL (skip prompts if set)"
        echo "                    Example: ghcr.io/org/sequencer:tag"
        echo ""
        echo "Examples:"
        echo "  # Interactive mode (asks for image)"
        echo "  ./deploy_batching_test.sh"
        echo ""
        echo "  # With environment variable (no prompts)"
        echo "  DOCKER_IMAGE=ghcr.io/starkware-libs/sequencer:my-tag ./deploy_batching_test.sh"
        echo ""
        echo "  # Monitor existing deployment"
        echo "  ./deploy_batching_test.sh --monitor"
        echo ""
        exit 0
        ;;
    --cleanup)
        cleanup_deployment
        exit 0
        ;;
    --monitor)
        monitor_test
        exit 0
        ;;
    --results)
        get_results
        exit 0
        ;;
    "")
        main
        ;;
    *)
        print_error "Unknown option: $1"
        echo "Use --help for usage information"
        exit 1
        ;;
esac

