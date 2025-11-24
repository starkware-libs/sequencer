#!/bin/bash

set -e

echo "=========================================="
echo "Setting up 200K Fixed Test (Separate from 500K test)"
echo "=========================================="
echo ""

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${YELLOW}Step 1: Create namespace${NC}"
kubectl create namespace dean-batching-200k-fixed --dry-run=client -o yaml | kubectl apply -f -
echo -e "${GREEN}✓ Namespace created${NC}"
echo ""

echo -e "${YELLOW}Step 2: Copy ConfigMaps from dean-batching namespace${NC}"
# Copy sequencer-configs
kubectl get configmap sequencer-configs -n dean-batching -o yaml | \
  sed 's/namespace: dean-batching/namespace: dean-batching-200k-fixed/' | \
  sed '/resourceVersion:/d' | \
  sed '/uid:/d' | \
  sed '/creationTimestamp:/d' | \
  kubectl apply -f -

# Copy test scripts
kubectl get configmap dean-test-scripts -n dean-batching -o yaml | \
  sed 's/namespace: dean-batching/namespace: dean-batching-200k-fixed/' | \
  sed '/resourceVersion:/d' | \
  sed '/uid:/d' | \
  sed '/creationTimestamp:/d' | \
  kubectl apply -f -

echo -e "${GREEN}✓ ConfigMaps copied${NC}"
echo ""

echo -e "${YELLOW}Step 3: Ensure StorageClass exists${NC}"
kubectl apply -f storage-class-balanced-1tb.yaml
echo -e "${GREEN}✓ StorageClass ready${NC}"
echo ""

echo -e "${YELLOW}Step 4: Create PVC (500GB)${NC}"
kubectl apply -f pvc-200k-fixed.yaml
echo -e "${GREEN}✓ PVC created${NC}"
echo ""

echo -e "${YELLOW}Step 5: Wait for PVC to be bound...${NC}"
kubectl wait --for=condition=bound pvc/dean-200k-fixed-pvc -n dean-batching-200k-fixed --timeout=120s
echo -e "${GREEN}✓ PVC bound${NC}"
echo ""

echo -e "${YELLOW}Step 6: Create and start the test job${NC}"
kubectl apply -f batching-test-200k-fixed.yaml
echo -e "${GREEN}✓ Job created${NC}"
echo ""

echo "=========================================="
echo -e "${GREEN}Test Setup Complete!${NC}"
echo "=========================================="
echo ""
echo "Your NEW 200K test is now starting in a separate pod."
echo "Your OLD 500K test continues running undisturbed."
echo ""
echo "Monitor the NEW test with:"
echo "  kubectl logs -f -n dean-batching-200k-fixed -l app=batching-test-200k-fixed"
echo ""
echo "Check pod status:"
echo "  kubectl get pods -n dean-batching-200k-fixed"
echo ""
echo "Check both tests:"
echo "  kubectl get pods -n dean-batching              # Old 500K test"
echo "  kubectl get pods -n dean-batching-200k-fixed   # New 200K test"
echo ""

