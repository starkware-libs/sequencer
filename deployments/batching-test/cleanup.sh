#!/bin/bash

# Sequencer Batching Test- Cleanup Script
set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Configuration
NAMESPACE="${K8S_NAMESPACE:-default}"

echo -e "${BLUE}=========================================${NC}"
echo -e "${BLUE}Sequencer Batching Test Cleanup${NC}"
echo -e "${BLUE}=========================================${NC}"
echo ""

# Confirm cleanup
echo -e "${YELLOW}This will delete:${NC}"
echo "  - Job: sequencer-batching-test"
echo "  - ConfigMap: batching-test-scripts"
echo ""
echo -e "${RED}Optional (you'll be asked):${NC}"
echo "  - PVC: test-results-pvc (contains test results)"
echo "  - PVC: sequencer-database-pvc (contains database)"
echo ""

read -p "Continue with cleanup? (y/N) " -n 1 -r
echo ""
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Cleanup cancelled"
    exit 0
fi

# Delete job
echo -e "${BLUE}Deleting job...${NC}"
kubectl delete job sequencer-batching-test -n $NAMESPACE --ignore-not-found=true
echo -e "${GREEN}✓ Job deleted${NC}"

# Delete configmap
echo -e "${BLUE}Deleting configmap...${NC}"
kubectl delete configmap batching-test-scripts -n $NAMESPACE --ignore-not-found=true
echo -e "${GREEN}✓ ConfigMap deleted${NC}"

# Ask about PVCs
echo ""
echo -e "${YELLOW}Delete PVCs?${NC}"
echo -e "${RED}WARNING: This will delete all test data and results!${NC}"
read -p "Delete PVCs? (y/N) " -n 1 -r
echo ""
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo -e "${BLUE}Deleting PVCs...${NC}"
    kubectl delete pvc test-results-pvc -n $NAMESPACE --ignore-not-found=true
    kubectl delete pvc sequencer-database-pvc -n $NAMESPACE --ignore-not-found=true
    echo -e "${GREEN}✓ PVCs deleted${NC}"
else
    echo -e "${YELLOW}PVCs preserved${NC}"
    echo "You can delete them manually later with:"
    echo "  kubectl delete pvc test-results-pvc sequencer-database-pvc -n $NAMESPACE"
fi

echo ""
echo -e "${BLUE}=========================================${NC}"
echo -e "${GREEN}Cleanup complete!${NC}"
echo -e "${BLUE}=========================================${NC}"

