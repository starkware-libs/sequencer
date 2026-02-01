#!/bin/bash

# Script to check for deadlock symptoms by comparing lock/flow markers per node.
# Analyzes commit_index, versioned state lock, RPC block_on, and client.request.

LOG_FILE="${1:-/home/arni/workspace/sequencer/sequencer_integration_test_restart.log}"

if [[ ! -f "$LOG_FILE" ]]; then
    echo "Error: Log file not found: $LOG_FILE"
    exit 1
fi

echo "Analyzing: $LOG_FILE"
echo ""
echo "=== TEMPDEBUG100 (trying commit_index lock for get_n_committed_txs) vs TEMPDEBUG101 (acquired) ==="
echo ""

for node in 0 1 2; do
    count_try=$(grep "Node $node distributed_batcher:" "$LOG_FILE" | grep -c "TEMPDEBUG100" 2>/dev/null || echo "0")
    count_done=$(grep "Node $node distributed_batcher:" "$LOG_FILE" | grep -c "TEMPDEBUG101" 2>/dev/null || echo "0")
    count_try=$(echo "$count_try" | tr -d '[:space:]')
    count_done=$(echo "$count_done" | tr -d '[:space:]')
    diff=$((count_try - count_done))

    if [[ $diff -gt 0 ]]; then
        status="STUCK"
    elif [[ $diff -eq 0 ]]; then
        status="OK"
    else
        status="UNEXPECTED"
    fi

    printf "Node %d: try=%d, done=%d, diff=%d => %s\n" "$node" "$count_try" "$count_done" "$diff" "$status"
    if [[ $diff -gt 0 ]]; then
        last_try=$(grep "Node $node distributed_batcher:" "$LOG_FILE" | grep "TEMPDEBUG100 " | tail -n 1)
        if [[ -n $last_try ]]; then
            echo "  Last try: $last_try"
        fi
    fi
done

echo ""
echo "=== TEMPDEBUG102 (trying commit_index lock for commit phase) vs TEMPDEBUG103 (acquired) ==="
echo ""

for node in 0 1 2; do
    count_try=$(grep "Node $node distributed_batcher:" "$LOG_FILE" | grep -c "TEMPDEBUG102" 2>/dev/null || echo "0")
    count_done=$(grep "Node $node distributed_batcher:" "$LOG_FILE" | grep -c "TEMPDEBUG103" 2>/dev/null || echo "0")
    count_try=$(echo "$count_try" | tr -d '[:space:]')
    count_done=$(echo "$count_done" | tr -d '[:space:]')
    diff=$((count_try - count_done))

    if [[ $diff -gt 0 ]]; then
        status="STUCK"
    elif [[ $diff -eq 0 ]]; then
        status="OK"
    else
        status="UNEXPECTED"
    fi

    printf "Node %d: try=%d, done=%d, diff=%d => %s\n" "$node" "$count_try" "$count_done" "$diff" "$status"
    if [[ $diff -gt 0 ]]; then
        last_try=$(grep "Node $node distributed_batcher:" "$LOG_FILE" | grep "TEMPDEBUG102 " | tail -n 1)
        if [[ -n $last_try ]]; then
            echo "  Last try: $last_try"
        fi
    fi
done

echo ""
echo "=== TEMPDEBUG200 (trying versioned state lock) vs TEMPDEBUG201 (acquired) ==="
echo ""

for node in 0 1 2; do
    count_try=$(grep "Node $node distributed_batcher:" "$LOG_FILE" | grep -c "TEMPDEBUG200 " 2>/dev/null || echo "0")
    count_done=$(grep "Node $node distributed_batcher:" "$LOG_FILE" | grep -c "TEMPDEBUG201 " 2>/dev/null || echo "0")
    count_try=$(echo "$count_try" | tr -d '[:space:]')
    count_done=$(echo "$count_done" | tr -d '[:space:]')
    diff=$((count_try - count_done))

    if [[ $diff -gt 0 ]]; then
        status="STUCK"
    elif [[ $diff -eq 0 ]]; then
        status="OK"
    else
        status="UNEXPECTED"
    fi

    printf "Node %d: try=%d, done=%d, diff=%d => %s\n" "$node" "$count_try" "$count_done" "$diff" "$status"
    if [[ $diff -gt 0 ]]; then
        last_try=$(grep "Node $node distributed_batcher:" "$LOG_FILE" | grep "TEMPDEBUG200 " | tail -n 1)
        if [[ -n $last_try ]]; then
            echo "  Last try: $last_try"
        fi
    fi
done

echo ""
echo "=== TEMPDEBUG300 (trying block_on get_executable) vs TEMPDEBUG301 (done) ==="
echo ""

