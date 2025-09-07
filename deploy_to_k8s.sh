#!/bin/bash
# Kubernetes Deployment Script for Sequencer Node.
set -e

# Colors for output.
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

# Check if required tools are installed.
check_prerequisites() {
    print_status "Checking prerequisites..."
    if ! command -v kubectl &> /dev/null; then
        print_error "kubectl is not installed. Please install kubectl first."
        exit 1
    fi
    if ! command -v docker &> /dev/null; then
        print_error "docker is not installed. Please install Docker first."
        exit 1
    fi
    if ! command -v gcloud &> /dev/null; then
        print_warning "gcloud CLI not found. You may need it for GKE clusters."
    fi
    print_success "Prerequisites check completed"
}

# Check kubectl connection.
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

# Build Docker image.
build_docker_image() {
    print_status "Building Docker image for sequencer..."
    
    # Prompt for image tag.
    read -p "Enter Docker image tag (default: latest): " IMAGE_TAG
    IMAGE_TAG=${IMAGE_TAG:-latest}
    
    # Build the image.
    print_status "Building sequencer image with tag: $IMAGE_TAG"
    
    if docker build -f deployments/images/sequencer/Dockerfile -t sequencer:$IMAGE_TAG .; then
        print_success "Docker image built successfully: sequencer:$IMAGE_TAG"
    else
        print_error "Failed to build Docker image"
        exit 1
    fi
    
    # For cloud deployment, you might need to push to a registry.
    read -p "Do you want to push to a Docker registry? (y/N): " PUSH_IMAGE
    if [[ $PUSH_IMAGE =~ ^[Yy]$ ]]; then
        # Auto-detect registry from current kubectl context
        CURRENT_PROJECT=$(kubectl config current-context | grep -o 'gke_[^_]*_[^_]*_[^_]*' | cut -d'_' -f2)
        if [ ! -z "$CURRENT_PROJECT" ]; then
            REGISTRY_URL="gcr.io/$CURRENT_PROJECT"
            print_status "Auto-detected registry: $REGISTRY_URL"
        else
            read -p "Enter registry URL (e.g., gcr.io/PROJECT_ID): " REGISTRY_URL
        fi
        if [ ! -z "$REGISTRY_URL" ]; then
            # Ensure Docker is authenticated with Google Container Registry
            print_status "Configuring Docker authentication for Google Container Registry..."
            gcloud auth configure-docker --quiet
            
            docker tag sequencer:$IMAGE_TAG $REGISTRY_URL/sequencer:$IMAGE_TAG
            docker push $REGISTRY_URL/sequencer:$IMAGE_TAG
            print_success "Image pushed to registry: $REGISTRY_URL/sequencer:$IMAGE_TAG"
            IMAGE_NAME="$REGISTRY_URL/sequencer:$IMAGE_TAG"
        fi
    else
        IMAGE_NAME="sequencer:$IMAGE_TAG"
    fi
}

