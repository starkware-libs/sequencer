#!/bin/bash
# Deploy Storage Batching Test to Kubernetes
set -e

NAMESPACE="dean-batching"

echo "========================================="
echo "STORAGE BATCHING K8S DEPLOYMENT"
echo "========================================="
echo ""

# Check if kubectl is installed
if ! command -v kubectl &> /dev/null; then
    echo "ERROR: kubectl is not installed"
    exit 1
fi

# Check if connected to k8s cluster
if ! kubectl cluster-info &> /dev/null; then
    echo "ERROR: Not connected to Kubernetes cluster"
    echo "Run: gcloud container clusters get-credentials sequencer-dev --region us-central1 --project starkware-dev"
    exit 1
fi

echo "✓ Connected to: $(kubectl config current-context)"
echo ""

# Create namespace
echo "Creating namespace..."
kubectl create namespace $NAMESPACE --dry-run=client -o yaml | kubectl apply -f -

# Create ConfigMap with test script
echo "Creating test script ConfigMap..."
kubectl create configmap dean-test-scripts \
    --from-file=test_batching.sh=../../test_batching.sh \
    --namespace=$NAMESPACE \
    --dry-run=client -o yaml | kubectl apply -f -

# Create ConfigMap with node configs
echo "Creating node config ConfigMap..."
cd ../..
kubectl create configmap sequencer-configs \
    --from-file=crates/apollo_deployments/resources/app_configs/ \
    --from-file=mainnet_deployment=crates/apollo_deployments/resources/deployments/mainnet/deployment_config_override.json \
    --from-file=mainnet_hybrid=crates/apollo_deployments/resources/deployments/mainnet/hybrid_0.json \
    --from-file=node_config=crates/apollo_deployments/resources/services/consolidated/node.json \
    --from-file=mainnet_secrets.json=crates/apollo_deployments/resources/mainnet_secrets.json \
    --namespace=$NAMESPACE \
    --dry-run=client -o yaml | kubectl apply -f -
cd deployments/dean-batching

# Apply k8s resources
echo "Creating storage class..."
kubectl apply -f storage.yaml

echo "Creating PVC (1TB disk)..."
kubectl apply -f pvc.yaml

echo "Creating test job..."
kubectl apply -f batching-test.yaml

echo ""
echo "========================================="
echo "DEPLOYMENT COMPLETE"
echo "========================================="
echo ""
echo "Monitor the test:"
echo "  kubectl logs -f -n $NAMESPACE -l app=batching-test"
echo ""
echo "Check status:"
echo "  kubectl get pods -n $NAMESPACE"
echo "  kubectl get pvc -n $NAMESPACE"
echo ""
echo "Delete everything:"
echo "  kubectl delete namespace $NAMESPACE"
echo ""
