#!/bin/bash

# Cleanup script for batching test artifacts
# Removes test data directories, log files, and stops running nodes

set -e

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

echo ""
echo -e "${BLUE}=========================================${NC}"
echo -e "${BLUE}Batching Test Cleanup Utility${NC}"
echo -e "${BLUE}=========================================${NC}"
echo ""

# Function to show size of directory if it exists
show_size() {
    local dir=$1
    if [ -d "$dir" ]; then
        local size=$(du -sh "$dir" 2>/dev/null | cut -f1)
        echo -e "${YELLOW}  $dir: $size${NC}"
        return 0
    fi
    return 1
}

# Function to show size of file if it exists
show_file_size() {
    local file=$1
    if [ -f "$file" ]; then
        local size=$(du -sh "$file" 2>/dev/null | cut -f1)
        echo -e "${YELLOW}  $file: $size${NC}"
        return 0
    fi
    return 1
}

# Check for running nodes
echo "Checking for running nodes..."
if pgrep -f apollo_node > /dev/null; then
    echo -e "${YELLOW}Found running apollo_node processes${NC}"
    read -p "Stop running nodes? (y/N): " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        pkill -f apollo_node
        sleep 2
        echo -e "${GREEN}✓ Nodes stopped${NC}"
    fi
else
    echo -e "${GREEN}✓ No running nodes${NC}"
fi
echo ""

# Check for test data directories
echo "Checking for test data directories..."
found_dirs=0
show_size "./data_compare_batching" && found_dirs=1
show_size "./data_compare_no_batching" && found_dirs=1
show_size "./data_with_batching" && found_dirs=1
show_size "./data_without_batching" && found_dirs=1

if [ $found_dirs -eq 0 ]; then
    echo -e "${GREEN}✓ No test data directories found${NC}"
else
    echo ""
    echo -e "${RED}WARNING: This will permanently delete the above directories!${NC}"
    read -p "Delete test data directories? (y/N): " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -rf ./data_compare_batching ./data_compare_no_batching ./data_with_batching ./data_without_batching 2>/dev/null || true
        echo -e "${GREEN}✓ Test data directories deleted${NC}"
    else
        echo -e "${YELLOW}Skipped${NC}"
    fi
fi
echo ""

# Check for log files
echo "Checking for log files..."
found_logs=0
show_file_size "batching_test.log" && found_logs=1
show_file_size "test_batching_on.log" && found_logs=1
show_file_size "test_batching_off.log" && found_logs=1
show_file_size "test_WITH_BATCHING.log" && found_logs=1
show_file_size "test_WITHOUT_BATCHING.log" && found_logs=1
show_file_size "test_with_batching.log" && found_logs=1
show_file_size "test_without_batching.log" && found_logs=1

if [ $found_logs -eq 0 ]; then
    echo -e "${GREEN}✓ No test log files found${NC}"
else
    echo ""
    read -p "Delete test log files? (y/N): " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -f batching_test.log test_batching_on.log test_batching_off.log \
              test_WITH_BATCHING.log test_WITHOUT_BATCHING.log \
              test_with_batching.log test_without_batching.log \
              time_*.txt 2>/dev/null || true
        echo -e "${GREEN}✓ Log files deleted${NC}"
    else
        echo -e "${YELLOW}Skipped${NC}"
    fi
fi
echo ""

# Check for temp config
if [ -f "/tmp/config_backup.json" ]; then
    echo "Found temporary config backup at /tmp/config_backup.json"
    read -p "Delete? (y/N): " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -f /tmp/config_backup.json
        echo -e "${GREEN}✓ Temp config deleted${NC}"
    fi
    echo ""
fi

# Show final disk space
echo -e "${BLUE}=========================================${NC}"
echo -e "${BLUE}Current Disk Space:${NC}"
df -h . | tail -1
echo ""

echo -e "${GREEN}✓ Cleanup complete!${NC}"
echo ""
echo "If you need to clean up K8s resources, run:"
echo "  cd deployments/batching-test && ./cleanup.sh"
echo ""