# Update configuration files.
update_configs() {
    print_status "Updating configuration files..."
    
    # Create a temporary directory for modified configs.
    TEMP_CONFIG_DIR="./k8s_configs_temp"
    mkdir -p $TEMP_CONFIG_DIR
    
    # Copy original configs.
    cp -r deployments/sequencer/* $TEMP_CONFIG_DIR/
    
    # Update image name in deployment files.
    if [ ! -z "$IMAGE_NAME" ]; then
        print_status "Updating deployment files with image: $IMAGE_NAME"
        find $TEMP_CONFIG_DIR -name "*.yaml" -exec sed -i "s|image: .*sequencer.*|image: $IMAGE_NAME|g" {} \;
    fi
    
    print_success "Configuration files updated in $TEMP_CONFIG_DIR"
}

# Inject secrets.       
inject_secrets() {
    print_status "Injecting secrets into configuration files..."
    
    # Check if the secrets injector exists.
    if [ ! -f "scripts/system_tests/config_secrets_injector.py" ]; then
        print_error "Secrets injector script not found!"
        exit 1
    fi
    
    # Run the secrets injector.
    if python3 scripts/system_tests/config_secrets_injector.py $TEMP_CONFIG_DIR; then
        print_success "Secrets injected successfully"
    else
        print_error "Failed to inject secrets"
        exit 1
    fi
}

# Deploy to Kubernetes.
deploy_to_k8s() {
    print_status "Deploying to Kubernetes..."
    
    # Create namespace if it doesn't exist.
    kubectl create namespace sequencer --dry-run=client -o yaml | kubectl apply -f -
    
    # Apply all configuration files.
    print_status "Applying Kubernetes configurations..."
    
    if kubectl apply -f $TEMP_CONFIG_DIR/ -n sequencer; then
        print_success "Deployment applied successfully"
    else
        print_error "Failed to apply deployment"
        exit 1
    fi
    
    # Wait for deployment to be ready.
    print_status "Waiting for deployment to be ready..."
    kubectl wait --for=condition=available --timeout=300s deployment/sequencer -n sequencer
    
    print_success "Sequencer deployed successfully!"
}

# Monitor deployment.
monitor_deployment() {
    print_status "Monitoring deployment status..."
    
    echo ""
    echo "Deployment Status:"
    kubectl get pods -n sequencer
    
    echo ""
    echo "Services:"
    kubectl get services -n sequencer
    
    echo ""
    echo "Recent logs:"
    POD_NAME=$(kubectl get pods -n sequencer -o jsonpath='{.items[0].metadata.name}')
    kubectl logs $POD_NAME -n sequencer --tail=20
    
    echo ""
    print_status "To view live logs: kubectl logs -f $POD_NAME -n sequencer"
    print_status "To get shell access: kubectl exec -it $POD_NAME -n sequencer -- /bin/bash"
}

# Performance monitoring setup.
setup_performance_monitoring() {
    print_status "Setting up performance monitoring..."
    
    echo ""
    print_status "Your concurrent flush metrics will be available at:"
    echo "  - storage_file_handler_flush_latency_seconds (production metric)"
    echo "  - Check Grafana dashboards for flush performance"
    
    # Get service endpoints.
    print_status "Service endpoints:"
    kubectl get services -n sequencer -o wide
    
    echo ""
    print_warning "To access metrics:"
    echo "  1. Port-forward to metrics endpoint: kubectl port-forward svc/sequencer-metrics 9090:9090 -n sequencer"
    echo "  2. Access metrics at: http://localhost:9090/metrics"
    echo "  3. Look for 'storage_file_handler_flush_latency_seconds' metrics"
}

# Cleanup function.
cleanup() {
    print_status "Cleaning up temporary files..."
    rm -rf $TEMP_CONFIG_DIR
}

# Main execution.
main() {
    echo ""
    print_status "Starting Kubernetes deployment process..."
    
    # Set trap for cleanup.
    trap cleanup EXIT
    
    # Execute deployment steps.     
    check_prerequisites
    check_k8s_connection
    build_docker_image
    update_configs
    inject_secrets
    deploy_to_k8s
    monitor_deployment
    setup_performance_monitoring
    
    echo ""
    print_success "Deployment completed successfully!"
    print_status "Your sequencer with concurrent flush optimization is now running on Kubernetes!"
    
    echo ""
    print_status "Next steps:"
    echo "  1. Monitor the logs for flush performance"
    echo "  2. Check Grafana for 'storage_file_handler_flush_latency_seconds' metrics"
    echo "  3. Compare performance before/after your concurrent optimization"
    echo "  4. Run: kubectl port-forward svc/sequencer-metrics 9090:9090 -n sequencer"
    echo "  5. Visit: http://localhost:9090/metrics to see raw metrics"
}

# Handle script arguments.
case "${1:-}" in
    --help|-h)
        echo "Usage: $0 [OPTIONS]"
        echo ""
        echo "Deploy sequencer to Kubernetes with concurrent flush optimization"
        echo ""
        echo "Options:"
        echo "  --help, -h     Show this help message"
        echo "  --cleanup      Clean up deployment"
        echo ""
        exit 0
        ;;
    --cleanup)
        print_status "Cleaning up Kubernetes deployment..."
        kubectl delete namespace sequencer --ignore-not-found=true
        print_success "Cleanup completed"
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
