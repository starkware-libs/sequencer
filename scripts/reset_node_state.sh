#!/bin/bash

# Script to reset node state and start syncing from scratch
set -e

echo "Resetting node state to start fresh sync from block 0!"
echo "This will delete all synced data and start from block 0!"
echo ""

read -p "Are you sure you want to reset? (y/N): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Reset cancelled."
    exit 1
fi

echo "Stopping any running node processes..."
pkill -f "apollo_node" || true
sleep 2

echo "Backing up current state (just in case)..."
BACKUP_TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Backup state sync data
if [ -d "/data/state_sync/SN_SEPOLIA" ]; then
    sudo mv /data/state_sync/SN_SEPOLIA /data/state_sync/SN_SEPOLIA.backup.${BACKUP_TIMESTAMP}
    echo "State sync backup created"
fi

# Backup batcher data  
if [ -d "/data/batcher" ]; then
    sudo mv /data/batcher /data/batcher.backup.${BACKUP_TIMESTAMP}
    echo "Batcher backup created"
fi

# Backup class manager data
if [ -d "/data/class_manager" ]; then
    sudo mv /data/class_manager /data/class_manager.backup.${BACKUP_TIMESTAMP}
    echo "Class manager backup created"
fi

echo "Removing ALL storage data..."
sudo rm -rf /data/state_sync/SN_SEPOLIA/
sudo rm -rf /data/batcher/
sudo rm -rf /data/class_manager/

echo "Recreating storage directories with correct permissions..."
sudo mkdir -p /data/{batcher,class_manager,state_sync}
sudo chown -R $USER:$USER /data/{batcher,class_manager,state_sync}
sudo chmod 755 /data/{batcher,class_manager,state_sync}
echo "Complete storage reset finished!"

echo ""
echo "Node will now start syncing from block 0 on next run!"
echo "   Run: ./scripts/run_sepolia_node.sh"
echo ""
echo "To restore backups later (use the same timestamp):"
echo "   sudo mv /data/state_sync/SN_SEPOLIA.backup.TIMESTAMP /data/state_sync/SN_SEPOLIA"
echo "   sudo mv /data/batcher.backup.TIMESTAMP /data/batcher"
echo "   sudo mv /data/class_manager.backup.TIMESTAMP /data/class_manager"
echo ""
echo "This reset ensures:"
echo "   Consensus starts from block 0 (not block 10540!)"
echo "   Batcher starts fresh and will show logs immediately"
echo "   All components are synchronized from genesis"