for node in 0 1 2; do
    count_try=$(grep "Node $node distributed_batcher:" "$LOG_FILE" | grep -c "TEMPDEBUG300 " 2>/dev/null || echo "0")
    count_done=$(grep "Node $node distributed_batcher:" "$LOG_FILE" | grep -c "TEMPDEBUG301 " 2>/dev/null || echo "0")
    count_try=$(echo "$count_try" | tr -d '[:space:]')
    count_done=$(echo "$count_done" | tr -d '[:space:]')
    diff=$((count_try - count_done))

    if [[ $diff -gt 0 ]]; then
        status="STUCK"
    elif [[ $diff -eq 0 ]]; then
        status="OK"
    else
        status="UNEXPECTED"
    fi

    printf "Node %d: try=%d, done=%d, diff=%d => %s\n" "$node" "$count_try" "$count_done" "$diff" "$status"
    if [[ $diff -gt 0 ]]; then
        last_try=$(grep "Node $node distributed_batcher:" "$LOG_FILE" | grep "TEMPDEBUG300 " | tail -n 1)
        if [[ -n $last_try ]]; then
            echo "  Last try: $last_try"
        fi
    fi
done

echo ""
echo "=== TEMPDEBUG400 (trying client.request) vs TEMPDEBUG401 (done) ==="
echo ""

for node in 0 1 2; do
    count_try=$(grep "Node $node distributed_batcher:" "$LOG_FILE" | grep -c "TEMPDEBUG400 " 2>/dev/null || echo "0")
    count_done=$(grep "Node $node distributed_batcher:" "$LOG_FILE" | grep -c "TEMPDEBUG401 " 2>/dev/null || echo "0")
    count_try=$(echo "$count_try" | tr -d '[:space:]')
    count_done=$(echo "$count_done" | tr -d '[:space:]')
    diff=$((count_try - count_done))

    if [[ $diff -gt 0 ]]; then
        status="STUCK"
    elif [[ $diff -eq 0 ]]; then
        status="OK"
    else
        status="UNEXPECTED"
    fi

    printf "Node %d: try=%d, done=%d, diff=%d => %s\n" "$node" "$count_try" "$count_done" "$diff" "$status"
    if [[ $diff -gt 0 ]]; then
        last_try=$(grep "Node $node distributed_batcher:" "$LOG_FILE" | grep "TEMPDEBUG400 " | tail -n 1)
        if [[ -n $last_try ]]; then
            echo "  Last try: $last_try"
        fi
    fi
done

echo ""
echo "=== Summary ==="
total_100=$(grep -c "TEMPDEBUG100" "$LOG_FILE" 2>/dev/null || echo "0")
total_101=$(grep -c "TEMPDEBUG101" "$LOG_FILE" 2>/dev/null || echo "0")
total_102=$(grep -c "TEMPDEBUG102" "$LOG_FILE" 2>/dev/null || echo "0")
total_103=$(grep -c "TEMPDEBUG103" "$LOG_FILE" 2>/dev/null || echo "0")
total_200=$(grep -c "TEMPDEBUG200" "$LOG_FILE" 2>/dev/null || echo "0")
total_201=$(grep -c "TEMPDEBUG201" "$LOG_FILE" 2>/dev/null || echo "0")
total_300=$(grep -c "TEMPDEBUG300" "$LOG_FILE" 2>/dev/null || echo "0")
total_301=$(grep -c "TEMPDEBUG301" "$LOG_FILE" 2>/dev/null || echo "0")
total_400=$(grep -c "TEMPDEBUG400" "$LOG_FILE" 2>/dev/null || echo "0")
total_401=$(grep -c "TEMPDEBUG401" "$LOG_FILE" 2>/dev/null || echo "0")

diff_100=$((total_100 - total_101))
diff_102=$((total_102 - total_103))
diff_200=$((total_200 - total_201))
diff_300=$((total_300 - total_301))
diff_400=$((total_400 - total_401))

echo "Total TEMPDEBUG100/101: $total_100 / $total_101 (diff: $((diff_100)))"
echo "Total TEMPDEBUG102/103: $total_102 / $total_103 (diff: $((diff_102)))"
echo "Total TEMPDEBUG200/201: $total_200 / $total_201 (diff: $((diff_200)))"
echo "Total TEMPDEBUG300/301: $total_300 / $total_301 (diff: $((diff_300)))"
echo "Total TEMPDEBUG400/401: $total_400 / $total_401 (diff: $((diff_400)))"

# echo ""


# if [[ $diff_100 -gt 0 ]]; then
#     echo "*** DEADLOCK: $diff_100 stuck waiting for commit_index lock (get_n_committed_txs) ***"
# fi
# if [[ $diff_102 -gt 0 ]]; then
#     echo "*** DEADLOCK: $diff_102 stuck waiting for commit_i  Last try: Node 1 distributed_batcher: TEMPDEBUG400 [thread: ThreadId(20)] trying to requestndex lock (commit phase) ***"
# fi
# if [[ $diff_200 -gt 0 ]]; then
#     echo "*** DEADLOCK: $diff_200 stuck waiting for versioned state lock ***"
# fi
# if [[ $diff_300 -gt 0 ]]; then
#     echo "*** DEADLOCK: $diff_300 stuck in block_on get_executable ***"
# fi
# if [[ $diff_400 -gt 0 ]]; then
#     echo "*** DEADLOCK: $diff_400 pending client.request(s) ***"
# fi
