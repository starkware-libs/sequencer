#!/bin/bash

set -e

echo "=========================================="
echo "Setting up 200K V2 Unified Queue Test"
echo "New Design: ALL streams → middle_queue"
echo "=========================================="
echo ""

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== V2 UNIFIED QUEUE DESIGN ===${NC}"
echo "Changes from old implementation:"
echo "  ✓ ALL 5 streams go through middle_queue (FuturesOrdered)"
echo "  ✓ Removed HashMap out-of-order buffering"
echo "  ✓ Removed try_flush_consecutive_batch() (~93 lines)"
echo "  ✓ Removed next_expected_compilation_block tracking"
echo "  ✓ Automatic ordering via FuturesOrdered"
echo "  ✓ Eliminated OOM risk"
echo "  ✓ Eliminated marker mismatch issues"
echo ""

echo -e "${YELLOW}Step 1: Create namespace${NC}"
kubectl create namespace dean-batching-200k-v2-unified --dry-run=client -o yaml | kubectl apply -f -
echo -e "${GREEN}✓ Namespace created${NC}"
echo ""

echo -e "${YELLOW}Step 2: Copy ConfigMaps from dean-batching namespace${NC}"
# Copy sequencer-configs
kubectl get configmap sequencer-configs -n dean-batching -o yaml | \
  sed 's/namespace: dean-batching/namespace: dean-batching-200k-v2-unified/' | \
  sed '/resourceVersion:/d' | \
  sed '/uid:/d' | \
  sed '/creationTimestamp:/d' | \
  kubectl apply -f -

# Copy test scripts
kubectl get configmap dean-test-scripts -n dean-batching -o yaml | \
  sed 's/namespace: dean-batching/namespace: dean-batching-200k-v2-unified/' | \
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
kubectl apply -f pvc-200k-v2-unified.yaml
echo -e "${GREEN}✓ PVC created${NC}"
echo ""

echo -e "${YELLOW}Step 5: Wait for PVC to be bound...${NC}"
kubectl wait --for=condition=bound pvc/dean-200k-v2-unified-pvc -n dean-batching-200k-v2-unified --timeout=120s
echo -e "${GREEN}✓ PVC bound${NC}"
echo ""

echo -e "${YELLOW}Step 6: Create and start the test job${NC}"
echo -e "${RED}NOTE: Make sure you updated the image in batching-test-200k-v2-unified.yaml!${NC}"
echo -e "${RED}      Look for IMAGE_PLACEHOLDER_UPDATE_THIS${NC}"
echo ""
read -p "Press Enter to continue (or Ctrl+C to abort)..."
kubectl apply -f batching-test-200k-v2-unified.yaml
echo -e "${GREEN}✓ Job created${NC}"
echo ""

echo "=========================================="
echo -e "${GREEN}V2 Unified Queue Test Setup Complete!${NC}"
echo "=========================================="
echo ""
echo -e "${BLUE}What's different in V2:${NC}"
echo "  • ProcessedBlockData enum for all stream types"
echo "  • Single middle_queue (FuturesOrdered) for ALL streams"
echo "  • Immediate futures for non-compilation blocks"
echo "  • Async futures for compilation blocks"
echo "  • Simple batch_queue accumulation"
echo "  • ~90 lines of code removed"
echo ""
echo "Your tests:"
echo "  - OLD test (200k-fixed): dean-batching-200k-fixed"
echo "  - NEW test (v2-unified): dean-batching-200k-v2-unified"
echo ""
echo "Monitor the V2 test with:"
echo "  kubectl logs -f -n dean-batching-200k-v2-unified -l app=batching-test-200k-v2-unified"
echo ""
echo "Check pod status:"
echo "  kubectl get pods -n dean-batching-200k-v2-unified"
echo ""
echo "Check all tests:"
echo "  kubectl get pods -n dean-batching-200k-fixed      # Old design"
echo "  kubectl get pods -n dean-batching-200k-v2-unified # New design"
echo ""
echo "Compare results when both finish!"
echo ""

