#!/bin/bash

# Monitor 200k sync with timing from the beginning

set -e

POD_NAME=$(kubectl get pods -n batching-test -l app=sequencer-sync -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || echo "")

if [ -z "$POD_NAME" ]; then
  echo "❌ No sync pod found!"
  exit 1
fi

# Job start time (from Kubernetes)
JOB_START_TIME="2025-11-13T09:42:07Z"
START_TIMESTAMP=$(date -d "$JOB_START_TIME" +%s 2>/dev/null || date -j -f "%Y-%m-%dT%H:%M:%SZ" "$JOB_START_TIME" +%s)

TARGET_BLOCK=200000

while true; do
  clear
  echo "========================================="
  echo "    HYPERDISK SYNC TO 200K - TIMING"
  echo "========================================="
  echo ""
  
  # Current time
  CURRENT_TIME=$(date +%s)
  ELAPSED=$((CURRENT_TIME - START_TIMESTAMP))
  ELAPSED_MINS=$((ELAPSED / 60))
  ELAPSED_HOURS=$((ELAPSED_MINS / 60))
  REMAINING_MINS=$((ELAPSED_MINS % 60))
  
  # Get current block
  CURRENT_BLOCK=$(kubectl logs -n batching-test $POD_NAME --tail=100 | grep -oE 'height[^0-9]*[0-9]+' | grep -oE '[0-9]+' | tail -1)
  
  if [ -n "$CURRENT_BLOCK" ]; then
    PERCENT=$(echo "scale=2; $CURRENT_BLOCK * 100 / $TARGET_BLOCK" | bc)
    
    # Calculate speed and ETA
    if [ $ELAPSED -gt 0 ]; then
      BLOCKS_PER_SEC=$(echo "scale=2; $CURRENT_BLOCK / $ELAPSED" | bc)
      REMAINING_BLOCKS=$((TARGET_BLOCK - CURRENT_BLOCK))
      
      if [ $(echo "$BLOCKS_PER_SEC > 0" | bc) -eq 1 ]; then
        ETA_SECS=$(echo "scale=0; $REMAINING_BLOCKS / $BLOCKS_PER_SEC" | bc)
        ETA_MINS=$((ETA_SECS / 60))
        ETA_HOURS=$((ETA_MINS / 60))
        ETA_MINS_MOD=$((ETA_MINS % 60))
        
        echo "📊 PROGRESS:"
        echo "   Current block: $CURRENT_BLOCK / $TARGET_BLOCK"
        echo "   Progress: $PERCENT%"
        echo ""
        echo "⏱️  TIMING (from job start):"
        echo "   Job started: $JOB_START_TIME"
        echo "   Elapsed: ${ELAPSED_HOURS}h ${REMAINING_MINS}m"
        echo "   Speed: $BLOCKS_PER_SEC blocks/sec"
        echo "   ETA to 200k: ~${ETA_HOURS}h ${ETA_MINS_MOD}m"
        echo ""
        echo "💾 STORAGE:"
        DB_SIZE=$(kubectl exec -n batching-test $POD_NAME -- du -sh /data_compare_batching/workspace/SN_MAIN 2>/dev/null | cut -f1 || echo "N/A")
        echo "   Database size: $DB_SIZE"
        echo ""
        echo "🎯 ESTIMATE:"
        TOTAL_TIME_MINS=$(echo "scale=0; $ELAPSED_MINS + $ETA_MINS" | bc)
        TOTAL_HOURS=$((TOTAL_TIME_MINS / 60))
        TOTAL_MINS_MOD=$((TOTAL_TIME_MINS % 60))
        echo "   Total time to 200k: ~${TOTAL_HOURS}h ${TOTAL_MINS_MOD}m"
      fi
    fi
  else
    echo "Status: Syncing... (waiting for block data)"
  fi
  
  echo ""
  echo "========================================="
  echo "Updated: $(date)"
  echo "Press Ctrl+C to stop monitoring"
  echo "========================================="
  
  sleep 30
done

